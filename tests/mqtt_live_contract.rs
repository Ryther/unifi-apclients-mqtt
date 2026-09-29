//! Live MQTT retention contract.
//!
//! Set `MQTT_TEST_BROKER=host:port` to run this test against a disposable
//! broker. With the variable absent it is intentionally skipped.

use std::{path::PathBuf, time::Duration};

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde_json::json;
use unifi_apclients_mqtt::config::Config;
use unifi_apclients_mqtt::mapper::{ApSnapshot, ClientSnapshot};
use unifi_apclients_mqtt::mqtt::{
    MqttPublisher, availability_topic_for, discovery_topic_for, eligible_discovery_topic_for,
    eligible_state_topic_for, run_event_loop, state_topic_for,
};

const AP_MAC: &str = "11:22:33:44:55:66";
const AP_WITHOUT_SNAPSHOT: &str = "22:33:44:55:66:77";
const MQTT_BASE_TOPIC: &str = "contract/unifi";
const SERVICE_STATUS_TOPIC: &str = "contract/unifi/status";
const DISCOVERY_PREFIX: &str = "ha2";
const HOMEASSISTANT_STATUS_TOPIC: &str = "ha/status";
static BROKER_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn state_topic(ap_mac: &str) -> String {
    state_topic_for(MQTT_BASE_TOPIC, ap_mac)
}

fn availability_topic(ap_mac: &str) -> String {
    availability_topic_for(MQTT_BASE_TOPIC, ap_mac)
}

fn discovery_topic(ap_mac: &str) -> String {
    discovery_topic_for(DISCOVERY_PREFIX, ap_mac)
}

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
        client_history_db: None,
        mqtt_base_topic: MQTT_BASE_TOPIC.to_owned(),
        homeassistant_discovery_enabled: true,
        homeassistant_discovery_prefix: DISCOVERY_PREFIX.to_owned(),
        homeassistant_status_topic: HOMEASSISTANT_STATUS_TOPIC.to_owned(),
    }
}

