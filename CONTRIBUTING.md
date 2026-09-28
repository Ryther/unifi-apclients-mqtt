# Contributing

The service is designed to read UniFi statistics and publish them to MQTT. It
does not write UniFi or Home Assistant configuration. Keep changes within that
boundary unless the product design is explicitly expanded.

## Development environment

Open the repository in its Dev Container, or install the version in
[`rust-toolchain.toml`](rust-toolchain.toml). Copy `.env.example` to `.env` only
for a local run, then replace every value with settings for a disposable or
authorized test environment. `.env` is ignored by Git.

The UniFi API tests use a local HTTP fixture and never contact a controller.
The live MQTT contract test needs a disposable broker; set
`MQTT_TEST_BROKER=host:port` to enable it. The GitHub workflow starts Mosquitto
with [`tests/mosquitto.conf`](tests/mosquitto.conf).

## Checks

Run these commands from the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
```

The supported MSRV is Rust 1.90. CI also checks Rust 1.98.1, the version pinned
for local development and Docker builds. For changes to `.github/workflows`,
run the pinned `actionlint` container. For Rust changes, include the affected
contract tests and report the exact command and result in the pull request.
Never use real household client records as fixtures.

## Pull requests

Use a Conventional Commit title and commits (`feat:`, `fix:`, `docs:`, `ci:`,
`build:`, `test:`, `refactor:`, or `chore:`). Describe the behavior change,
verification performed, and any deployment or compatibility impact. Update the
README or architecture/release documentation when the public contract changes.

Do not include credentials, `.env` files, local logs, MQTT captures, household
hostnames/IPs, or real AP/client MAC addresses. Review the entire staged diff
and run the secret scanner before pushing.

Dependabot updates are proposals only. Review the source version, Cargo.lock or
action SHA changes, and all applicable CI checks before merging; no dependency
PR is merged automatically.
