# Installation

[Documentation home](index.md) · [Configuration →](configuration.md)

Use Docker Engine with Compose on a Linux x86-64 host. You also need a UniFi
Network application exposing the [supported default-site API](configuration.md#connection-compatibility),
a read-only account for its device/client statistics, and a reachable MQTT
broker. Home Assistant needs its MQTT integration connected to that same broker
with discovery enabled. The service has no web interface or inbound port.

MQTT connections are plain TCP. Use a trusted private network; do not point
this service directly at a TLS-only listener. From inside a container,
`localhost` means that container, not the Docker host or another service.

## Run with Docker Compose

For the published image, create a working directory and save this as
`compose.yaml`:

```yaml
services:
  unifi-apclients-mqtt:
    image: ghcr.io/ryther/unifi-apclients-mqtt:latest
    env_file:
      - .env
    volumes:
      - unifi_apclients_history:/data
    restart: unless-stopped
    logging:
      driver: json-file
      options:
        max-size: 10m
        max-file: "3"

volumes:
  unifi_apclients_history:
```

1. Save the repository's [`.env.example`](https://github.com/Ryther/unifi-apclients-mqtt/blob/main/.env.example)
   as `.env` alongside `compose.yaml`. Set the controller URL, read-only username
   and password, AP MAC addresses, and reachable MQTT host/port. Set broker
   credentials if required. Keep `.env` private and outside version control.
2. If the controller or broker is on a user-defined Docker network, attach the
   service to that network before starting; use names reachable on that network.
3. Validate and start from this directory:

   ```sh
   docker compose config --quiet
   docker compose pull
   docker compose up -d
   docker compose logs --tail=100 -f unifi-apclients-mqtt
   ```

Pressing Ctrl+C stops following logs; the detached service keeps running.
`config --quiet` checks the Compose file without printing expanded passwords.
Environment values remain visible to administrators with Docker access; use
[file-backed secrets](#use-docker-compose-secrets) if appropriate.

The runtime image is a static x86-64 musl binary on `scratch`, running as
UID/GID `10001`, with no shell or package manager. Use Docker logs rather than
`docker exec ... sh`. Run one instance per broker because its MQTT client ID
is fixed.

### Confirm the first successful poll

Check `docker compose ps` for a running service, then look for a
`UniFi poll cycle completed` JSON log event. The configured APs should be
counted as available. In Home Assistant's MQTT integration, each AP should have
a client-count sensor, plus **Eligible clients** when history is enabled.
Check availability as well as the number: an offline sensor can retain an old
snapshot. Eligible clients can correctly stay at zero until the presence
threshold is met. See [troubleshooting](troubleshooting.md) for failures.

### Build from a checkout

The repository's own `compose.yaml` uses `build: .` instead of the published
image. In a checkout, configure `.env` as above, then use:

```sh
docker compose config --quiet
docker compose up --build -d
docker compose logs --tail=100 -f unifi-apclients-mqtt
```

Do not mix this source-build file with the image-only upgrade commands below.

## Use Docker Compose secrets

The repository's [complete secrets example](https://github.com/Ryther/unifi-apclients-mqtt/blob/main/compose.secrets.example.yaml)
uses the published image and mounted files instead of password environment
values. Save it alongside `.env` and create the `secrets/` directory. Create
`secrets/unifi_password` and, if the broker requires it, `secrets/mqtt_password`
with your editor or secret manager. Do not put passwords into shell history.

Keep `UNIFI_PASSWORD` and `MQTT_PASSWORD` empty in `.env`. Set `MQTT_USERNAME`
when using broker authentication. If the broker has no authentication, remove
`MQTT_PASSWORD_FILE`, the service's `mqtt_password` secret entry, and the
corresponding top-level secret definition from this example.

For rootful Docker without user-namespace remapping, make the host files
readable only by the image's UID/GID:

```sh
sudo chown 10001:10001 secrets/unifi_password secrets/mqtt_password
sudo chmod 0400 secrets/unifi_password secrets/mqtt_password
```

Omit the MQTT file from those commands if unused. With rootless Docker or
user-namespace remapping, grant access to the corresponding mapped host
identity instead. File-backed Compose secrets use bind mounts: setting
`uid`, `gid`, or `mode` in Compose does not change the host file's ownership.
See [Docker's secrets reference](https://docs.docker.com/reference/compose-file/services/#secrets).
Do not make secret files world-readable to work around an identity mismatch.

Run these commands using the secrets file consistently:

```sh
docker compose -f compose.secrets.example.yaml config --quiet
docker compose -f compose.secrets.example.yaml pull
docker compose -f compose.secrets.example.yaml up -d
docker compose -f compose.secrets.example.yaml logs --tail=100 -f unifi-apclients-mqtt
```

Set either a non-empty password variable or its `_FILE` path. Conflicting
sources, unreadable files, or an empty required UniFi password cause startup
to fail. Trailing LF/CRLF characters are removed. Keep `.env` and `secrets/`
private; both are ignored in the repository.

## Preserve client history

The image sets `CLIENT_HISTORY_DB=/data/client-history.db` and provides `/data`
owned by UID/GID `10001`. Both Compose examples keep it in a named volume.
The database stores client MAC addresses and observation times with owner-only
permissions. Keep this volume across upgrades and restrict Docker access.
An unwritable or corrupt database stops the service.

An empty `CLIENT_HISTORY_DB=` in `.env` overrides the image default and disables
history. Remove that line to enable the image default. Stop the service before
backing up the database with your volume-backup tooling; restart it afterward.
A fresh volume starts a fresh observation history.

## Update the published image

Record the currently deployed image and back up the stopped history volume
if preserving accumulated observations matters. Choose a published version
from [Releases](https://github.com/Ryther/unifi-apclients-mqtt/releases).
Version tags include the prefix, for example
`ghcr.io/ryther/unifi-apclients-mqtt:unifi-apclients-mqtt-v1.0.1`.
Set `image:` to the desired tag, or retain `latest` to follow stable releases.

```sh
docker compose pull
docker compose up -d
docker compose logs --tail=100 unifi-apclients-mqtt
```

For the secrets example, add `-f compose.secrets.example.yaml` after
`docker compose` in each command. Recheck the first-poll and HA checkpoints.
Keep the same Compose project directory/name and volume: changing the project
can select a new empty volume. Reverting the image tag does not roll back
history data; check release notes before reusing a newer database with an older
image, and retain the backup.

## Remove the service

Run `docker compose down` (with the appropriate `-f` option). This removes the
containers and network while retaining the named history volume. Avoid `-v`
unless you intend to delete that history. Keep or securely remove your local
configuration, secrets and volume according to your retention needs.

Stopping or removing the container does not erase broker-retained messages.
To remove its Home Assistant entities, first stop the service so it cannot
republish, then clear each affected retained discovery config using your
broker's authenticated management tools. Publish a zero-byte retained payload
to the exact topics:

- `<discovery-prefix>/sensor/unifi_apclients/<ap-id>/config`
- `<discovery-prefix>/sensor/unifi_apclients/<ap-id>_eligible/config` when enabled

`ap-id` is the AP MAC in lowercase without colons. Clear the associated
`<base>/<ap-id>/state`, `<base>/<ap-id>/eligible/state`,
`<base>/<ap-id>/availability` and `<base>/status` retained topics too if you
want their stored snapshots and availability removed. Never clear a shared
`homeassistant/#` tree; other integrations may use it. Disabling discovery
alone does not remove previously retained configs.

## Run the image directly

For the environment-file option, from a directory containing your protected
`.env`, use a named volume just as Compose does:

```sh
docker run -d --name unifi-apclients-mqtt \
  --restart unless-stopped \
  --env-file .env \
  --mount type=volume,source=unifi_apclients_history,target=/data \
  --log-driver json-file --log-opt max-size=10m --log-opt max-file=3 \
  ghcr.io/ryther/unifi-apclients-mqtt:latest
docker logs --tail=100 unifi-apclients-mqtt
```

This creates a separate volume from a project-prefixed Compose volume unless
you deliberately use the same actual volume name. Use the same network and
history checks described above. The image opens no inbound service port.
