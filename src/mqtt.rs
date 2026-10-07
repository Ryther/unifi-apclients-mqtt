use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use rumqttc::{AsyncClient, Event, EventLoop, LastWill, MqttOptions, QoS};
use serde_json::{Value, json};

use crate::{config::Config, mapper::ApSnapshot};

const MQTT_ENQUEUE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct MqttPublisher {
    client: AsyncClient,
    cache: Arc<Mutex<HashMap<String, CachedAp>>>,
    config: MqttConfig,
}

#[derive(Clone)]
struct MqttConfig {
    base_topic: String,
    discovery_enabled: bool,
    eligible_enabled: bool,
    discovery_prefix: String,
    homeassistant_status_topic: String,
}

#[derive(Clone)]
struct CachedAp {
    ap_mac: String,
    snapshot: Option<ApSnapshot>,
    eligible_snapshot: Option<ApSnapshot>,
    available: bool,
}

struct ReplayTask(Option<tokio::task::JoinHandle<()>>);

impl ReplayTask {
    fn replace(&mut self, task: tokio::task::JoinHandle<()>) {
        if let Some(previous) = self.0.replace(task) {
            previous.abort();
        }
    }

    fn abort(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }

    fn is_running(&self) -> bool {
        self.0.as_ref().is_some_and(|task| !task.is_finished())
    }
}

impl Drop for ReplayTask {
    fn drop(&mut self) {
        self.abort();
    }
}

impl MqttPublisher {
    pub fn new(config: &Config) -> (Self, EventLoop) {
        let mut options = MqttOptions::new(
            "unifi-apclients-mqtt",
            config.mqtt_host.as_str(),
            config.mqtt_port,
        );
        options.set_keep_alive(std::time::Duration::from_secs(30));
        if let Some(username) = config.mqtt_username.as_deref() {
            options.set_credentials(
                username,
                config.mqtt_password.as_deref().unwrap_or_default(),
            );
        }
        let mqtt_config = MqttConfig {
            base_topic: config.mqtt_base_topic.clone(),
            discovery_enabled: config.homeassistant_discovery_enabled,
            eligible_enabled: config.client_history_db.is_some(),
            discovery_prefix: config.homeassistant_discovery_prefix.clone(),
            homeassistant_status_topic: config.homeassistant_status_topic.clone(),
        };
        options.set_last_will(LastWill::new(
            service_availability_topic_for(&mqtt_config.base_topic),
            "offline",
            QoS::AtLeastOnce,
            true,
        ));
        let (client, event_loop) = AsyncClient::new(options, 100);
        (
            Self {
                client,
                config: mqtt_config,
                cache: Arc::new(Mutex::new(
                    config
                        .ap_macs
                        .iter()
                        .map(|mac| {
                            (
                                mac.to_ascii_lowercase(),
                                CachedAp {
                                    ap_mac: mac.clone(),
                                    snapshot: None,
                                    eligible_snapshot: None,
                                    available: false,
                                },
                            )
                        })
                        .collect(),
                )),
            },
            event_loop,
        )
    }

    pub async fn publish_online(&self) -> Result<(), rumqttc::ClientError> {
        self.client
            .publish(
                service_availability_topic_for(&self.config.base_topic),
                QoS::AtLeastOnce,
                true,
                "online",
            )
            .await
    }

    pub async fn publish_offline(&self) -> Result<(), rumqttc::ClientError> {
        self.client
            .publish(
                service_availability_topic_for(&self.config.base_topic),
                QoS::AtLeastOnce,
                true,
                "offline",
            )
            .await
    }

