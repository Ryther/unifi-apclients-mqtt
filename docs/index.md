# UniFi AP Clients MQTT

This service reads client and device statistics from UniFi Network and publishes
retained per-access-point client snapshots over MQTT. Home Assistant discovers
one sensor per configured access point.

Use the [README](https://github.com/Ryther/unifi-apclients-mqtt#readme) to
configure and run the service. The [architecture](architecture.md) page covers
the API, MQTT topics, and failure behavior. Contributors can find the local
checks in the repository's
[contribution guide](https://github.com/Ryther/unifi-apclients-mqtt/blob/main/CONTRIBUTING.md).
