# Releases

[Documentation home](index.md) · [← Architecture](architecture.md)

Release Please proposes version and changelog updates from Conventional Commits
merged to `main`. Review the release pull request before merging it. Stable
release tags use the `unifi-apclients-mqtt-vMAJOR.MINOR.PATCH` form.

## GitHub configuration

Use `main` as the default branch and enable Actions. In Pages settings, select
GitHub Actions as the build and deployment source. The `DOCS_PAGES_ENABLED`
Actions variable controls automatic publication from `main`; manual workflow
runs also publish when it is `true`.

Set the fine-grained repository secret `RELEASE_PLEASE_TOKEN` with repository
Contents, Issues, and Pull requests read/write permissions. The built-in
`GITHUB_TOKEN` does not trigger follow-up CI for bot-created pull requests.
Set `SONAR_TOKEN` to a project analysis token for the SonarQube Cloud project
`unifi-apclients-mqtt`. Sonar runs only on `main` pushes and
same-repository pull requests; its Clippy report job has no Sonar secret. Allow
GitHub Actions to create pull requests in repository settings if the
organization requires that setting.

The `main` branch rules require these checks: `Conventional Commits`, `Workflow
lint`, `Secrets`, `Rust (1.90.0)`, `Rust (1.98.1)`, `Docker image`, `build`
(documentation), `CodeQL (rust)`, `CodeQL (actions)`, `Sonar Clippy report`,
and `Sonar Cloud`. The Cloud scan is skipped for fork and Dependabot pull
requests because they do not receive the project token. Branch rules require
pull requests and current-base checks, enforce linear history and conversation
resolution, and disallow force pushes and branch deletion. Review security and
release settings before the first image publication.

## Exact-candidate publication

The release workflow creates a draft release. It validates the exact commit
recorded as the draft's target, runs formatting, Clippy, tests against a
disposable Mosquitto broker, and builds the Docker image before publishing to
GHCR. Only after the image push succeeds does the workflow publish the GitHub
release and create its tag.

If validation or image publication fails, the release remains a draft. Use the
workflow's manual `release_tag` input (for example,
`unifi-apclients-mqtt-v0.2.0`) to validate and resume that draft after fixing
the cause. Never move a published tag or publish an image from a different
commit.

GHCR package visibility is configured separately from repository visibility.
If the server must pull a private package, provide a read-only package token to
the deployment host. If the package is made public, anyone can pull the image.

## Dependency updates

Dependabot proposes weekly Cargo and GitHub Actions updates. Review each update
and its lock/SHA diff, run the normal checks, and merge deliberately. Do not
enable automatic merging for dependencies or action pins.
