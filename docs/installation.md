# Installation

[Documentation home](index.md) · [Configuration →](configuration.md)

The service runs as a Docker container. It needs network access to a UniFi
Network application over HTTPS and an MQTT broker. Create a UniFi account with
read access to client and device statistics; this service does not control
UniFi devices.

## Run with Docker Compose

1. Copy `.env.example` to `.env` and set the controller URL, read-only username,
   AP MAC addresses, and MQTT connection details.
2. Set `UNIFI_PASSWORD` directly for this local build. For a deployment using
   file-backed secrets, follow
   [Use Docker Compose secrets](#use-docker-compose-secrets) and use its
   separate Compose file and startup command.
3. Make sure the container can reach both the controller and broker. If they
   are on a user-defined Docker network, attach the service to that network and
   use the reachable service names in `UNIFI_URL` and `MQTT_HOST`.
4. Start the local build:

   ```sh
   docker compose up --build -d
   docker compose logs -f unifi-apclients-mqtt
   ```

The repository Compose file builds from source. To use the published image,
replace `build: .` with:

```yaml
image: ghcr.io/ryther/unifi-apclients-mqtt:latest
```

Use a versioned image tag when you want upgrades to happen only when you choose.

## Use Docker Compose secrets

The service accepts `UNIFI_PASSWORD_FILE` and `MQTT_PASSWORD_FILE` paths. These
point to files mounted by Docker; they are not passed as password environment
values. Keep the corresponding direct variables empty in `.env` and set the
file variables in the service configuration:

The repository-root
[`compose.secrets.example.yaml`](https://github.com/Ryther/unifi-apclients-mqtt/blob/main/compose.secrets.example.yaml)
is a complete starting point for this setup.

```yaml
services:
  unifi-apclients-mqtt:
    image: ghcr.io/ryther/unifi-apclients-mqtt:latest
    env_file:
      - .env
    environment:
      UNIFI_PASSWORD_FILE: /run/secrets/unifi_password
      MQTT_PASSWORD_FILE: /run/secrets/mqtt_password
      CLIENT_HISTORY_DB: /data/client-history.db
    volumes:
      - ./data:/data
    secrets:
      - unifi_password
      - mqtt_password
    restart: unless-stopped

secrets:
  unifi_password:
    file: ./secrets/unifi_password
  mqtt_password:
    file: ./secrets/mqtt_password
```

Create the source files outside version control. For file-backed Compose
secrets, Compose mounts the host files directly; `uid`, `gid`, and `mode`
overrides are not applied. The image runs as UID/GID `10001`, so make each file
readable by that identity and restrict access on the host. For example, after
creating the files with your preferred secret manager or editor:

```sh
sudo chown 10001:10001 secrets/unifi_password secrets/mqtt_password
sudo chmod 0400 secrets/unifi_password secrets/mqtt_password
```

The Eligible clients sensor also needs a persistent writable directory. Create
it before starting the example Compose service:

```sh
mkdir -p data
sudo chown 10001:10001 data
sudo chmod 0700 data
```

The example mounts `./data` at `/data` and sets `CLIENT_HISTORY_DB` to
`/data/client-history.db`. The SQLite database contains client MAC addresses and
observation times. The service creates it with owner-only permissions and
refuses to start if it cannot read or write the history. Keep `data/` out of
Git and back it up if preserving the accumulated presence history matters.
Stop the service before copying the database file for a backup.

The repository ignores the `secrets/` directory. Do not commit real secret
files. A password file may end in a newline; the service removes the final
line-ending characters when it reads the value.

Start and inspect this image with the Compose file from the repository root:

```sh
docker compose -f compose.secrets.example.yaml up -d
docker compose -f compose.secrets.example.yaml logs -f unifi-apclients-mqtt
```

Set either a password variable or its `_FILE` form, not both. An unreadable,
missing, empty required UniFi password file, or conflicting sources causes
configuration to fail during startup. `MQTT_PASSWORD` is optional; if set, it
requires `MQTT_USERNAME`.

Docker Compose mounts secrets as files under `/run/secrets/`; see the [Docker
Compose secrets guide](https://docs.docker.com/compose/how-tos/use-secrets/) for
the platform behavior and limitations of file-backed secrets.

## Run the image directly

The image also accepts the regular environment variables documented in the
[configuration reference](configuration.md). For example, pass a protected
environment file with `docker run --env-file .env ...`. Environment variables
are convenient but may be visible to processes with access to container
metadata; prefer secret files when your deployment supports them.
