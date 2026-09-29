//! MQTT and Home Assistant discovery contract tests.
//!
//! Expected production interface:
//!
//! * `topic_id` lowercases an AP MAC and removes its colons.
//! * The topic helpers use the normalized ID in the paths asserted below.
//! * `discovery_config` returns a JSON object suitable for a retained Home
//!   Assistant MQTT discovery message.

use serde_json::json;
use std::time::Duration;
use unifi_apclients_mqtt::config::Config;
use unifi_apclients_mqtt::mqtt::{
    availability_topic, availability_topic_for, discovery_config, discovery_config_for,
    discovery_topic, discovery_topic_for, service_availability_topic_for, state_topic,
    state_topic_for, topic_id,
};

const AP_MAC: &str = "11:22:33:44:55:66";
const AP_ID: &str = "112233445566";

fn configured_topics() -> Config {
    Config {
        unifi_url: "https://controller.example.test:8443".to_owned(),
        unifi_username: "collector".to_owned(),
        unifi_password: "secret".to_owned(),
        ap_macs: vec![AP_MAC.to_owned()],
        poll_interval: Duration::from_secs(5),
        unifi_tls_insecure: false,
        mqtt_host: "mqtt.example.test".to_owned(),
        mqtt_port: 1883,
        mqtt_username: None,
        mqtt_password: None,
        mqtt_base_topic: "site-a/unifi".to_owned(),
        homeassistant_discovery_enabled: true,
        homeassistant_discovery_prefix: "ha2".to_owned(),
        homeassistant_status_topic: "ha/birth".to_owned(),
    }
}

#[test]
fn builds_normalized_topics_for_an_ap() {
    assert_eq!(topic_id(AP_MAC), AP_ID);
    assert_eq!(state_topic(AP_MAC), "unifi/apclients/112233445566/state");
    assert_eq!(
        availability_topic(AP_MAC),
        "unifi/apclients/112233445566/availability"
    );
    assert_eq!(
        discovery_topic(AP_MAC),
        "homeassistant/sensor/unifi_apclients/112233445566/config"
    );
}

#[test]
fn default_topics_remain_compatible_with_existing_installations() {
    assert_eq!(state_topic(AP_MAC), "unifi/apclients/112233445566/state");
    assert_eq!(
        availability_topic(AP_MAC),
        "unifi/apclients/112233445566/availability"
    );
    assert_eq!(
        discovery_topic(AP_MAC),
        "homeassistant/sensor/unifi_apclients/112233445566/config"
    );
    let config = discovery_config(AP_MAC, "Living Room AP");
    assert_eq!(config["state_topic"], "unifi/apclients/112233445566/state");
    assert_eq!(config["availability"][0]["topic"], "unifi/apclients/status");
}

#[test]
fn configured_topics_keep_status_and_discovery_prefix_independent() {
    let config = configured_topics();

    assert_eq!(
        state_topic_for(&config.mqtt_base_topic, AP_MAC),
        "site-a/unifi/112233445566/state"
    );
    assert_eq!(
        availability_topic_for(&config.mqtt_base_topic, AP_MAC),
        "site-a/unifi/112233445566/availability"
    );
    assert_eq!(
        service_availability_topic_for(&config.mqtt_base_topic),
        "site-a/unifi/status"
    );
    assert_eq!(
        discovery_topic_for(&config.homeassistant_discovery_prefix, AP_MAC),
        "ha2/sensor/unifi_apclients/112233445566/config"
    );
}

#[test]
fn discovery_config_describes_the_ap_client_count_and_availability() {
    let app_config = configured_topics();
    let config = discovery_config_for(&app_config.mqtt_base_topic, AP_MAC, "Living Room AP");

    assert_eq!(
        config,
        json!({
            "name": "Living Room AP",
            "unique_id": "unifi_apclients_112233445566",
            "state_topic": "site-a/unifi/112233445566/state",
            "json_attributes_topic": "site-a/unifi/112233445566/state",
            "value_template": "{{ value_json.clients | count }}",
            "availability": [
                {
                    "topic": "site-a/unifi/status",
                    "payload_available": "online",
                    "payload_not_available": "offline"
                },
                {
                    "topic": "site-a/unifi/112233445566/availability",
                    "payload_available": "online",
                    "payload_not_available": "offline"
                }
            ],
            "availability_mode": "all",
            "device": {
                "name": "Living Room AP",
                "identifiers": [AP_MAC]
            }
        })
    );
}
