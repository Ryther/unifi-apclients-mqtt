use std::{collections::HashMap, path::PathBuf, time::Duration};

use thiserror::Error;
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub unifi_url: String,
    pub unifi_username: String,
    pub unifi_password: String,
    pub ap_macs: Vec<String>,
    pub poll_interval: Duration,
    pub unifi_tls_insecure: bool,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub mqtt_base_topic: String,
    pub homeassistant_discovery_enabled: bool,
    pub homeassistant_discovery_prefix: String,
    pub homeassistant_status_topic: String,
    pub client_history_db: Option<PathBuf>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid value for environment variable {0}")]
    Invalid(&'static str),
    #[error("set either {0} or its _FILE variant, not both")]
    ConflictingSecretSources(&'static str),
}

impl Config {
    pub fn from_env<I, K, V>(env: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let env: HashMap<String, String> = env
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();

        let unifi_url = required(&env, "UNIFI_URL")?;
        let parsed_url = Url::parse(&unifi_url).map_err(|_| ConfigError::Invalid("UNIFI_URL"))?;
        if parsed_url.scheme() != "https" || parsed_url.host().is_none() {
            return Err(ConfigError::Invalid("UNIFI_URL"));
        }

        let ap_macs = required(&env, "UNIFI_AP_MACS")?
            .split(',')
            .map(|mac| normalize_mac(mac.trim()))
            .collect::<Result<Vec<_>, _>>()?;

        let poll_interval = env
            .get("UNIFI_POLL_INTERVAL_SECS")
            .map(|value| {
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|secs| *secs > 0)
                    .map(Duration::from_secs)
                    .ok_or(ConfigError::Invalid("UNIFI_POLL_INTERVAL_SECS"))
            })
            .unwrap_or(Ok(Duration::from_secs(5)))?;

        let unifi_tls_insecure = env
            .get("UNIFI_TLS_INSECURE")
            .map(|value| match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(ConfigError::Invalid("UNIFI_TLS_INSECURE")),
            })
            .unwrap_or(Ok(false))?;

        let mqtt_port = required(&env, "MQTT_PORT")?
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or(ConfigError::Invalid("MQTT_PORT"))?;

        let mqtt_host = required(&env, "MQTT_HOST")?;

        let mqtt_base_topic = topic_setting(&env, "MQTT_BASE_TOPIC", "unifi/apclients")?;
        let homeassistant_discovery_enabled = env
            .get("HOMEASSISTANT_DISCOVERY_ENABLED")
            .map(|value| match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(ConfigError::Invalid("HOMEASSISTANT_DISCOVERY_ENABLED")),
            })
            .unwrap_or(Ok(true))?;
        let homeassistant_discovery_prefix =
            topic_setting(&env, "HOMEASSISTANT_DISCOVERY_PREFIX", "homeassistant")?;
        let homeassistant_status_topic =
            topic_setting(&env, "HOMEASSISTANT_STATUS_TOPIC", "homeassistant/status")?;
        let client_history_db = optional(&env, "CLIENT_HISTORY_DB").map(PathBuf::from);

        let mqtt_username = optional(&env, "MQTT_USERNAME");
        let mqtt_password = secret_value(&env, "MQTT_PASSWORD", "MQTT_PASSWORD_FILE")?;
        if mqtt_password.is_some() && mqtt_username.is_none() {
            return Err(ConfigError::Invalid("MQTT_PASSWORD"));
        }

        Ok(Self {
            unifi_url: unifi_url.trim_end_matches('/').to_owned(),
            unifi_username: required(&env, "UNIFI_USERNAME")?,
            unifi_password: required_secret(&env, "UNIFI_PASSWORD", "UNIFI_PASSWORD_FILE")?,
            ap_macs,
            poll_interval,
            unifi_tls_insecure,
            mqtt_host,
            mqtt_port,
            mqtt_username,
            mqtt_password,
            mqtt_base_topic,
            homeassistant_discovery_enabled,
            homeassistant_discovery_prefix,
            homeassistant_status_topic,
            client_history_db,
        })
    }
}

fn required(env: &HashMap<String, String>, name: &'static str) -> Result<String, ConfigError> {
    env.get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(ConfigError::Missing(name))
}

fn required_secret(
    env: &HashMap<String, String>,
    name: &'static str,
    file_name: &'static str,
) -> Result<String, ConfigError> {
    secret_value(env, name, file_name)?.ok_or(ConfigError::Missing(name))
}

fn secret_value(
    env: &HashMap<String, String>,
    name: &'static str,
    file_name: &'static str,
) -> Result<Option<String>, ConfigError> {
    let value = optional(env, name);
    let path = optional(env, file_name);
    if value.is_some() && path.is_some() {
        return Err(ConfigError::ConflictingSecretSources(name));
    }

    if let Some(path) = path {
        let contents =
            std::fs::read_to_string(path).map_err(|_| ConfigError::Invalid(file_name))?;
        let secret = contents.trim_end_matches(['\r', '\n']).to_owned();
        return Ok((!secret.trim().is_empty()).then_some(secret));
    }

    Ok(value)
}

fn optional(env: &HashMap<String, String>, name: &str) -> Option<String> {
    env.get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
}

fn topic_setting(
    env: &HashMap<String, String>,
    name: &'static str,
    default: &'static str,
) -> Result<String, ConfigError> {
    let value = env
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default)
        .trim();
    if value.is_empty() || value.contains(['+', '#', '\0']) {
        return Err(ConfigError::Invalid(name));
    }
    Ok(value.to_owned())
}

fn normalize_mac(value: &str) -> Result<String, ConfigError> {
    let valid = value.len() == 17
        && value
            .split(':')
            .all(|octet| octet.len() == 2 && octet.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid {
        return Err(ConfigError::Invalid("UNIFI_AP_MACS"));
    }
    Ok(value.to_ascii_lowercase())
}
