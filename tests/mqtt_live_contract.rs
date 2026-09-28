//! Live MQTT retention contract.
//!
//! Set `MQTT_TEST_BROKER=host:port` to run this test against a disposable
//! broker. With the variable absent it is intentionally skipped.

use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde_json::json;
use unifi_apclients_mqtt::config::Config;
use unifi_apclients_mqtt::mapper::{ApSnapshot, ClientSnapshot};
use unifi_apclients_mqtt::mqtt::{
    MqttPublisher, availability_topic, discovery_topic, run_event_loop, state_topic,
};

const AP_MAC: &str = "11:22:33:44:55:66";

fn broker_config(host: String, port: u16) -> Config {
    Config {
        unifi_url: "https://controller.example.test:8443".to_owned(),
        unifi_username: "collector".to_owned(),
        unifi_password: "secret".to_owned(),
        ap_macs: vec![AP_MAC.to_owned()],
        poll_interval: Duration::from_secs(5),
        unifi_tls_insecure: false,
        mqtt_host: host,
        mqtt_port: port,
        mqtt_username: None,
        mqtt_password: None,
    }
}

fn parse_broker(value: &str) -> Option<(String, u16)> {
    let value = value.strip_prefix("mqtt://").unwrap_or(value);
    let (host, port) = value.rsplit_once(':')?;
    Some((host.to_owned(), port.parse().ok()?))
}

#[tokio::test]
async fn retained_discovery_state_and_availability_are_delivered_to_late_subscriber() {
    let Some(broker) = std::env::var("MQTT_TEST_BROKER")
        .ok()
        .and_then(|value| parse_broker(&value))
    else {
        return;
    };

    let config = broker_config(broker.0, broker.1);
    let (publisher, publisher_events) = MqttPublisher::new(&config);
    let publisher_task = tokio::spawn(run_event_loop(publisher_events, publisher.clone()));

    tokio::time::sleep(Duration::from_millis(250)).await;
    let snapshot = ApSnapshot {
        available: true,
        ap_mac: AP_MAC.to_owned(),
        ap_name: "Living Room AP".to_owned(),
        clients: vec![ClientSnapshot {
            mac: "AA:AA:AA:AA:AA:01".to_owned(),
            name: "living-room-phone".to_owned(),
            hostname: Some("living-room-phone".to_owned()),
            ip: Some("192.0.2.21".to_owned()),
        }],
    };
    publisher
        .publish_discovery(std::slice::from_ref(&snapshot))
        .await
        .expect("discovery publish should queue");
    publisher
        .publish_snapshots(std::slice::from_ref(&snapshot))
        .await
        .expect("snapshot publish should queue");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let client_id = format!("unifi-apclients-contract-{}", std::process::id());
    let mut options = MqttOptions::new(client_id, &config.mqtt_host, config.mqtt_port);
    options.set_keep_alive(Duration::from_secs(5));
    let (subscriber, mut subscriber_events) = AsyncClient::new(options, 20);
    subscriber
        .subscribe(
            "homeassistant/sensor/unifi_apclients/+/config",
            QoS::AtLeastOnce,
        )
        .await
        .expect("discovery subscription should succeed");
    subscriber
        .subscribe("unifi/apclients/+/state", QoS::AtLeastOnce)
        .await
        .expect("state subscription should succeed");
    subscriber
        .subscribe("unifi/apclients/+/availability", QoS::AtLeastOnce)
        .await
        .expect("availability subscription should succeed");

    let mut retained = Vec::new();
    while retained.len() < 3 {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("retained MQTT messages should arrive")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            retained.push((message.topic, message.payload.to_vec()));
        }
    }

    let discovery_payload = retained
        .iter()
        .find(|(topic, _)| topic == &discovery_topic(AP_MAC))
        .expect("retained discovery message should arrive");
    let discovery: serde_json::Value =
        serde_json::from_slice(&discovery_payload.1).expect("discovery payload should be JSON");
    assert_eq!(discovery["state_topic"], state_topic(AP_MAC));

    let state_payload = retained
        .iter()
        .find(|(topic, _)| topic == &state_topic(AP_MAC))
        .expect("retained state message should arrive");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&state_payload.1).unwrap(),
        json!(snapshot)
    );

    let availability_payload = retained
        .iter()
        .find(|(topic, _)| topic == &availability_topic(AP_MAC))
        .expect("retained availability message should arrive");
    assert_eq!(availability_payload.1, b"online");

    // A fresh broker session must receive cached discovery and snapshots
    // again, even if the broker lost its retained store during reconnection.
    publisher
        .disconnect()
        .await
        .expect("publisher should disconnect");
    let mut replayed_topics = std::collections::HashSet::new();
    while replayed_topics.len() < 3 {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("cached MQTT state should be replayed after reconnect")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            replayed_topics.insert(message.topic);
        }
    }
    assert!(replayed_topics.contains(&discovery_topic(AP_MAC)));
    assert!(replayed_topics.contains(&state_topic(AP_MAC)));
    assert!(replayed_topics.contains(&availability_topic(AP_MAC)));

    subscriber
        .disconnect()
        .await
        .expect("subscriber should disconnect");
    publisher_task.abort();
}
