use std::collections::HashMap;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    config::Config,
    mapper::{ApSnapshot, MapperError, map_ap_snapshot},
};

pub struct UniFiClient {
    http: Client,
    base_url: String,
    username: String,
    password: String,
    authenticated: bool,
}

#[derive(Debug, Error)]
pub enum UniFiError {
    #[error("could not build UniFi HTTP client: {0}")]
    Client(#[from] reqwest::Error),
    #[error("UniFi API returned HTTP status {0}")]
    HttpStatus(StatusCode),
    #[error("UniFi API rejected the request: {0}")]
    Api(String),
    #[error("invalid UniFi API response: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Mapping(#[from] MapperError),
}

impl UniFiError {
    /// Return an operational summary that cannot expose controller URLs or API payloads.
    pub fn safe_summary(&self) -> String {
        match self {
            Self::Client(_) => "UniFi HTTP client or request failure".to_owned(),
            Self::HttpStatus(status) => format!("UniFi HTTP status {status}"),
            Self::Api(_) => "UniFi API rejected the request".to_owned(),
            Self::Json(_) => "invalid UniFi API JSON response".to_owned(),
            Self::Mapping(_) => "invalid UniFi API data".to_owned(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SnapshotBatch {
    pub snapshots: Vec<ApSnapshot>,
    pub unavailable_aps: Vec<String>,
}

impl UniFiClient {
    pub fn new(config: &Config) -> Result<Self, UniFiError> {
        let http = Client::builder()
            .cookie_store(true)
            .danger_accept_invalid_certs(config.unifi_tls_insecure)
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        Ok(Self {
            http,
            base_url: config.unifi_url.trim_end_matches('/').to_owned(),
            username: config.unifi_username.clone(),
            password: config.unifi_password.clone(),
            authenticated: false,
        })
    }

    pub async fn fetch_snapshots(
        &mut self,
        ap_macs: &[String],
    ) -> Result<SnapshotBatch, UniFiError> {
        let devices = self.get_json("/api/s/default/stat/device").await?;
        let clients = self.get_json("/api/s/default/stat/sta").await?;

        let mut names = HashMap::new();
        for device in devices["data"].as_array().into_iter().flatten() {
            if let Some(mac) = device["mac"].as_str() {
                let name = device["name"]
                    .as_str()
                    .or_else(|| device["name_override"].as_str())
                    .unwrap_or(mac);
                names.insert(mac.to_ascii_lowercase(), name.to_owned());
            }
        }

        let client_payload = serde_json::to_string(&clients)?;
        let mut batch = SnapshotBatch::default();
        for ap_mac in ap_macs {
            let normalized_mac = ap_mac.to_ascii_lowercase();
            let Some(name) = names.get(&normalized_mac) else {
                batch.unavailable_aps.push(ap_mac.clone());
                continue;
            };
            batch
                .snapshots
                .push(map_ap_snapshot(ap_mac, name, &client_payload)?);
        }
        Ok(batch)
    }

    async fn get_json(&mut self, path: &str) -> Result<Value, UniFiError> {
        if !self.authenticated {
            self.login().await?;
        }

        let mut response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .send()
            .await?;
        if response.status() == StatusCode::UNAUTHORIZED {
            self.authenticated = false;
            self.login().await?;
            response = self
                .http
                .get(format!("{}{path}", self.base_url))
                .send()
                .await?;
        }

        let status = response.status();
        if !status.is_success() {
            return Err(UniFiError::HttpStatus(status));
        }
        let value: Value = response.json().await?;
        check_api_response(&value)?;
        Ok(value)
    }

    async fn login(&mut self) -> Result<(), UniFiError> {
        let response = self
            .http
            .post(format!("{}/api/login", self.base_url))
            .json(&json!({"username": self.username, "password": self.password}))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(UniFiError::HttpStatus(status));
        }
        let value: Value = response.json().await?;
        check_api_response(&value)?;
        self.authenticated = true;
        Ok(())
    }
}

fn check_api_response(value: &Value) -> Result<(), UniFiError> {
    let Some(meta) = value.get("meta") else {
        return Ok(());
    };
    let Some(result) = meta.get("rc").and_then(Value::as_str) else {
        return Ok(());
    };
    if result == "ok" {
        Ok(())
    } else {
        Err(UniFiError::Api(
            meta.get("msg")
                .and_then(Value::as_str)
                .unwrap_or("unsuccessful response")
                .to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;
    use serde_json::Value;

    use crate::mapper::MapperError;

    use super::UniFiError;

    #[test]
    fn safe_summaries_cover_api_failure_kinds_without_exposing_payloads() {
        let errors = [
            (
                UniFiError::HttpStatus(StatusCode::BAD_GATEWAY),
                "UniFi HTTP status 502 Bad Gateway",
            ),
            (
                UniFiError::Api("controller.example.test: secret response".to_owned()),
                "UniFi API rejected the request",
            ),
            (
                UniFiError::Json(
                    serde_json::from_str::<Value>("{").expect_err("the fixture is malformed JSON"),
                ),
                "invalid UniFi API JSON response",
            ),
            (
                UniFiError::Mapping(MapperError::UnsuccessfulResponse),
                "invalid UniFi API data",
            ),
        ];

        for (error, expected) in errors {
            let summary = error.safe_summary();
            assert_eq!(summary, expected);
            assert!(!summary.contains("controller.example.test"));
            assert!(!summary.contains("secret response"));
        }
    }
}
