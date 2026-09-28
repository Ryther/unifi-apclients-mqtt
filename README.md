# UniFi AP Clients MQTT

[![CI](https://github.com/Ryther/unifi-apclients-mqtt/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/Ryther/unifi-apclients-mqtt/actions/workflows/ci.yaml)
[![CodeQL](https://github.com/Ryther/unifi-apclients-mqtt/actions/workflows/codeql.yaml/badge.svg?branch=main)](https://github.com/Ryther/unifi-apclients-mqtt/actions/workflows/codeql.yaml)
[![Sonar quality gate](https://sonarcloud.io/api/project_badges/measure?project=Ryther_unifi-apclients-mqtt&metric=alert_status)](https://sonarcloud.io/dashboard?id=Ryther_unifi-apclients-mqtt)
[![Documentation](https://img.shields.io/badge/docs-GitHub%20Pages-blue)](https://ryther.github.io/unifi-apclients-mqtt/)
[![Latest release](https://img.shields.io/github/v/release/Ryther/unifi-apclients-mqtt)](https://github.com/Ryther/unifi-apclients-mqtt/releases/latest)
[![License](https://img.shields.io/github/license/Ryther/unifi-apclients-mqtt)](LICENSE)

Small Rust service that polls the UniFi Network API and publishes a retained
client snapshot for configured access points. MQTT Discovery creates one
Home Assistant sensor per AP; the sensor state is the number of associated
clients and its attributes include the AP and client details.

## Configuration

Copy `.env.example` to `.env` and replace every example value with your own.
Configure a
read-only UniFi account with access to client and device statistics. List one
or more AP MAC addresses in `UNIFI_AP_MACS`, separated by commas.

`UNIFI_TLS_INSECURE` defaults to `false`. Set it to `true` for the current
self-signed controller certificate. Requests still use HTTPS encryption, but
the client will not authenticate the controller certificate. Keep this option
on the trusted local network and do not expose the service to untrusted hosts.

The poll interval defaults to five seconds. MQTT credentials are optional; if
you set a password, also set its username.

## MQTT contract

- State: `unifi/apclients/<ap-mac-without-colons>/state` (retained JSON)
- Availability: `unifi/apclients/<ap-mac-without-colons>/availability`
- Global service availability: `unifi/apclients/status`
- Home Assistant discovery: `homeassistant/sensor/unifi_apclients/<id>/config`

Each sensor reports a client count and exposes `ap_mac`, `ap_name`, and
`clients` as attributes. A successful empty UniFi response is published as an
empty client list. A missing configured AP is marked unavailable independently;
a failed UniFi/API request marks every configured AP unavailable. Both cases
leave the last retained snapshots intact. Discovery and cached state are
republished after the MQTT connection is restored.

## Run locally

```sh
docker compose up --build
```

Example Compose service on a Docker network shared with the controller:

```yaml
services:
  unifi-apclients-mqtt:
    image: ghcr.io/ryther/unifi-apclients-mqtt:latest
    container_name: unifi-apclients-mqtt
    restart: unless-stopped
    env_file:
      - .env
    environment:
      UNIFI_URL: https://unifi-network:8443
    networks:
      - controller-net

networks:
  controller-net:
    external: true
```

Replace the example service and network names with the names used by your
controller Compose project. Set `MQTT_HOST` to the broker address reachable
from this container. Keep real credentials in the untracked `.env` file.

## Development and contribution

See [CONTRIBUTING.md](CONTRIBUTING.md) for the Rust toolchain, local checks,
and pull request expectations. [Architecture](docs/architecture.md) describes
the read-only UniFi API and MQTT boundaries; [releasing](docs/releasing.md)
covers versioning and image publication.

## Releases

The Release Please workflow proposes version and changelog updates from
Conventional Commits merged to `main`. Merging its release PR creates a draft;
the exact tagged commit must pass CI and Docker build before GHCR publication
and GitHub release publication. Configure the repository's
`RELEASE_PLEASE_TOKEN` secret so release PRs trigger normal CI.
