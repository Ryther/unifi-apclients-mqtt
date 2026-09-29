use std::collections::HashSet;
use std::future::Future;
use std::time::Duration;

use tokio::time::MissedTickBehavior;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use unifi_apclients_mqtt::{
    config::Config,
    mqtt::{MqttPublisher, run_event_loop},
    unifi::{SnapshotBatch, UniFiClient},
};

trait SnapshotSource {
    async fn fetch_snapshots(&mut self, ap_macs: &[String]) -> Result<SnapshotBatch, String>;
}

impl SnapshotSource for UniFiClient {
    async fn fetch_snapshots(&mut self, ap_macs: &[String]) -> Result<SnapshotBatch, String> {
        UniFiClient::fetch_snapshots(self, ap_macs)
            .await
            .map_err(|error| error.to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init()?;

    let config = Config::from_env(std::env::vars())?;
    let mut unifi = UniFiClient::new(&config)?;
    let (publisher, event_loop) = MqttPublisher::new(&config);
    run_service(
        config,
        publisher,
        event_loop,
        &mut unifi,
        tokio::signal::ctrl_c(),
    )
    .await
}

async fn run_service<Source, ShutdownFuture>(
    config: Config,
    publisher: MqttPublisher,
    event_loop: rumqttc::EventLoop,
    source: &mut Source,
    shutdown: ShutdownFuture,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    Source: SnapshotSource,
    ShutdownFuture: Future<Output = Result<(), std::io::Error>>,
{
    let event_publisher = publisher.clone();
    let mqtt_task = tokio::spawn(run_event_loop(event_loop, event_publisher));

    info!(
        ap_count = config.ap_macs.len(),
        poll_seconds = config.poll_interval.as_secs(),
        tls_insecure = config.unifi_tls_insecure,
        "UniFi AP client poller started"
    );

    let mut ticker = tokio::time::interval(config.poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    tokio::pin!(shutdown);
    let mut discovered_aps = HashSet::new();

    loop {
        tokio::select! {
            result = &mut shutdown => {
                result?;
                break;
            }
            _ = ticker.tick() => {
                match source.fetch_snapshots(&config.ap_macs).await {
                    Ok(batch) => {
                        let new_snapshots = batch
                            .snapshots
                            .iter()
                            .filter(|snapshot| !discovered_aps.contains(&snapshot.ap_mac))
                            .cloned()
                            .collect::<Vec<_>>();
                        if !new_snapshots.is_empty() {
                            match publisher.publish_discovery(&new_snapshots).await {
                                Ok(()) => {
                                    discovered_aps.extend(
                                        new_snapshots.iter().map(|snapshot| snapshot.ap_mac.clone()),
                                    );
                                }
                                Err(error) => warn!(%error, "could not publish Home Assistant discovery"),
                            }
                        }
                        if let Err(error) = publisher.publish_snapshots(&batch.snapshots).await {
                            warn!(%error, "could not publish AP client snapshots");
                        }
                        if let Err(error) = publisher.publish_unavailable(&batch.unavailable_aps).await {
                            warn!(%error, "could not publish unavailable AP status");
                        }
                    }
                    Err(error) => {
                        warn!(%error, "UniFi poll failed; preserving the last retained snapshots");
                        if let Err(error) = publisher.publish_unavailable(&config.ap_macs).await {
                            warn!(%error, "could not publish AP unavailable status");
                        }
                    }
                }
            }
        }
    }

    info!("shutting down UniFi AP client poller");
    if let Err(error) = publisher.publish_unavailable(&config.ap_macs).await {
        error!(%error, "could not publish AP shutdown status");
    }
    if let Err(error) = publisher.publish_offline().await {
        error!(%error, "could not publish service shutdown status");
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    if let Err(error) = publisher.disconnect().await {
        error!(%error, "could not disconnect MQTT client cleanly");
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    mqtt_task.abort();
    let _ = tokio::time::timeout(Duration::from_secs(2), mqtt_task).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::oneshot;
    use unifi_apclients_mqtt::{
        config::Config,
        mapper::{ApSnapshot, ClientSnapshot},
        mqtt::MqttPublisher,
        unifi::{SnapshotBatch, UniFiClient},
    };

    use super::SnapshotSource;
    use super::run_service;

    struct FixtureSource {
        result: Option<Result<SnapshotBatch, String>>,
        polled_sender: Option<oneshot::Sender<()>>,
    }

    impl SnapshotSource for FixtureSource {
        async fn fetch_snapshots(&mut self, _ap_macs: &[String]) -> Result<SnapshotBatch, String> {
            if let Some(sender) = self.polled_sender.take() {
                let _ = sender.send(());
            }
            self.result
                .take()
                .expect("fixture source should be polled once")
        }
    }

    const AP_MAC: &str = "11:22:33:44:55:66";
    const MISSING_AP_MAC: &str = "22:33:44:55:66:77";

    fn test_config() -> Config {
        Config {
            unifi_url: "https://controller.example.test:8443".to_owned(),
            unifi_username: "fixture-user".to_owned(),
            unifi_password: "fixture-secret".to_owned(),
            ap_macs: vec![AP_MAC.to_owned(), MISSING_AP_MAC.to_owned()],
            poll_interval: Duration::from_secs(60),
            unifi_tls_insecure: false,
            mqtt_host: "127.0.0.1".to_owned(),
            mqtt_port: 1,
            mqtt_username: None,
            mqtt_password: None,
            mqtt_base_topic: "unifi/apclients".to_owned(),
            homeassistant_discovery_enabled: true,
            homeassistant_discovery_prefix: "homeassistant".to_owned(),
            homeassistant_status_topic: "homeassistant/status".to_owned(),
        }
    }

    fn sample_batch() -> SnapshotBatch {
        SnapshotBatch {
            snapshots: vec![ApSnapshot {
                available: true,
                ap_mac: AP_MAC.to_owned(),
                ap_name: "Fixture AP".to_owned(),
                clients: vec![ClientSnapshot {
                    mac: "AA:AA:AA:AA:AA:01".to_owned(),
                    name: "fixture-client".to_owned(),
                    hostname: Some("fixture-client".to_owned()),
                    ip: Some("192.0.2.21".to_owned()),
                }],
            }],
            unavailable_aps: vec![MISSING_AP_MAC.to_owned()],
        }
    }

    #[tokio::test]
    async fn publishes_discovery_snapshots_and_partial_unavailability_before_shutdown() {
        let config = test_config();
        let (publisher, event_loop) = MqttPublisher::new(&config);
        let (polled_sender, polled_receiver) = oneshot::channel();
        let batch = sample_batch();
        let mut source = FixtureSource {
            result: Some(Ok(batch)),
            polled_sender: Some(polled_sender),
        };

        let result = run_service(config, publisher, event_loop, &mut source, async move {
            polled_receiver.await.map_err(std::io::Error::other)?;
            Ok(())
        })
        .await;

        assert!(result.is_ok(), "successful poll should shut down cleanly");
    }

    #[tokio::test]
    async fn marks_all_aps_unavailable_when_the_poll_fails() {
        let config = test_config();
        let (publisher, event_loop) = MqttPublisher::new(&config);
        let (polled_sender, polled_receiver) = oneshot::channel();
        let mut source = FixtureSource {
            result: Some(Err("fixture poll failure".to_owned())),
            polled_sender: Some(polled_sender),
        };

        let result = run_service(config, publisher, event_loop, &mut source, async move {
            polled_receiver.await.map_err(std::io::Error::other)?;
            Ok(())
        })
        .await;

        assert!(result.is_ok(), "failed poll should still shut down cleanly");
    }

    #[tokio::test]
    async fn production_snapshot_source_surfaces_local_controller_errors() {
        let mut config = test_config();
        config.unifi_url = "https://127.0.0.1:1".to_owned();
        config.unifi_tls_insecure = true;
        let mut source = UniFiClient::new(&config).expect("local HTTP client should build");

        let error = SnapshotSource::fetch_snapshots(&mut source, &config.ap_macs)
            .await
            .expect_err("a closed loopback endpoint must surface a poll error");

        assert!(!error.is_empty());
    }
}
