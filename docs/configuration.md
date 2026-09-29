# Configuration

[Documentation home](index.md) · [← Installation](installation.md) · [Troubleshooting →](troubleshooting.md)

Set these values in the container environment. Use either the direct password
variable or its `_FILE` counterpart. The service reads files at startup and
does not log their contents.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `UNIFI_URL` | Yes | — | HTTPS URL of the UniFi Network application, including a non-default port if needed. |
| `UNIFI_USERNAME` | Yes | — | Dedicated read-only UniFi account. |
| `UNIFI_PASSWORD` or `UNIFI_PASSWORD_FILE` | Yes | — | Password value or path to the mounted password file. |
| `UNIFI_AP_MACS` | Yes | — | Comma-separated MAC addresses for the access points to publish. |
| `UNIFI_POLL_INTERVAL_SECS` | No | `5` | Poll interval in seconds; must be greater than zero. |
| `UNIFI_TLS_INSECURE` | No | `false` | Set `true` only when you explicitly accept an unverified controller certificate. HTTPS encryption remains enabled. |
| `MQTT_HOST` | Yes | — | Broker hostname or IP reachable from the container. |
| `MQTT_PORT` | Yes | — | Broker TCP port. |
| `MQTT_USERNAME` | No | — | MQTT username when broker authentication is enabled. |
| `MQTT_PASSWORD` or `MQTT_PASSWORD_FILE` | No | — | Password value or mounted-file path; a password requires `MQTT_USERNAME`. |
| `MQTT_BASE_TOPIC` | No | `unifi/apclients` | Shared topic root for service status and per-AP state and availability. Do not include MQTT wildcards. |
| `HOMEASSISTANT_DISCOVERY_ENABLED` | No | `true` | Publish Home Assistant MQTT Discovery configuration. Accepts `true` or `false`. |
| `HOMEASSISTANT_DISCOVERY_PREFIX` | No | `homeassistant` | Home Assistant MQTT Discovery prefix; must match the MQTT integration setting. |
| `HOMEASSISTANT_STATUS_TOPIC` | No | `homeassistant/status` | Topic where Home Assistant publishes its online birth message; discovery is replayed when `online` is received. |

An empty direct password variable is treated as unset, which lets an `.env` file
be shared by the environment and secret-file deployment examples. If both a
non-empty direct value and a file path are set, startup fails rather than
choosing one silently. A secret file can end with LF or CRLF; those trailing
line endings are removed.

## Home Assistant MQTT entities

The service publishes one sensor per configured AP through MQTT Discovery. The
sensor state is the associated-client count; attributes include the AP name,
AP MAC, and client details. Client device identifiers are therefore present in
the MQTT payloads and should be protected by broker access controls.

| Purpose | Topic |
| --- | --- |
| AP state snapshot | `<MQTT_BASE_TOPIC>/<ap-mac-without-colons>/state` |
| AP availability | `<MQTT_BASE_TOPIC>/<ap-mac-without-colons>/availability` |
| Service availability | `<MQTT_BASE_TOPIC>/status` |
| Home Assistant discovery | `<HOMEASSISTANT_DISCOVERY_PREFIX>/sensor/unifi_apclients/<id>/config` |

State and discovery messages are retained. Home Assistant's `online` birth
message prompts the service to replay discovery and cached snapshots. Setting
`HOMEASSISTANT_DISCOVERY_ENABLED=false` stops discovery publication but does
not remove discovery messages already retained by the broker; remove those
retained config messages explicitly if you also want the HA entities removed.
A successful empty API response publishes an empty client list. API errors
mark availability offline and keep the last retained state, rather than
replacing it with fabricated empty data. The client list and AP attributes are described in the
[MQTT architecture](architecture.md#mqtt-contracts).
