use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rumqttc::{AsyncClient, Event, EventLoop, LastWill, MqttOptions, QoS};
use serde_json::{Value, json};

use crate::{config::Config, mapper::ApSnapshot};

const GLOBAL_AVAILABILITY_TOPIC: &str = "unifi/apclients/status";

#[derive(Clone)]
pub struct MqttPublisher {
    client: AsyncClient,
    cache: Arc<Mutex<HashMap<String, CachedAp>>>,
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
        options.set_last_will(LastWill::new(
            GLOBAL_AVAILABILITY_TOPIC,
            "offline",
            QoS::AtLeastOnce,
            true,
        ));
        let (client, event_loop) = AsyncClient::new(options, 100);
        (
            Self {
                client,
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
            .publish(GLOBAL_AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "online")
            .await
    }

    pub async fn publish_offline(&self) -> Result<(), rumqttc::ClientError> {
        self.client
            .publish(GLOBAL_AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "offline")
            .await
    }

    pub async fn publish_discovery(
        &self,
        snapshots: &[ApSnapshot],
    ) -> Result<(), rumqttc::ClientError> {
        for snapshot in snapshots {
            self.client
                .publish(
                    discovery_topic(&snapshot.ap_mac),
                    QoS::AtLeastOnce,
                    true,
                    discovery_config(&snapshot.ap_mac, &snapshot.ap_name).to_string(),
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
                    availability_topic(&snapshot.ap_mac),
                    QoS::AtLeastOnce,
                    true,
                    "online",
                )
                .await?;
            self.client
                .publish(
                    state_topic(&snapshot.ap_mac),
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
                    availability_topic(ap_mac),
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
        self.publish_discovery(&snapshots).await?;
        for ap in &cached {
            if let Some(snapshot) = &ap.snapshot {
                if ap.available {
                    self.client
                        .publish(
                            availability_topic(&snapshot.ap_mac),
                            QoS::AtLeastOnce,
                            true,
                            "online",
                        )
                        .await?;
                    self.client
                        .publish(
                            state_topic(&snapshot.ap_mac),
                            QoS::AtLeastOnce,
                            true,
                            serde_json::to_string(snapshot)
                                .expect("snapshot serialization is infallible"),
                        )
                        .await?;
                } else {
                    self.client
                        .publish(
                            availability_topic(&snapshot.ap_mac),
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
                        availability_topic(&ap.ap_mac),
                        QoS::AtLeastOnce,
                        true,
                        "offline",
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

pub async fn run_event_loop(mut event_loop: EventLoop, publisher: MqttPublisher) {
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                if let Err(error) = publisher.publish_online().await {
                    tracing::warn!(%error, "could not publish MQTT online status");
                }
                if let Err(error) = publisher.replay_cached_state().await {
                    tracing::warn!(%error, "could not replay cached MQTT discovery and state");
                }
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "MQTT connection error");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
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
    format!("unifi/apclients/{}/state", topic_id(ap_mac))
}

pub fn availability_topic(ap_mac: &str) -> String {
    format!("unifi/apclients/{}/availability", topic_id(ap_mac))
}

pub fn discovery_topic(ap_mac: &str) -> String {
    format!(
        "homeassistant/sensor/unifi_apclients/{}/config",
        topic_id(ap_mac)
    )
}

pub fn discovery_config(ap_mac: &str, ap_name: &str) -> Value {
    let id = topic_id(ap_mac);
    json!({
        "name": ap_name,
        "unique_id": format!("unifi_apclients_{id}"),
        "state_topic": state_topic(ap_mac),
        "json_attributes_topic": state_topic(ap_mac),
        "value_template": "{{ value_json.clients | count }}",
        "availability": [
            {
                "topic": GLOBAL_AVAILABILITY_TOPIC,
                "payload_available": "online",
                "payload_not_available": "offline"
            },
            {
                "topic": availability_topic(ap_mac),
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
