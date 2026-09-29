//! UniFi client to AP snapshot contract tests.
//!
//! Expected production interface:
//! `map_ap_snapshot(ap_mac, ap_name, raw_unifi_clients_json)` returns a
//! serializable `ApSnapshot` with `available` and `clients` fields. Each
//! client contains `mac`, `name`, `hostname`, and `ip`; only clients whose
//! `ap_mac` matches the requested AP are included. MAC matching is
//! case-insensitive. An empty successful response is an available snapshot
//! with an empty client array.

use std::fs;

use serde_json::json;
use unifi_apclients_mqtt::mapper::map_ap_snapshot;

fn fixture(name: &str) -> String {
    fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture should be present")
}

#[test]
fn selects_only_clients_associated_with_requested_ap() {
    let snapshot = map_ap_snapshot(
        "11:22:33:44:55:66",
        "Living Room AP",
        &fixture("unifi_clients.json"),
    )
    .expect("valid UniFi response should map");

    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        json!({
            "available": true,
            "ap_mac": "11:22:33:44:55:66",
            "ap_name": "Living Room AP",
            "clients": [
                {
                    "mac": "AA:AA:AA:AA:AA:01",
                    "name": "living-room-phone",
                    "hostname": "living-room-phone",
                    "ip": "192.0.2.21"
                },
                {
                    "mac": "AA:AA:AA:AA:AA:03",
                    "name": "living-room-tablet",
                    "hostname": null,
                    "ip": "192.0.2.23"
                }
            ]
        })
    );
}

#[test]
fn treats_ap_mac_matching_as_case_insensitive() {
    let lowercase_ap_response =
        fixture("unifi_clients.json").replace("11:22:33:44:55:66", "aa:bb:cc:dd:ee:ff");
    let snapshot = map_ap_snapshot(
        "AA:BB:CC:DD:EE:FF",
        "Living Room AP",
        &lowercase_ap_response,
    )
    .expect("valid UniFi response should map");

    let value = serde_json::to_value(snapshot).unwrap();
    assert_eq!(value["clients"].as_array().unwrap().len(), 2);
}

#[test]
fn maps_a_successful_empty_response_to_an_available_empty_snapshot() {
    let snapshot = map_ap_snapshot(
        "11:22:33:44:55:66",
        "Living Room AP",
        &fixture("unifi_clients_empty.json"),
    )
    .expect("valid empty UniFi response should map");

    let value = serde_json::to_value(snapshot).unwrap();
    assert_eq!(value["available"], true);
    assert_eq!(value["clients"], json!([]));
}

#[test]
fn rejects_a_client_response_that_reports_an_api_error() {
    let error = map_ap_snapshot(
        "11:22:33:44:55:66",
        "Living Room AP",
        r#"{"meta":{"rc":"error"},"data":[]}"#,
    )
    .expect_err("an unsuccessful API response must not become an empty snapshot");

    assert!(error.to_string().contains("unsuccessful client response"));
}
