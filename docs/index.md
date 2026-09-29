# UniFi AP Clients MQTT

This service reads client and device statistics from UniFi Network and publishes
retained per-access-point client snapshots over MQTT. Home Assistant discovers
one sensor per configured access point.

The service only reads UniFi statistics after login. Client identifiers and AP
details are published to your MQTT broker; choose a broker and UniFi account
that match your privacy and access requirements.

## What do you want to do?

- **[Install the service](installation.md):** Run the published container or
  build locally, and choose environment variables or Docker Compose secrets.
- **[Configure UniFi, MQTT, and Home Assistant](configuration.md):** Set the
  required values and understand discovery, state, and availability topics.
- **[Fix a problem](troubleshooting.md):** Check controller access, secret-file
  permissions, broker connectivity, and Home Assistant discovery.
- **[Agent setup guide](https://github.com/Ryther/unifi-apclients-mqtt/blob/main/.agents/skills/unifi-apclients-mqtt-guide/SKILL.md):** Give an AI assistant a self-contained guide to installation,
  configuration, and troubleshooting.
- **[Understand the runtime](architecture.md):** Follow the API, mapping,
  publishing, and recovery boundaries.
- **[Contribute or release](releasing.md):** Review versioning and publication;
  local development checks are in the repository's
  [contribution guide](https://github.com/Ryther/unifi-apclients-mqtt/blob/main/CONTRIBUTING.md).

The [README](https://github.com/Ryther/unifi-apclients-mqtt#readme) provides a
short project overview and a Compose example. These guides track `main`; use the
[release notes](https://github.com/Ryther/unifi-apclients-mqtt/releases) to
check what changed between versions.
