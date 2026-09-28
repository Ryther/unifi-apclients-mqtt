# Architecture

[Documentation home](index.md) · [← Troubleshooting](troubleshooting.md) · [Releasing →](releasing.md)

The service polls UniFi Network, maps client records to configured APs, and
publishes retained MQTT state plus Home Assistant MQTT Discovery configuration.
All UniFi operations are read-only apart from the login request.

## Runtime flow

1. `config.rs` validates the HTTPS controller URL, credentials, AP MACs, poll
   interval, and MQTT settings. UniFi and MQTT passwords may be supplied as
   environment values or loaded from mounted secret files with `*_FILE`.
2. `unifi.rs` logs in through `/api/login` and reads
   `/api/s/default/stat/device` and `/api/s/default/stat/sta`.
3. `mapper.rs` associates clients through `ap_mac`, sorts/deduplicates them,
   and creates one snapshot per available configured AP.
4. `mqtt.rs` publishes retained JSON state, per-AP availability, global
   availability, and one discovery sensor per AP. Cached discovery and state
   are replayed after reconnect.
5. `main.rs` drives polling and reports offline availability on API errors or
   clean shutdown without replacing the last good snapshot.

## Failure behavior

A valid response with no clients is an available empty snapshot. A configured
AP missing from `stat/device` is marked unavailable without affecting other
APs. An HTTP, authentication, or API-level failure marks every configured AP
unavailable. In either failure case, retained client state is preserved so an
empty response is never fabricated from an API error.

## MQTT contracts

Each AP has a stable state and availability topic derived from its MAC. Home
Assistant creates one sensor whose state is the client count; the JSON payload
also supplies AP and client attributes. Discovery uses both global service and
per-AP availability with `availability_mode: all`.

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
