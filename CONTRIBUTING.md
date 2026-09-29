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

The Documentation workflow builds `docs/` with MkDocs Material in strict mode
and publishes it to GitHub Pages on `main` when `DOCS_PAGES_ENABLED=true`. The
Sonar workflow creates Clippy JSON and LCOV coverage reports without
credentials. Pushes and trusted same-repository pull requests use SonarQube
Cloud; fork and Dependabot pull requests use a disposable SonarQube Community
Build with a job-scoped token. A final required status fails if coverage or the
expected analysis report is missing or unsuccessful. Coverage runs the
contract suite against a disposable Mosquitto broker and enforces at least 90%
total line coverage.

The Docker image check scans the exact candidate image and `Cargo.lock` with
Trivy. Fixable high and critical findings fail the check; all high and
critical findings are retained in SARIF reports. Trusted `main` pushes upload
those reports to GitHub Code Scanning. A weekly workflow also scans the
published `latest` image and the current lockfile.

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
