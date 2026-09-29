# Troubleshooting

[Documentation home](index.md) · [← Configuration](configuration.md) · [Architecture →](architecture.md)

Start with the service logs and check configuration values without copying
passwords or client payloads into an issue. For the default Compose file, run:

```sh
docker compose logs --tail=100 unifi-apclients-mqtt
docker compose config
```

Logs are JSON on stdout. A successful poll cycle reports duration plus
available AP, unavailable AP and client counts; connection and replay events describe MQTT
recovery. Temporarily set `RUST_LOG=debug` for more detail. The example Compose
configuration limits local Docker log storage by rotating at 10 MiB and keeping
three files. Review output before sharing it; never include client details,
hostnames, IP addresses, AP/client MACs or credentials.

For the secret-file example, add `-f compose.secrets.example.yaml` to both
commands.

## Container exits during startup

- Check that all required variables in the
  [configuration reference](configuration.md) are set.
- For secret files, confirm that the configured `*_FILE` path exists inside the
  container and is readable by UID/GID `10001`.
- Set either `UNIFI_PASSWORD` or `UNIFI_PASSWORD_FILE`; do not set both. The
  same rule applies to the MQTT password pair.
- If using MQTT authentication, provide both `MQTT_USERNAME` and a non-empty
  `MQTT_PASSWORD` or `MQTT_PASSWORD_FILE`.
- The published image sets `CLIENT_HISTORY_DB=/data/client-history.db` by
  default. Confirm `/data` is mounted on durable storage and writable by
  UID/GID `10001`. Remove an empty `CLIENT_HISTORY_DB=` entry from `.env`,
  because it overrides the image default. A corrupt or unwritable history
  database prevents startup or stops the service, so no client can become
  eligible from incomplete history.

## UniFi shows authentication or connection errors

- Confirm that the controller URL uses `https://`, the host and port are
  reachable from the container, and the account can read client and device
  statistics.
- The service validates the controller certificate by default. Install a
  publicly trusted certificate when possible. The `scratch` image has no OS
  certificate store; `reqwest` uses the WebPKI roots bundled by its `rustls-tls`
  feature. A private CA is not trusted merely by mounting its certificate into
  the container. `UNIFI_TLS_INSECURE=true` is an explicit opt-in to skipping
  certificate validation; use it only on a network you trust.
- Check that every configured AP MAC is correct. A missing AP affects that AP
  only; it is marked unavailable while other configured APs continue.

## MQTT or Home Assistant entities are missing

- Check broker reachability from the container, `MQTT_PORT`, and optional
  credentials.
- Confirm the broker permits the service to publish under the configured
  `MQTT_BASE_TOPIC` and `HOMEASSISTANT_DISCOVERY_PREFIX` paths, and to subscribe
  to `HOMEASSISTANT_STATUS_TOPIC`.
- Home Assistant needs its MQTT integration connected to the same broker and
  MQTT Discovery enabled.
- Look for the retained discovery config under
  `<HOMEASSISTANT_DISCOVERY_PREFIX>/sensor/unifi_apclients/<id>/config`, then
  inspect the matching AP state and availability topics from the
  [configuration reference](configuration.md#home-assistant-mqtt-entities).
- The Eligible clients sensor appears when `CLIENT_HISTORY_DB` is configured;
  the published image configures it by default.
  Check its separate retained discovery and state topics in the configuration
  reference. Its count can stay at zero until a client meets the presence rule.

## A snapshot appears stale

On UniFi/API failure the service marks AP availability offline and preserves
the last retained state. A successful response with no associated clients
publishes an empty client list. Check availability before interpreting a
retained snapshot as current.

Before sharing logs or broker diagnostics, remove passwords, usernames,
controller hostnames, AP MAC addresses, and client details.

The runtime image has no shell, so `docker exec ... sh` cannot be used for
diagnosis. Use Docker logs or a separate diagnostic container to inspect a
mounted volume.

## SonarCloud badge says the quality gate is not computed

This badge follows the main branch configured in SonarCloud. If the repository's
GitHub default branch is `main`, make sure SonarCloud also marks `main` as the
project's main branch. Analyses may pass on a secondary SonarCloud branch while
the badge for its configured main branch remains uncomputed. See SonarCloud's
[first analysis guide](https://docs.sonarsource.com/sonarqube-cloud/getting-started/first-analysis)
for how it displays main-branch analysis results.
