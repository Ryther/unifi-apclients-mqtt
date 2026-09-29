use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rumqttc::{AsyncClient, Event, EventLoop, LastWill, MqttOptions, QoS};
use serde_json::{Value, json};

use crate::{config::Config, mapper::ApSnapshot};

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
    discovery_prefix: String,
    homeassistant_status_topic: String,
}

#[derive(Clone)]
struct CachedAp {
    ap_mac: String,
    snapshot: Option<ApSnapshot>,
    available: bool,
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
                cache.insert(
                    snapshot.ap_mac.to_ascii_lowercase(),
                    CachedAp {
                        ap_mac: snapshot.ap_mac.clone(),
                        snapshot: Some(snapshot.clone()),
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
            .filter_map(|ap| ap.snapshot.clone())
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
            if let Some(snapshot) = &ap.snapshot {
                if ap.available {
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
                            serde_json::to_string(snapshot)
                                .expect("snapshot serialization is infallible"),
                        )
                        .await?;
                } else {
                    self.client
                        .publish(
                            availability_topic_for(&self.config.base_topic, &snapshot.ap_mac),
                            QoS::AtLeastOnce,
                            true,
                            "offline",
                        )
                        .await?;
                }
            } else {
                // The AP has never returned a valid snapshot, but still needs
                // an explicit offline marker after a broker state reset.
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
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(rumqttc::Packet::ConnAck(ack))) => {
                tracing::info!(
                    session_present = ack.session_present,
                    "connected to MQTT broker"
                );
                if publisher.config.discovery_enabled {
                    let subscribe_failed = publisher
                        .client
                        .subscribe(
                            publisher.config.homeassistant_status_topic.as_str(),
                            QoS::AtLeastOnce,
                        )
                        .await
                        .is_err();
                    log_mqtt_failure(subscribe_failed, false, "subscribe_home_assistant_status");
                }
                log_mqtt_failure(
                    publisher.publish_online().await.is_err(),
                    false,
                    "publish_service_online_status",
                );
                log_mqtt_failure(
                    publisher.replay_cached_state().await.is_err(),
                    false,
                    "replay_cached_mqtt_state",
                );
            }
            Ok(Event::Incoming(rumqttc::Packet::Publish(message)))
                if publisher.config.discovery_enabled
                    && message.topic == publisher.config.homeassistant_status_topic
                    && message.payload.as_ref() == b"online" =>
            {
                tracing::debug!("Home Assistant reported online; replaying discovery and state");
                log_mqtt_failure(
                    publisher.replay_cached_state().await.is_err(),
                    false,
                    "replay_state_after_home_assistant_startup",
                );
            }
            Ok(_) => {}
            Err(_) => {
                log_mqtt_failure(true, false, "mqtt_connection");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
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
