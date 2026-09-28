use std::{collections::HashMap, time::Duration};

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
        if ap_macs.is_empty() {
            return Err(ConfigError::Invalid("UNIFI_AP_MACS"));
        }

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
        if mqtt_host.trim().is_empty() {
            return Err(ConfigError::Invalid("MQTT_HOST"));
        }

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
