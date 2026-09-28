use std::collections::HashSet;
use std::time::Duration;

use tokio::time::MissedTickBehavior;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use unifi_apclients_mqtt::{
    config::Config,
    mqtt::{MqttPublisher, run_event_loop},
    unifi::UniFiClient,
};

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
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let mut discovered_aps = HashSet::new();

    loop {
        tokio::select! {
            result = &mut shutdown => {
                result?;
                break;
            }
            _ = ticker.tick() => {
                match unifi.fetch_snapshots(&config.ap_macs).await {
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