#[tokio::test]
async fn eligible_state_is_retained_and_replayed_after_home_assistant_birth() {
    let Some(broker) = std::env::var("MQTT_TEST_BROKER")
        .ok()
        .and_then(|value| parse_broker(&value))
    else {
        return;
    };
    let _broker_guard = BROKER_TEST_LOCK.lock().await;

    let mut config = broker_config(broker.0, broker.1);
    config.client_history_db = Some(PathBuf::from("_tmp/live-history-fixture.db"));
    config.mqtt_base_topic = format!("contract/eligible/{}", std::process::id());
    config.homeassistant_discovery_prefix = format!("ha2-eligible-{}", std::process::id());
    config.homeassistant_status_topic = format!("ha/eligible-{}", std::process::id());
    let eligible_state_topic = eligible_state_topic_for(&config.mqtt_base_topic, AP_MAC);
    let eligible_discovery_topic =
        eligible_discovery_topic_for(&config.homeassistant_discovery_prefix, AP_MAC);

    let (publisher, publisher_events) = MqttPublisher::new(&config);
    let publisher_task = tokio::spawn(run_event_loop(publisher_events, publisher.clone()));
    tokio::time::sleep(Duration::from_millis(250)).await;

    let snapshot = ApSnapshot {
        available: true,
        ap_mac: AP_MAC.to_owned(),
        ap_name: "Living Room AP".to_owned(),
        clients: vec![ClientSnapshot {
            mac: "02:aa:00:00:00:01".to_owned(),
            name: "Test client".to_owned(),
            hostname: None,
            ip: None,
        }],
    };
    publisher
        .publish_discovery(std::slice::from_ref(&snapshot))
        .await
        .expect("eligible discovery should queue");
    publisher
        .publish_eligible_snapshots(std::slice::from_ref(&snapshot))
        .await
        .expect("eligible state should queue");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let client_id = format!("unifi-eligible-contract-{}", std::process::id());
    let mut options = MqttOptions::new(client_id, &config.mqtt_host, config.mqtt_port);
    options.set_keep_alive(Duration::from_secs(5));
    let (subscriber, mut subscriber_events) = AsyncClient::new(options, 20);
    subscriber
        .subscribe(&eligible_discovery_topic, QoS::AtLeastOnce)
        .await
        .expect("eligible discovery subscription should succeed");
    subscriber
        .subscribe(&eligible_state_topic, QoS::AtLeastOnce)
        .await
        .expect("eligible state subscription should succeed");

    let mut retained = std::collections::HashMap::new();
    while retained.len() < 2 {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("retained eligible messages should arrive")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            assert!(
                message.retain,
                "late subscriber should receive retained state"
            );
            retained.insert(message.topic, message.payload.to_vec());
        }
    }
    let discovery: serde_json::Value = serde_json::from_slice(
        retained
            .get(&eligible_discovery_topic)
            .expect("eligible discovery should be retained"),
    )
    .expect("eligible discovery payload should be JSON");
    assert_eq!(discovery["state_topic"], eligible_state_topic);
    assert_eq!(
        discovery["value_template"],
        "{{ value_json.clients | count }}"
    );
    let state = retained
        .get(&eligible_state_topic)
        .expect("eligible state should be retained");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(state).expect("eligible state should be JSON"),
        json!(snapshot)
    );

    subscriber
        .publish(&eligible_state_topic, QoS::AtLeastOnce, true, "")
        .await
        .expect("eligible state removal should queue");
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("eligible state tombstone should arrive")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event
            && message.topic == eligible_state_topic
            && message.payload.is_empty()
        {
            break;
        }
    }
    subscriber
        .publish(
            &config.homeassistant_status_topic,
            QoS::AtLeastOnce,
            true,
            "online",
        )
        .await
        .expect("Home Assistant birth should queue");
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), subscriber_events.poll())
            .await
            .expect("eligible state should replay after Home Assistant birth")
            .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event
            && message.topic == eligible_state_topic
            && serde_json::from_slice::<serde_json::Value>(&message.payload).ok()
                == Some(json!(snapshot))
        {
            break;
        }
    }

    subscriber
        .disconnect()
        .await
        .expect("subscriber should disconnect");
    publisher_task.abort();
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
    let _broker_guard = BROKER_TEST_LOCK.lock().await;

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
        .subscribe("ha2/sensor/unifi_apclients/+/config", QoS::AtLeastOnce)
        .await
        .expect("discovery subscription should succeed");
    subscriber
        .subscribe("contract/unifi/+/state", QoS::AtLeastOnce)
        .await
        .expect("state subscription should succeed");
    subscriber
        .subscribe("contract/unifi/+/availability", QoS::AtLeastOnce)
        .await
        .expect("availability subscription should succeed");
    subscriber
        .subscribe(SERVICE_STATUS_TOPIC, QoS::AtLeastOnce)
        .await
        .expect("service availability subscription should succeed");
    subscriber
        .subscribe(HOMEASSISTANT_STATUS_TOPIC, QoS::AtLeastOnce)
        .await
        .expect("Home Assistant status subscription should succeed");

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
    assert_eq!(retained.get(SERVICE_STATUS_TOPIC).unwrap(), b"online");

    // Home Assistant's online birth message prompts the service to restore
    // discovery configuration even if the broker no longer has it retained.
    subscriber
        .publish(discovery_topic(AP_MAC), QoS::AtLeastOnce, true, "")
        .await
        .expect("discovery config removal should queue");
    let mut removed = false;
    let removal_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !removed {
        let event = tokio::time::timeout(
            removal_deadline.saturating_duration_since(tokio::time::Instant::now()),
            subscriber_events.poll(),
        )
        .await
        .expect("discovery config removal should be observed")
        .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            removed = message.topic == discovery_topic(AP_MAC) && message.payload.is_empty();
        }
    }
    subscriber
        .publish(
            HOMEASSISTANT_STATUS_TOPIC,
            QoS::AtLeastOnce,
            false,
            "online",
        )
        .await
        .expect("Home Assistant online event should queue");
    let mut rediscovered = false;
    let discovery_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !rediscovered {
        let event = tokio::time::timeout(
            discovery_deadline.saturating_duration_since(tokio::time::Instant::now()),
            subscriber_events.poll(),
        )
        .await
        .expect("discovery config should be restored after Home Assistant startup")
        .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event {
            rediscovered = message.topic == discovery_topic(AP_MAC)
                && serde_json::from_slice::<serde_json::Value>(&message.payload).is_ok();
        }
    }
    let mut replay_after_ha = std::collections::HashSet::new();
    let replay_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while replay_after_ha.len() < 3 {
        let event = tokio::time::timeout(
            replay_deadline.saturating_duration_since(tokio::time::Instant::now()),
            subscriber_events.poll(),
        )
        .await
        .expect("cached state should follow the discovery replay")
        .expect("subscriber event loop should remain connected");
        if let Event::Incoming(Packet::Publish(message)) = event
            && [
                discovery_topic(AP_MAC),
                state_topic(AP_MAC),
                availability_topic(AP_MAC),
                availability_topic(AP_WITHOUT_SNAPSHOT),
            ]
            .iter()
            .any(|topic| topic == &message.topic)
        {
            replay_after_ha.insert(message.topic);
        }
    }

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
    assert_eq!(online_replay.get(SERVICE_STATUS_TOPIC).unwrap(), b"online");

    publisher
        .publish_unavailable(&[AP_MAC.to_owned()])
        .await
        .expect("AP unavailable status should queue");
    publisher
        .publish_offline()
        .await
        .expect("service offline status should queue");
    let mut unavailable = std::collections::HashMap::<String, Vec<u8>>::new();
    while unavailable
        .get(&availability_topic(AP_MAC))
        .is_none_or(|payload| payload != b"offline")
        || unavailable
            .get(SERVICE_STATUS_TOPIC)
            .is_none_or(|payload| payload != b"offline")
    {
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
    assert_eq!(unavailable.get(SERVICE_STATUS_TOPIC).unwrap(), b"offline");

    // Once unavailable, reconnect marks the AP offline and does not publish
    // the cached client state as if it were current.
    publisher
        .disconnect()
        .await
        .expect("publisher should disconnect after becoming unavailable");
    let mut offline_replay = std::collections::HashMap::<String, Vec<u8>>::new();
    while offline_replay
        .get(&discovery_topic(AP_MAC))
        .is_none_or(|payload| serde_json::from_slice::<serde_json::Value>(payload).is_err())
        || offline_replay
            .get(&availability_topic(AP_MAC))
            .is_none_or(|payload| payload != b"offline")
        || offline_replay
            .get(&availability_topic(AP_WITHOUT_SNAPSHOT))
            .is_none_or(|payload| payload != b"offline")
        || offline_replay
            .get(SERVICE_STATUS_TOPIC)
            .is_none_or(|payload| payload != b"online")
    {
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
    assert_eq!(offline_replay.get(SERVICE_STATUS_TOPIC).unwrap(), b"online");

    subscriber
        .disconnect()
        .await
        .expect("subscriber should disconnect");
    publisher_task.abort();
}
