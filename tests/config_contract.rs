//! Configuration contract tests.
//!
//! Expected production interface:
//!
//! * `unifi_apclients_mqtt::config::Config::from_env` accepts an iterable of
//!   `(key, value)` pairs and returns `Result<Config, ConfigError>`.
//! * Required settings are `UNIFI_URL`, `UNIFI_USERNAME`, a UniFi password
//!   value or file, `UNIFI_AP_MACS`, `MQTT_HOST`, and `MQTT_PORT`.
//! * `UNIFI_AP_MACS` is a comma separated list of MAC addresses.
//! * `UNIFI_POLL_INTERVAL_SECS` defaults to 5; `UNIFI_TLS_INSECURE` defaults
//!   to false and accepts `true`/`false`.
//! * Optional MQTT credentials are `MQTT_USERNAME` and a password value or
//!   file.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use unifi_apclients_mqtt::config::Config;

fn valid_env() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("UNIFI_URL", "https://controller.example.test:8443"),
        ("UNIFI_USERNAME", "collector"),
        ("UNIFI_PASSWORD", "secret"),
        ("UNIFI_AP_MACS", "11:22:33:44:55:66,22:33:44:55:66:77"),
        ("UNIFI_POLL_INTERVAL_SECS", "15"),
        ("MQTT_HOST", "mqtt.example.test"),
        ("MQTT_PORT", "1883"),
        ("MQTT_USERNAME", "publisher"),
        ("MQTT_PASSWORD", "mqtt-secret"),
    ])
}

fn unique_secret_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "unifi-apclients-mqtt-{name}-{}-{}.secret",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos()
    ))
}

#[test]
fn parses_required_and_optional_settings() {
    let config = Config::from_env(valid_env()).expect("valid environment should parse");

    assert_eq!(config.unifi_url, "https://controller.example.test:8443");
    assert_eq!(config.unifi_username, "collector");
    assert_eq!(config.unifi_password, "secret");
    assert_eq!(
        config.ap_macs,
        vec!["11:22:33:44:55:66", "22:33:44:55:66:77"]
    );
    assert_eq!(config.poll_interval, Duration::from_secs(15));
    assert_eq!(config.mqtt_host, "mqtt.example.test");
    assert_eq!(config.mqtt_port, 1883);
    assert_eq!(config.mqtt_username.as_deref(), Some("publisher"));
    assert_eq!(config.mqtt_password.as_deref(), Some("mqtt-secret"));
}

#[test]
fn verifies_tls_by_default_and_allows_explicit_opt_in_to_insecure_tls() {
    let mut secure_env = valid_env();
    secure_env.remove("UNIFI_POLL_INTERVAL_SECS");
    let secure = Config::from_env(secure_env).expect("valid environment should parse");
    assert!(!secure.unifi_tls_insecure);
    assert_eq!(secure.poll_interval, Duration::from_secs(5));

    let mut insecure_env = valid_env();
    insecure_env.insert("UNIFI_TLS_INSECURE", "true");
    let insecure = Config::from_env(insecure_env).expect("explicit opt in should parse");
    assert!(insecure.unifi_tls_insecure);
}

#[test]
fn rejects_missing_required_settings_and_invalid_values() {
    let mut missing_url = valid_env();
    missing_url.remove("UNIFI_URL");
    assert!(Config::from_env(missing_url).is_err());

    let mut blank_mqtt_host = valid_env();
    blank_mqtt_host.insert("MQTT_HOST", "  ");
    assert!(Config::from_env(blank_mqtt_host).is_err());

    let mut bad_port = valid_env();
    bad_port.insert("MQTT_PORT", "not-a-port");
    assert!(Config::from_env(bad_port).is_err());

    let mut bad_bool = valid_env();
    bad_bool.insert("UNIFI_TLS_INSECURE", "yes");
    assert!(Config::from_env(bad_bool).is_err());

    let mut bad_ap_mac = valid_env();
    bad_ap_mac.insert("UNIFI_AP_MACS", "not-a-mac");
    assert!(Config::from_env(bad_ap_mac).is_err());

    let mut plaintext_url = valid_env();
    plaintext_url.insert("UNIFI_URL", "http://controller.example.test:8443");
    assert!(Config::from_env(plaintext_url).is_err());

    let mut password_without_username = valid_env();
    password_without_username.remove("MQTT_USERNAME");
    assert!(Config::from_env(password_without_username).is_err());
}

#[test]
fn reads_passwords_from_secret_files_and_removes_one_trailing_line_ending() {
    let unifi_secret = unique_secret_path("unifi");
    let mqtt_secret = unique_secret_path("mqtt");
    fs::write(&unifi_secret, "unifi-secret\r\n").expect("write UniFi secret fixture");
    fs::write(&mqtt_secret, "mqtt-secret\n").expect("write MQTT secret fixture");

    let mut env: HashMap<String, String> = valid_env()
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    env.remove("UNIFI_PASSWORD");
    env.remove("MQTT_PASSWORD");
    env.insert(
        "UNIFI_PASSWORD_FILE".to_owned(),
        unifi_secret.display().to_string(),
    );
    env.insert(
        "MQTT_PASSWORD_FILE".to_owned(),
        mqtt_secret.display().to_string(),
    );

    let config = Config::from_env(env).expect("secret files should satisfy password settings");

    assert_eq!(config.unifi_password, "unifi-secret");
    assert_eq!(config.mqtt_password.as_deref(), Some("mqtt-secret"));
    fs::remove_file(unifi_secret).expect("remove UniFi secret fixture");
    fs::remove_file(mqtt_secret).expect("remove MQTT secret fixture");
}

#[test]
fn rejects_password_set_as_both_environment_value_and_secret_file() {
    let secret = unique_secret_path("conflict");
    fs::write(&secret, "file-secret").expect("write secret fixture");

    let mut env: HashMap<String, String> = valid_env()
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    env.insert(
        "UNIFI_PASSWORD_FILE".to_owned(),
        secret.display().to_string(),
    );

    let result = Config::from_env(env);

    assert!(
        result.is_err(),
        "ambiguous password sources must be rejected"
    );
    fs::remove_file(secret).expect("remove secret fixture");
}