    pub async fn publish_discovery(
        &self,
        snapshots: &[ApSnapshot],
    ) -> Result<(), rumqttc::ClientError> {
        if !self.config.discovery_enabled {
            return Ok(());
        }
        for snapshot in snapshots {
            self.client
                .publish(
                    discovery_topic_for(&self.config.discovery_prefix, &snapshot.ap_mac),
                    QoS::AtLeastOnce,
                    true,
                    discovery_config_from_mqtt(&self.config, &snapshot.ap_mac, &snapshot.ap_name)
                        .to_string(),
                )
                .await?;
            if self.config.eligible_enabled {
                self.client
                    .publish(
                        eligible_discovery_topic_for(
                            &self.config.discovery_prefix,
                            &snapshot.ap_mac,
                        ),
                        QoS::AtLeastOnce,
                        true,
                        eligible_discovery_config_for(
                            &self.config.base_topic,
                            &snapshot.ap_mac,
                            &snapshot.ap_name,
                        )
                        .to_string(),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn publish_snapshots(
        &self,
        snapshots: &[ApSnapshot],
    ) -> Result<(), rumqttc::ClientError> {
        {
            let mut cache = self.cache.lock().expect("MQTT snapshot cache poisoned");
            for snapshot in snapshots {
                let eligible_snapshot = cache
                    .get(&snapshot.ap_mac.to_ascii_lowercase())
                    .and_then(|ap| ap.eligible_snapshot.clone());
                cache.insert(
                    snapshot.ap_mac.to_ascii_lowercase(),
                    CachedAp {
                        ap_mac: snapshot.ap_mac.clone(),
                        snapshot: Some(snapshot.clone()),
                        eligible_snapshot,
                        available: true,
                    },
                );
            }
        }
        for snapshot in snapshots {
            self.client
                .publish(
                    availability_topic_for(&self.config.base_topic, &snapshot.ap_mac),
                    QoS::AtLeastOnce,
                    true,
                    "online",
                )
                .await?;
            self.client
                .publish(
                    state_topic_for(&self.config.base_topic, &snapshot.ap_mac),
                    QoS::AtLeastOnce,
                    true,
                    serde_json::to_string(snapshot).expect("snapshot serialization is infallible"),
                )
                .await?;
        }
        Ok(())
    }

    pub async fn publish_eligible_snapshots(
        &self,
        snapshots: &[ApSnapshot],
    ) -> Result<(), rumqttc::ClientError> {
        if !self.config.eligible_enabled {
            return Ok(());
        }
        {
            let mut cache = self.cache.lock().expect("MQTT snapshot cache poisoned");
            for snapshot in snapshots {
                if let Some(ap) = cache.get_mut(&snapshot.ap_mac.to_ascii_lowercase()) {
                    ap.eligible_snapshot = Some(snapshot.clone());
                    ap.available = true;
                }
            }
        }
        for snapshot in snapshots {
            self.client
                .publish(
                    eligible_state_topic_for(&self.config.base_topic, &snapshot.ap_mac),
                    QoS::AtLeastOnce,
                    true,
                    serde_json::to_string(snapshot)
                        .expect("eligible snapshot serialization is infallible"),
                )
                .await?;
        }
        Ok(())
    }

    pub async fn publish_unavailable(
        &self,
        ap_macs: &[String],
    ) -> Result<(), rumqttc::ClientError> {
        {
            let mut cache = self.cache.lock().expect("MQTT snapshot cache poisoned");
            for ap_mac in ap_macs {
                if let Some(cached) = cache.get_mut(&ap_mac.to_ascii_lowercase()) {
                    cached.available = false;
                }
            }
        }
        for ap_mac in ap_macs {
            self.client
                .publish(
                    availability_topic_for(&self.config.base_topic, ap_mac),
                    QoS::AtLeastOnce,
                    true,
                    "offline",
                )
                .await?;
        }
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), rumqttc::ClientError> {
        self.client.disconnect().await
    }

    async fn replay_cached_state(&self) -> Result<(), rumqttc::ClientError> {
        let cached = self
            .cache
            .lock()
            .expect("MQTT snapshot cache poisoned")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let snapshots = cached
            .iter()
            .filter_map(|ap| ap.snapshot.clone().or_else(|| ap.eligible_snapshot.clone()))
            .collect::<Vec<_>>();
        let available_ap_count = cached.iter().filter(|ap| ap.available).count();
        let client_count = cached
            .iter()
            .filter(|ap| ap.available)
            .filter_map(|ap| ap.snapshot.as_ref())
            .map(|snapshot| snapshot.clients.len())
            .sum::<usize>();
        if self.config.discovery_enabled {
            self.publish_discovery(&snapshots).await?;
        }
        for ap in &cached {
            if ap.available {
                self.client
                    .publish(
                        availability_topic_for(&self.config.base_topic, &ap.ap_mac),
                        QoS::AtLeastOnce,
                        true,
                        "online",
                    )
                    .await?;
                if let Some(snapshot) = &ap.snapshot {
                    self.client
                        .publish(
                            state_topic_for(&self.config.base_topic, &ap.ap_mac),
                            QoS::AtLeastOnce,
                            true,
                            serde_json::to_string(snapshot)
                                .expect("snapshot serialization is infallible"),
                        )
                        .await?;
                }
                if self.config.eligible_enabled
                    && let Some(eligible) = &ap.eligible_snapshot
                {
                    self.client
                        .publish(
                            eligible_state_topic_for(&self.config.base_topic, &ap.ap_mac),
                            QoS::AtLeastOnce,
                            true,
                            serde_json::to_string(eligible)
                                .expect("eligible snapshot serialization is infallible"),
                        )
                        .await?;
                }
            } else {
                self.client
                    .publish(
                        availability_topic_for(&self.config.base_topic, &ap.ap_mac),
                        QoS::AtLeastOnce,
                        true,
                        "offline",
                    )
                    .await?;
            }
        }
        tracing::info!(
            ap_count = cached.len(),
            available_ap_count,
            client_count,
            discovery_enabled = self.config.discovery_enabled,
            "queued cached MQTT state replay"
        );
        Ok(())
    }
}

pub async fn run_event_loop(mut event_loop: EventLoop, publisher: MqttPublisher) {
    let mut replay_task = ReplayTask(None);
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(rumqttc::Packet::ConnAck(ack))) => {
                tracing::info!(
                    session_present = ack.session_present,
                    "connected to MQTT broker"
                );
                let replay_publisher = publisher.clone();
                replay_task.replace(tokio::spawn(async move {
                    if replay_publisher.config.discovery_enabled {
                        log_mqtt_failure(
                            replay_publisher
                                .client
                                .subscribe(
                                    replay_publisher.config.homeassistant_status_topic.as_str(),
                                    QoS::AtLeastOnce,
                                )
                                .await
                                .is_err(),
                            false,
                            "subscribe_home_assistant_status",
                        );
                    }
                    log_mqtt_failure(
                        replay_publisher.publish_online().await.is_err(),
                        false,
                        "publish_service_online_status",
                    );
                    log_mqtt_failure(
                        replay_publisher.replay_cached_state().await.is_err(),
                        false,
                        "replay_cached_mqtt_state",
                    );
                }));
            }
            Ok(Event::Incoming(rumqttc::Packet::Publish(message)))
                if publisher.config.discovery_enabled
                    && message.topic == publisher.config.homeassistant_status_topic
                    && message.payload.as_ref() == b"online" =>
            {
                tracing::debug!("Home Assistant reported online; replaying discovery and state");
                if replay_task.is_running() {
                    continue;
                }
                let replay_publisher = publisher.clone();
                replay_task.replace(tokio::spawn(async move {
                    log_mqtt_failure(
                        replay_publisher.replay_cached_state().await.is_err(),
                        false,
                        "replay_state_after_home_assistant_startup",
                    );
                }));
            }
            Ok(_) => {}
            Err(_) => {
                replay_task.abort();
                log_mqtt_failure(true, false, "mqtt_connection");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

/// Bound waits for rumqttc's request queue so its event loop can keep draining it.
/// A timed-out send is cancelled; the cached snapshots are replayed on reconnect.
pub async fn enqueue_with_timeout<F, E>(
    operation_future: F,
    is_shutdown: bool,
    operation: &'static str,
) -> bool
where
    F: Future<Output = Result<(), E>>,
{
    match tokio::time::timeout(MQTT_ENQUEUE_TIMEOUT, operation_future).await {
        Ok(Ok(())) => true,
        Ok(Err(_)) => {
            log_mqtt_failure(true, is_shutdown, operation);
            false
        }
        Err(_) => {
            if is_shutdown {
                tracing::error!(operation, "MQTT operation timed out");
            } else {
                tracing::warn!(operation, "MQTT operation timed out");
            }
            false
        }
    }
}

pub fn log_mqtt_failure(failed: bool, is_shutdown: bool, operation: &'static str) {
    if failed {
        if is_shutdown {
            tracing::error!(operation, "MQTT operation failed");
        } else {
            tracing::warn!(operation, "MQTT operation failed");
        }
    }
}

pub fn topic_id(ap_mac: &str) -> String {
    ap_mac
        .chars()
        .filter(|character| *character != ':')
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn state_topic(ap_mac: &str) -> String {
    state_topic_for("unifi/apclients", ap_mac)
}

pub fn availability_topic(ap_mac: &str) -> String {
    availability_topic_for("unifi/apclients", ap_mac)
}

pub fn discovery_topic(ap_mac: &str) -> String {
    discovery_topic_for("homeassistant", ap_mac)
}

pub fn state_topic_for(base_topic: &str, ap_mac: &str) -> String {
    format!("{}/{}/state", base_topic, topic_id(ap_mac))
}

pub fn availability_topic_for(base_topic: &str, ap_mac: &str) -> String {
    format!("{}/{}/availability", base_topic, topic_id(ap_mac))
}

pub fn service_availability_topic_for(base_topic: &str) -> String {
    format!("{base_topic}/status")
}

pub fn discovery_topic_for(discovery_prefix: &str, ap_mac: &str) -> String {
    format!(
        "{discovery_prefix}/sensor/unifi_apclients/{}/config",
        topic_id(ap_mac)
    )
}

pub fn eligible_state_topic_for(base_topic: &str, ap_mac: &str) -> String {
    format!("{}/{}/eligible/state", base_topic, topic_id(ap_mac))
}

pub fn eligible_discovery_topic_for(discovery_prefix: &str, ap_mac: &str) -> String {
    format!(
        "{discovery_prefix}/sensor/unifi_apclients/{}_eligible/config",
        topic_id(ap_mac)
    )
}

pub fn eligible_discovery_config_for(base_topic: &str, ap_mac: &str, ap_name: &str) -> Value {
    let id = topic_id(ap_mac);
    json!({
        "name": "Eligible clients",
        "icon": "mdi:account-check",
        "unique_id": format!("unifi_apclients_{id}_eligible"),
        "state_topic": eligible_state_topic_for(base_topic, ap_mac),
        "json_attributes_topic": eligible_state_topic_for(base_topic, ap_mac),
        "value_template": "{{ value_json.clients | count }}",
        "availability": [
            {
                "topic": service_availability_topic_for(base_topic),
                "payload_available": "online",
                "payload_not_available": "offline"
            },
            {
                "topic": availability_topic_for(base_topic, ap_mac),
                "payload_available": "online",
                "payload_not_available": "offline"
            }
        ],
        "availability_mode": "all",
        "device": {
            "name": ap_name,
            "identifiers": [ap_mac]
        }
    })
}

pub fn discovery_config(ap_mac: &str, ap_name: &str) -> Value {
    discovery_config_for("unifi/apclients", ap_mac, ap_name)
}

fn discovery_config_from_mqtt(config: &MqttConfig, ap_mac: &str, ap_name: &str) -> Value {
    discovery_config_for_topics(
        &config.base_topic,
        &service_availability_topic_for(&config.base_topic),
        ap_mac,
        ap_name,
    )
}

pub fn discovery_config_for(base_topic: &str, ap_mac: &str, ap_name: &str) -> Value {
    discovery_config_for_topics(
        base_topic,
        &service_availability_topic_for(base_topic),
        ap_mac,
        ap_name,
    )
}

fn discovery_config_for_topics(
    base_topic: &str,
    global_availability_topic: &str,
    ap_mac: &str,
    ap_name: &str,
) -> Value {
    let id = topic_id(ap_mac);
    json!({
        "name": ap_name,
        "unique_id": format!("unifi_apclients_{id}"),
        "state_topic": state_topic_for(base_topic, ap_mac),
        "json_attributes_topic": state_topic_for(base_topic, ap_mac),
        "value_template": "{{ value_json.clients | count }}",
        "availability": [
            {
                "topic": global_availability_topic,
                "payload_available": "online",
                "payload_not_available": "offline"
            },
            {
                "topic": availability_topic_for(base_topic, ap_mac),
                "payload_available": "online",
                "payload_not_available": "offline"
            }
        ],
        "availability_mode": "all",
        "device": {
            "name": ap_name,
            "identifiers": [ap_mac]
        }
    })
}

#[cfg(test)]
mod logging_tests {
    use std::{io, sync::Mutex};

    use super::log_mqtt_failure;

    #[derive(Clone)]
    struct CaptureWriter(std::sync::Arc<Mutex<Vec<u8>>>);

    impl io::Write for CaptureWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("capture buffer lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn emits_structured_failure_operations_only_for_failed_requests() {
        let buffer = std::sync::Arc::new(Mutex::new(Vec::new()));
        let writer_buffer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_writer(move || CaptureWriter(writer_buffer.clone()))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            log_mqtt_failure(true, false, "publish_state");
            log_mqtt_failure(true, true, "shutdown_status");
            log_mqtt_failure(false, false, "ignored_operation");
        });

        let output = String::from_utf8(buffer.lock().expect("capture buffer lock").clone())
            .expect("JSON logs are UTF-8");
        assert_eq!(output.lines().count(), 2);
        assert!(output.contains("\"operation\":\"publish_state\""));
        assert!(output.contains("\"operation\":\"shutdown_status\""));
        assert!(!output.contains("ignored_operation"));
        assert!(output.contains("\"level\":\"WARN\""));
        assert!(output.contains("\"level\":\"ERROR\""));
    }
}
