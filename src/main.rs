use std::collections::HashSet;
use std::future::Future;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::time::MissedTickBehavior;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use unifi_apclients_mqtt::{
    config::Config,
    mapper::ApSnapshot,
    mqtt::{MqttPublisher, enqueue_with_timeout, run_event_loop},
    presence_history::PresenceHistory,
    unifi::{SnapshotBatch, UniFiClient},
};

trait SnapshotSource {
    async fn fetch_snapshots(&mut self, ap_macs: &[String]) -> Result<SnapshotBatch, String>;
}

fn unix_now() -> Result<i64, std::io::Error> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs();
    i64::try_from(seconds).map_err(std::io::Error::other)
}

impl SnapshotSource for UniFiClient {
    async fn fetch_snapshots(&mut self, ap_macs: &[String]) -> Result<SnapshotBatch, String> {
        UniFiClient::fetch_snapshots(self, ap_macs)
            .await
            .map_err(|error| error.safe_summary())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .json()
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
    let mut mqtt_task = tokio::spawn(run_event_loop(event_loop, event_publisher));

    let mut ticker = tokio::time::interval(config.poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    tokio::pin!(shutdown);
    let mut discovered_aps = HashSet::new();
    let mut history = if let Some(path) = config.client_history_db.as_deref() {
        let max_gap_seconds = config
            .poll_interval
            .as_secs()
            .saturating_mul(2)
            .saturating_add(5)
            .min(i64::MAX as u64) as i64;
        Some(PresenceHistory::open(path, unix_now()?, max_gap_seconds)?)
    } else {
        None
    };
    info!(
        ap_count = config.ap_macs.len(),
        poll_seconds = config.poll_interval.as_secs(),
        tls_insecure = config.unifi_tls_insecure,
        client_history_enabled = history.is_some(),
        "UniFi AP client poller started"
    );

    loop {
        tokio::select! {
            result = &mut shutdown => {
                result?;
                break;
            }
            result = &mut mqtt_task => {
                warn!(panicked = result.is_err(), "MQTT event loop stopped");
                return Err(std::io::Error::other("MQTT event loop stopped").into());
            }
            _ = ticker.tick() => {
                let poll_started = Instant::now();
                match source.fetch_snapshots(&config.ap_macs).await {
                    Ok(batch) => {
                        let eligible_snapshots = if let Some(history) = history.as_mut() {
                            let now = unix_now()?;
                            let mut eligible = Vec::with_capacity(batch.snapshots.len());
                            for snapshot in &batch.snapshots {
                                let clients = history.observe_ap(snapshot, now)?;
                                eligible.push(ApSnapshot {
                                    available: true,
                                    ap_mac: snapshot.ap_mac.clone(),
                                    ap_name: snapshot.ap_name.clone(),
                                    clients,
                                });
                            }
                            for ap_mac in &batch.unavailable_aps {
                                history.mark_unavailable(ap_mac, now)?;
                            }
                            Some(eligible)
                        } else {
                            None
                        };
                        let available_ap_count = batch.snapshots.len();
                        let unavailable_ap_count = batch.unavailable_aps.len();
                        let client_count = batch.snapshots.iter()
                            .map(|snapshot| snapshot.clients.len())
                            .sum::<usize>();
                        let new_snapshots = batch
                            .snapshots
                            .iter()
                            .filter(|snapshot| !discovered_aps.contains(&snapshot.ap_mac))
                            .cloned()
                            .collect::<Vec<_>>();
                        if !new_snapshots.is_empty() {
                            let discovery_queued = enqueue_with_timeout(
                                publisher.publish_discovery(&new_snapshots),
                                false,
                                "publish_home_assistant_discovery",
                            ).await;
                            if discovery_queued {
                                discovered_aps.extend(
                                    new_snapshots.iter().map(|snapshot| snapshot.ap_mac.clone()),
                                );
                                info!(ap_count = new_snapshots.len(), "queued Home Assistant discovery");
                            }
                        }
                        enqueue_with_timeout(
                            publisher.publish_snapshots(&batch.snapshots),
                            false,
                            "publish_ap_client_snapshots",
                        ).await;
                        if let Some(eligible) = &eligible_snapshots {
                            enqueue_with_timeout(
                                publisher.publish_eligible_snapshots(eligible),
                                false,
                                "publish_ap_eligible_client_snapshots",
                            ).await;
                        }
                        enqueue_with_timeout(
                            publisher.publish_unavailable(&batch.unavailable_aps),
                            false,
                            "publish_unavailable_ap_status",
                        ).await;
                        info!(
                            cycle_duration_ms = poll_started.elapsed().as_millis() as u64,
                            available_ap_count,
                            unavailable_ap_count,
                            client_count,
                            "UniFi poll cycle completed"
                        );
                    }
                    Err(error) => {
                        if let Some(history) = history.as_mut() {
                            let now = unix_now()?;
                            for ap_mac in &config.ap_macs {
                                history.mark_unavailable(ap_mac, now)?;
                            }
                        }
                        warn!(failure = %error, poll_duration_ms = poll_started.elapsed().as_millis() as u64, "UniFi poll failed; preserving the last retained snapshots");
                        enqueue_with_timeout(
                            publisher.publish_unavailable(&config.ap_macs),
                            false,
                            "publish_ap_unavailable_status",
                        ).await;
                    }
                }
            }
        }
    }

    info!("shutting down UniFi AP client poller");
    enqueue_with_timeout(
        publisher.publish_unavailable(&config.ap_macs),
        true,
        "publish_ap_shutdown_status",
    )
    .await;
    enqueue_with_timeout(
        publisher.publish_offline(),
        true,
        "publish_service_shutdown_status",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    enqueue_with_timeout(publisher.disconnect(), true, "disconnect_mqtt_client").await;
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
            client_history_db: None,
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
