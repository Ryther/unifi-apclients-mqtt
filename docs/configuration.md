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
| `CLIENT_HISTORY_DB` | No | Disabled | Path to a writable persistent SQLite database for client eligibility. Enables one additional Eligible clients sensor per AP. |
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

When `CLIENT_HISTORY_DB` is set, it publishes another sensor named **Eligible
clients** on each AP device. Its count and `clients` attribute include currently
connected clients that have either 24 hours of continuous observed presence or
five observed hours in the last 48 hours across sessions separated by a valid
snapshot showing the client absent. Poll failures, missing APs, long poll gaps,
and restarts break continuous observation but do not count as a confirmed
absence. The history starts when this feature is enabled; it is not reconstructed
from old MQTT snapshots. The database includes MAC addresses and observation
times, so protect the volume and keep it out of version control.

| Purpose | Topic |
| --- | --- |
| AP state snapshot | `<MQTT_BASE_TOPIC>/<ap-mac-without-colons>/state` |
| Eligible clients snapshot | `<MQTT_BASE_TOPIC>/<ap-mac-without-colons>/eligible/state` |
| AP availability | `<MQTT_BASE_TOPIC>/<ap-mac-without-colons>/availability` |
| Service availability | `<MQTT_BASE_TOPIC>/status` |
| Home Assistant discovery | `<HOMEASSISTANT_DISCOVERY_PREFIX>/sensor/unifi_apclients/<id>/config` |
| Eligible clients discovery | `<HOMEASSISTANT_DISCOVERY_PREFIX>/sensor/unifi_apclients/<id>_eligible/config` |

State and discovery messages are retained. Home Assistant's `online` birth
message prompts the service to replay discovery and cached snapshots. Setting
`HOMEASSISTANT_DISCOVERY_ENABLED=false` stops discovery publication but does
not remove discovery messages already retained by the broker; remove those
retained config messages explicitly if you also want the HA entities removed.
A successful empty API response publishes an empty client list. API errors
mark availability offline and keep the last retained state, rather than
replacing it with fabricated empty data. The client list and AP attributes are described in the
[MQTT architecture](architecture.md#mqtt-contracts).

## Logs

The service writes structured JSON logs to stdout, so Docker captures them with
the container logs. The included Compose example rotates logs at 10 MiB and
keeps three files. The default level is `info`; set `RUST_LOG` in the service
environment to change it, for example `RUST_LOG=debug` while diagnosing a
problem. Remove the override after diagnosis because debug output is more
verbose.

At `info`, startup reports the configured AP count, polling interval and TLS
verification mode. Each successful cycle reports duration and aggregate counts
of available APs, unavailable APs and clients. MQTT connection/reconnection,
queued Home Assistant discovery, cached-state replay and shutdown are also reported.
Warnings identify failed UniFi polls and MQTT operations. UniFi failure logs use
a safe summary rather than the URL or API response. Logs intentionally omit
client details, names, AP identifiers and payloads; aggregate counts are not a
per-client activity history.

Use `docker compose logs -f unifi-apclients-mqtt` to follow the service. Review
logs before sharing them; broker/runtime errors can still contain environmental
details. For the agent-oriented setup and troubleshooting guide, see the
[UniFi AP Clients MQTT guide skill](https://github.com/Ryther/unifi-apclients-mqtt/blob/main/.agents/skills/unifi-apclients-mqtt-guide/SKILL.md).
