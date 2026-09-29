# Architecture

[Documentation home](index.md) · [← Troubleshooting](troubleshooting.md) · [Releasing →](releasing.md)

The service polls UniFi Network, maps client records to configured APs, and
publishes retained MQTT state plus Home Assistant MQTT Discovery configuration.
All UniFi operations are read-only apart from the login request.

## Runtime flow

1. `config.rs` validates the HTTPS controller URL, credentials, AP MACs, poll
   interval, MQTT connection settings, topic root, and Home Assistant discovery
   settings. UniFi and MQTT passwords may be supplied as environment values or
   loaded from mounted secret files with `*_FILE`.
2. `unifi.rs` logs in through `/api/login` and reads
   `/api/s/default/stat/device` and `/api/s/default/stat/sta`.
3. `mapper.rs` associates clients through `ap_mac`, sorts/deduplicates them,
   and creates one snapshot per available configured AP.
4. When `CLIENT_HISTORY_DB` is configured (by default in the container image),
   `presence_history.rs` stores observed
   client intervals by AP and MAC. A valid absent snapshot records a confirmed
   departure. Poll failures, missing APs, long poll gaps, and restarts break
   continuous observation without earning time or confirming a departure.
5. `mqtt.rs` publishes retained JSON state, per-AP availability, global
   availability, and the AP client sensor. With history enabled it also
   publishes an Eligible clients sensor on the same AP device. It subscribes to Home
   Assistant's status topic and replays discovery and cached state after a
   broker reconnect or Home Assistant's `online` birth message.
6. `main.rs` drives polling and reports offline availability on API errors or
   clean shutdown without replacing the last good snapshot.

## Failure behavior

A valid response with no clients is an available empty snapshot. A configured
AP missing from `stat/device` is marked unavailable without affecting other
APs. An HTTP, authentication, or API-level failure marks every configured AP
unavailable. In either failure case, retained client state is preserved so an
empty response is never fabricated from an API error.

## MQTT contracts

The `MQTT_BASE_TOPIC` setting is shared across the service: each AP has a
stable state and availability topic derived from its MAC, plus one service
availability topic. `HOMEASSISTANT_DISCOVERY_PREFIX` independently controls
where discovery config is published. Home Assistant creates one sensor per AP
whose state is the client count; the JSON payload also supplies AP and client
attributes. Discovery uses both global service and per-AP availability with
`availability_mode: all`.

With history enabled, each AP has an additional Eligible clients sensor at
`<MQTT_BASE_TOPIC>/<ap-id>/eligible/state`. Its retained payload has the same
snapshot shape but contains only currently connected clients that satisfy the
presence policy. A continuous observation needs 24 hours; the alternative
requires five hours within a rolling 48-hour window and at least one confirmed
absence between observed sessions. SQLite commits each AP observation as a
transaction and stores MAC addresses and timestamps, not names or IP addresses.
The database must be stored on persistent writable storage. The original
sensor and topics remain unchanged.

Payload examples and exact topics are documented in the
[repository README](https://github.com/Ryther/unifi-apclients-mqtt#readme).
Do not change topics, unique IDs, retained behavior, or availability semantics
without considering existing Home Assistant entities and retained broker data.

## Test boundaries

UniFi API contract tests use a local TCP fixture and inspect request paths,
session-cookie reuse, and response mapping. MQTT unit tests check topic and
discovery shape. The live MQTT contract test uses only a disposable Mosquitto
broker and verifies retained delivery and replay after reconnect. Tests must not
need household credentials, a real controller, or an existing HA instance.
