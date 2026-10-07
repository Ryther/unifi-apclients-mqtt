# Changelog

## [2.0.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v1.0.1...unifi-apclients-mqtt-v2.0.0) (2026-10-07)


### ⚠ BREAKING CHANGES

* The runtime image has no shell or OS certificate store, and is built for x86-64 only. Mounted private CA files are not automatically trusted.

### Features

* add eligible clients with SQLite presence history ([#28](https://github.com/Ryther/unifi-apclients-mqtt/issues/28)) ([2618a9e](https://github.com/Ryther/unifi-apclients-mqtt/commit/2618a9e092f34cff3b2a3ddfea44412a3b28bbbf))
* add structured operational logging and user guide ([01815c6](https://github.com/Ryther/unifi-apclients-mqtt/commit/01815c62cc1fc2e9c65ea79f3c29de397e2d2098))
* add UniFi AP client MQTT service ([e27db38](https://github.com/Ryther/unifi-apclients-mqtt/commit/e27db38bbf6b6027268981647392ca80a83128f1))
* **mqtt:** configure discovery and topic roots ([#23](https://github.com/Ryther/unifi-apclients-mqtt/issues/23)) ([0203051](https://github.com/Ryther/unifi-apclients-mqtt/commit/0203051aaf25097f630a7563e0da8a204d588671))
* persist client history by default in container ([#30](https://github.com/Ryther/unifi-apclients-mqtt/issues/30)) ([43ca151](https://github.com/Ryther/unifi-apclients-mqtt/commit/43ca151924296eea8037551ebf0fe6d34ab23de1))
* publish docs and Sonar analysis ([8aa9db6](https://github.com/Ryther/unifi-apclients-mqtt/commit/8aa9db6122658b2e06c41a548b04d931d2fe7e68))
* scan images and Rust dependencies for vulnerabilities ([#33](https://github.com/Ryther/unifi-apclients-mqtt/issues/33)) ([5ccf18e](https://github.com/Ryther/unifi-apclients-mqtt/commit/5ccf18eb567efecd26edb756ec68f8cac2ac9064))
* ship a static scratch runtime image ([#37](https://github.com/Ryther/unifi-apclients-mqtt/issues/37)) ([5346329](https://github.com/Ryther/unifi-apclients-mqtt/commit/5346329b80861a8ae9297e8b09f53611d0b047e7))
* support Docker secret files and expand user docs ([2fa4bb5](https://github.com/Ryther/unifi-apclients-mqtt/commit/2fa4bb5cbe1c0ddb064c134153017d6f14f1105b))


### Bug Fixes

* align SonarCloud project key ([a55b481](https://github.com/Ryther/unifi-apclients-mqtt/commit/a55b481bb56dc92e8e0c0f10127c9862be5e36f7))
* **mqtt:** keep polling through broker backpressure ([#43](https://github.com/Ryther/unifi-apclients-mqtt/issues/43)) ([48dadf3](https://github.com/Ryther/unifi-apclients-mqtt/commit/48dadf32a5a95cdff8b65e347b56668d3abd0346))
* pass artifact read permission to release validation ([#34](https://github.com/Ryther/unifi-apclients-mqtt/issues/34)) ([bba7821](https://github.com/Ryther/unifi-apclients-mqtt/commit/bba78217749bbe1810e010f14a21e5c0d49f1619))
* preserve baseline checkout for external PR analysis ([#31](https://github.com/Ryther/unifi-apclients-mqtt/issues/31)) ([ec09462](https://github.com/Ryther/unifi-apclients-mqtt/commit/ec094628e638cbfeedd859574b61e197126dfefd))
* **release:** find drafts without materialized tags ([3d6c18a](https://github.com/Ryther/unifi-apclients-mqtt/commit/3d6c18a15da0ab97380be487e487d7ea0a6395a2))
* **release:** grant draft visibility to verifier ([833f657](https://github.com/Ryther/unifi-apclients-mqtt/commit/833f6574a036d9295fee5eb3c58dcdc72ef9f252))
* **release:** validate draft target commit before publish ([f1562e6](https://github.com/Ryther/unifi-apclients-mqtt/commit/f1562e65908eb1fa59b4b86d5a99b8a9662cc05f))
* skip release automation until token is configured ([d709cec](https://github.com/Ryther/unifi-apclients-mqtt/commit/d709cec78973ef179db40cd31957e2eeaf2ac8aa))
* use supported CodeQL extraction mode ([a36dca4](https://github.com/Ryther/unifi-apclients-mqtt/commit/a36dca4c6ba5407a1400529fd944d17f91e9ba96))

## [1.0.1](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v1.0.0...unifi-apclients-mqtt-v1.0.1) (2026-10-07)


### Bug Fixes

* **mqtt:** keep polling through broker backpressure ([#43](https://github.com/Ryther/unifi-apclients-mqtt/issues/43)) ([48dadf3](https://github.com/Ryther/unifi-apclients-mqtt/commit/48dadf32a5a95cdff8b65e347b56668d3abd0346))

## [1.0.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.9.0...unifi-apclients-mqtt-v1.0.0) (2026-09-29)


### ⚠ BREAKING CHANGES

* The runtime image has no shell or OS certificate store, and is built for x86-64 only. Mounted private CA files are not automatically trusted.

### Features

* ship a static scratch runtime image ([#37](https://github.com/Ryther/unifi-apclients-mqtt/issues/37)) ([5346329](https://github.com/Ryther/unifi-apclients-mqtt/commit/5346329b80861a8ae9297e8b09f53611d0b047e7))

## [0.9.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.8.0...unifi-apclients-mqtt-v0.9.0) (2026-09-29)


### Features

* scan images and Rust dependencies for vulnerabilities ([#33](https://github.com/Ryther/unifi-apclients-mqtt/issues/33)) ([5ccf18e](https://github.com/Ryther/unifi-apclients-mqtt/commit/5ccf18eb567efecd26edb756ec68f8cac2ac9064))


### Bug Fixes

* pass artifact read permission to release validation ([#34](https://github.com/Ryther/unifi-apclients-mqtt/issues/34)) ([bba7821](https://github.com/Ryther/unifi-apclients-mqtt/commit/bba78217749bbe1810e010f14a21e5c0d49f1619))

## [0.8.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.7.0...unifi-apclients-mqtt-v0.8.0) (2026-09-29)


### Features

* persist client history by default in container ([#30](https://github.com/Ryther/unifi-apclients-mqtt/issues/30)) ([43ca151](https://github.com/Ryther/unifi-apclients-mqtt/commit/43ca151924296eea8037551ebf0fe6d34ab23de1))


### Bug Fixes

* preserve baseline checkout for external PR analysis ([#31](https://github.com/Ryther/unifi-apclients-mqtt/issues/31)) ([ec09462](https://github.com/Ryther/unifi-apclients-mqtt/commit/ec094628e638cbfeedd859574b61e197126dfefd))

## [0.7.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.6.0...unifi-apclients-mqtt-v0.7.0) (2026-09-29)


### Features

* add eligible clients with SQLite presence history ([#28](https://github.com/Ryther/unifi-apclients-mqtt/issues/28)) ([2618a9e](https://github.com/Ryther/unifi-apclients-mqtt/commit/2618a9e092f34cff3b2a3ddfea44412a3b28bbbf))

## [0.6.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.5.0...unifi-apclients-mqtt-v0.6.0) (2026-09-29)


### Features

* add structured operational logging and user guide ([01815c6](https://github.com/Ryther/unifi-apclients-mqtt/commit/01815c62cc1fc2e9c65ea79f3c29de397e2d2098))

## [0.5.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.4.1...unifi-apclients-mqtt-v0.5.0) (2026-09-29)


### Features

* **mqtt:** configure discovery and topic roots ([#23](https://github.com/Ryther/unifi-apclients-mqtt/issues/23)) ([0203051](https://github.com/Ryther/unifi-apclients-mqtt/commit/0203051aaf25097f630a7563e0da8a204d588671))

## [0.4.1](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.4.0...unifi-apclients-mqtt-v0.4.1) (2026-09-28)


### Bug Fixes

* align SonarCloud project key ([a55b481](https://github.com/Ryther/unifi-apclients-mqtt/commit/a55b481bb56dc92e8e0c0f10127c9862be5e36f7))

## [0.4.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.3.0...unifi-apclients-mqtt-v0.4.0) (2026-09-28)


### Features

* support Docker secret files and expand user docs ([2fa4bb5](https://github.com/Ryther/unifi-apclients-mqtt/commit/2fa4bb5cbe1c0ddb064c134153017d6f14f1105b))

## [0.3.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.2.0...unifi-apclients-mqtt-v0.3.0) (2026-09-28)


### Features

* add UniFi AP client MQTT service ([e27db38](https://github.com/Ryther/unifi-apclients-mqtt/commit/e27db38bbf6b6027268981647392ca80a83128f1))
* publish docs and Sonar analysis ([8aa9db6](https://github.com/Ryther/unifi-apclients-mqtt/commit/8aa9db6122658b2e06c41a548b04d931d2fe7e68))


### Bug Fixes

* **release:** find drafts without materialized tags ([3d6c18a](https://github.com/Ryther/unifi-apclients-mqtt/commit/3d6c18a15da0ab97380be487e487d7ea0a6395a2))
* **release:** grant draft visibility to verifier ([833f657](https://github.com/Ryther/unifi-apclients-mqtt/commit/833f6574a036d9295fee5eb3c58dcdc72ef9f252))
* **release:** validate draft target commit before publish ([f1562e6](https://github.com/Ryther/unifi-apclients-mqtt/commit/f1562e65908eb1fa59b4b86d5a99b8a9662cc05f))
* skip release automation until token is configured ([d709cec](https://github.com/Ryther/unifi-apclients-mqtt/commit/d709cec78973ef179db40cd31957e2eeaf2ac8aa))
* use supported CodeQL extraction mode ([a36dca4](https://github.com/Ryther/unifi-apclients-mqtt/commit/a36dca4c6ba5407a1400529fd944d17f91e9ba96))

## [0.2.0](https://github.com/Ryther/unifi-apclients-mqtt/compare/unifi-apclients-mqtt-v0.1.0...unifi-apclients-mqtt-v0.2.0) (2026-09-28)


### Features

* add UniFi AP client MQTT service ([e27db38](https://github.com/Ryther/unifi-apclients-mqtt/commit/e27db38bbf6b6027268981647392ca80a83128f1))
* publish docs and Sonar analysis ([8aa9db6](https://github.com/Ryther/unifi-apclients-mqtt/commit/8aa9db6122658b2e06c41a548b04d931d2fe7e68))


### Bug Fixes

* skip release automation until token is configured ([d709cec](https://github.com/Ryther/unifi-apclients-mqtt/commit/d709cec78973ef179db40cd31957e2eeaf2ac8aa))
* use supported CodeQL extraction mode ([a36dca4](https://github.com/Ryther/unifi-apclients-mqtt/commit/a36dca4c6ba5407a1400529fd944d17f91e9ba96))
