use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Deserialize)]
struct UniFiClientsResponse {
    data: Vec<UniFiClient>,
    #[serde(default)]
    meta: Option<UniFiMeta>,
}

#[derive(Debug, Deserialize)]
struct UniFiMeta {
    rc: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UniFiClient {
    mac: Option<String>,
    ap_mac: Option<String>,
    name: Option<String>,
    hostname: Option<String>,
    ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApSnapshot {
    pub available: bool,
    pub ap_mac: String,
    pub ap_name: String,
    pub clients: Vec<ClientSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClientSnapshot {
    pub mac: String,
    pub name: String,
    pub hostname: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Error)]
pub enum MapperError {
    #[error("invalid UniFi client response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("UniFi returned an unsuccessful client response")]
    UnsuccessfulResponse,
}

pub fn map_ap_snapshot(
    ap_mac: &str,
    ap_name: &str,
    raw_unifi_clients_json: &str,
) -> Result<ApSnapshot, MapperError> {
    let response: UniFiClientsResponse = serde_json::from_str(raw_unifi_clients_json)?;
    if response
        .meta
        .and_then(|meta| meta.rc)
        .is_some_and(|result| result != "ok")
    {
        return Err(MapperError::UnsuccessfulResponse);
    }

    let ap_mac = ap_mac.to_ascii_lowercase();
    let mut clients: Vec<ClientSnapshot> = response
        .data
        .into_iter()
        .filter(|client| {
            client
                .ap_mac
                .as_deref()
                .is_some_and(|mac| mac.eq_ignore_ascii_case(&ap_mac))
        })
        .filter_map(|client| {
            let mac = client.mac?;
            let name = client
                .name
                .clone()
                .or_else(|| client.hostname.clone())
                .unwrap_or_else(|| mac.clone());
            Some(ClientSnapshot {
                mac,
                name,
                hostname: client.hostname,
                ip: client.ip,
            })
        })
        .collect();
    clients.sort_by(|left, right| left.mac.cmp(&right.mac));
    clients.dedup_by(|left, right| left.mac.eq_ignore_ascii_case(&right.mac));

    Ok(ApSnapshot {
        available: true,
        ap_mac,
        ap_name: ap_name.to_owned(),
        clients,
    })
}
