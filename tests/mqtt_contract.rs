//! MQTT and Home Assistant discovery contract tests.
//!
//! Expected production interface:
//!
//! * `topic_id` lowercases an AP MAC and removes its colons.
//! * The topic helpers use the normalized ID in the paths asserted below.
//! * `discovery_config` returns a JSON object suitable for a retained Home
//!   Assistant MQTT discovery message.

use serde_json::json;
use unifi_apclients_mqtt::mqtt::{
    availability_topic, discovery_config, discovery_topic, state_topic, topic_id,
};

const AP_MAC: &str = "11:22:33:44:55:66";
const AP_ID: &str = "112233445566";

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
fn discovery_config_describes_the_ap_client_count_and_availability() {
    let config = discovery_config(AP_MAC, "Living Room AP");

    assert_eq!(
        config,
        json!({
            "name": "Living Room AP",
            "unique_id": "unifi_apclients_112233445566",
            "state_topic": "unifi/apclients/112233445566/state",
            "json_attributes_topic": "unifi/apclients/112233445566/state",
            "value_template": "{{ value_json.clients | count }}",
            "availability": [
                {
                    "topic": "unifi/apclients/status",
                    "payload_available": "online",
                    "payload_not_available": "offline"
                },
                {
                    "topic": "unifi/apclients/112233445566/availability",
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
