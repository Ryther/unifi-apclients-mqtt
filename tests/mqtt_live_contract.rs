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
const AP_WITHOUT_SNAPSHOT: &str = "22:33:44:55:66:77";

fn broker_config(host: String, port: u16) -> Config {
    Config {
        unifi_url: "https://controller.example.test:8443".to_owned(),
        unifi_username: "collector".to_owned(),
        unifi_password: "secret".to_owned(),
        ap_macs: vec![AP_MAC.to_owned(), AP_WITHOUT_SNAPSHOT.to_owned()],
        poll_interval: Duration::from_secs(5),
        unifi_tls_insecure: false,
        mqtt_host: host,
        mqtt_port: port,
        mqtt_username: Some("publisher".to_owned()),
        mqtt_password: Some("fixture-secret".to_owned()),
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
    subscriber
        .subscribe("unifi/apclients/status", QoS::AtLeastOnce)
        .await
        .expect("service availability subscription should succeed");

    let mut retained = std::collections::HashMap::new();
    while retained.len() < 5 {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("retained MQTT messages should arrive")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            retained.insert(message.topic, message.payload.to_vec());
        }
    }

    let discovery_payload = retained
        .get(&discovery_topic(AP_MAC))
        .expect("retained discovery message should arrive");
    let discovery: serde_json::Value =
        serde_json::from_slice(discovery_payload).expect("discovery payload should be JSON");
    assert_eq!(discovery["state_topic"], state_topic(AP_MAC));

    let state_payload = retained
        .get(&state_topic(AP_MAC))
        .expect("retained state message should arrive");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(state_payload).unwrap(),
        json!(snapshot)
    );

    let availability_payload = retained
        .get(&availability_topic(AP_MAC))
        .expect("retained availability message should arrive");
    assert_eq!(availability_payload, b"online");
    assert_eq!(
        retained
            .get(&availability_topic(AP_WITHOUT_SNAPSHOT))
            .unwrap(),
        b"offline"
    );
    assert_eq!(retained.get("unifi/apclients/status").unwrap(), b"online");

    // The connected subscriber sees fresh cached publications on reconnect,
    // including the snapshot and online availability of a healthy AP.
    publisher
        .disconnect()
        .await
        .expect("publisher should disconnect");
    let mut online_replay = std::collections::HashMap::new();
    while online_replay.len() < 5 {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("cached MQTT state should be replayed after reconnect")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            online_replay.insert(message.topic, message.payload.to_vec());
        }
    }
    assert!(online_replay.contains_key(&discovery_topic(AP_MAC)));
    assert!(online_replay.contains_key(&state_topic(AP_MAC)));
    assert_eq!(
        online_replay.get(&availability_topic(AP_MAC)).unwrap(),
        b"online"
    );
    assert_eq!(
        online_replay
            .get(&availability_topic(AP_WITHOUT_SNAPSHOT))
            .unwrap(),
        b"offline"
    );
    assert_eq!(
        online_replay.get("unifi/apclients/status").unwrap(),
        b"online"
    );

    publisher
        .publish_unavailable(&[AP_MAC.to_owned()])
        .await
        .expect("AP unavailable status should queue");
    publisher
        .publish_offline()
        .await
        .expect("service offline status should queue");
    let mut unavailable = std::collections::HashMap::new();
    while unavailable.len() < 2 {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("offline MQTT messages should arrive")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            unavailable.insert(message.topic, message.payload.to_vec());
        }
    }
    assert_eq!(
        unavailable.get(&availability_topic(AP_MAC)).unwrap(),
        b"offline"
    );
    assert_eq!(
        unavailable.get("unifi/apclients/status").unwrap(),
        b"offline"
    );

    // Once unavailable, reconnect marks the AP offline and does not publish
    // the cached client state as if it were current.
    publisher
        .disconnect()
        .await
        .expect("publisher should disconnect after becoming unavailable");
    let mut offline_replay = std::collections::HashMap::new();
    while offline_replay.len() < 4 {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("offline cached MQTT state should be replayed")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            offline_replay.insert(message.topic, message.payload.to_vec());
        }
    }
    assert!(offline_replay.contains_key(&discovery_topic(AP_MAC)));
    assert_eq!(
        offline_replay.get(&availability_topic(AP_MAC)).unwrap(),
        b"offline"
    );
    assert_eq!(
        offline_replay
            .get(&availability_topic(AP_WITHOUT_SNAPSHOT))
            .unwrap(),
        b"offline"
    );
    assert_eq!(
        offline_replay.get("unifi/apclients/status").unwrap(),
        b"online"
    );

    subscriber
        .disconnect()
        .await
        .expect("subscriber should disconnect");
    publisher_task.abort();
}
