# Releases

[Documentation home](index.md) · [← Architecture](architecture.md)

Release Please proposes version and changelog updates from Conventional Commits
merged to `main`. Review the release pull request before merging it. Stable
release tags use the `unifi-apclients-mqtt-vMAJOR.MINOR.PATCH` form.

For an explicitly authorized maintenance release, add a `Release-As: X.Y.Z`
footer to a Conventional Commit containing the reviewed change. Release Please
then proposes that version; review its generated diff and checks before merging.
Do not edit version files or create a release tag by hand. Empty commits can be
dropped during a rebase merge, so do not rely on an empty commit to carry this
request through a pull request.

## GitHub configuration

Use `main` as the default branch and enable Actions. In Pages settings, select
GitHub Actions as the build and deployment source. The `DOCS_PAGES_ENABLED`
Actions variable controls automatic publication from `main`; manual workflow
runs also publish when it is `true`.

Set the fine-grained repository secret `RELEASE_PLEASE_TOKEN` with repository
Contents, Issues, and Pull requests read/write permissions. The built-in
`GITHUB_TOKEN` does not trigger follow-up CI for bot-created pull requests.
Set `SONAR_TOKEN` to a project analysis token for the SonarQube Cloud project
`Ryther_unifi-apclients-mqtt` in the `ryther` organization. Sonar runs only on
`main` pushes and trusted same-repository pull requests; its Clippy and
coverage report job has
no Sonar secret. The required `Sonar Cloud` status fails when coverage or the
expected analysis report is missing or fails. Fork and Dependabot pull
requests are analyzed on a disposable SonarQube Community Build with a
job-scoped token instead of the SonarCloud secret. Allow GitHub Actions to
create pull requests in repository settings if the organization requires that
setting.

Configure and verify the live `main` branch rules to require these checks:
`Conventional Commits`, `Workflow lint`, `Secrets`, `Rust (1.90.0)`, `Rust (1.98.1)`, `Docker image`, `build`
(documentation), `CodeQL (rust)`, `CodeQL (actions)`, `Sonar Clippy report`,
and `Sonar Cloud`. Fork and Dependabot pull requests use the disposable
Community scan because they do not receive the project token. Configure branch
rules to require pull requests and current-base checks, enforce linear history and conversation
resolution, and disallow force pushes and branch deletion. Recheck live
security and release settings when workflow contracts change; this document
does not certify the current GitHub settings.

## Exact-candidate publication

The release workflow creates a draft release. It validates the exact commit
recorded as the draft's target, runs formatting, Clippy, tests against a
disposable Mosquitto broker, builds the Docker image, and scans that exact
candidate and `Cargo.lock` with Trivy before publishing to GHCR. Fixable high
and critical findings fail validation. High and critical findings, including
those without fixes, are uploaded to GitHub Code Scanning from trusted `main`
pushes. A weekly workflow rescans the published `latest` image and current
lockfile. Only after the image push succeeds does the workflow publish the
GitHub release and create its tag.

The `scratch` runtime has no distribution package database, so a clean image
scan alone is not proof that the compiled binary has no vulnerable components.
Keep the separate `Cargo.lock` scan and review bundled native dependencies.

If validation or image publication fails, the release remains a draft. Use the
workflow's manual `release_tag` input with that draft's exact tag to retry the
same candidate after correcting a transient network, registry or credential
problem. A code or lockfile fix in a later commit does not repair
the old candidate: propose a new candidate through Release Please instead.
Never move a published tag or publish an image from a different commit.

The versioned image tag is the full release tag, for example
`ghcr.io/ryther/unifi-apclients-mqtt:unifi-apclients-mqtt-v1.0.1`, not `:1.0.1`.
The same publication also advances `latest`.

GHCR package visibility is configured separately from repository visibility.
If the server must pull a private package, provide a read-only package token to
the deployment host. If the package is made public, anyone can pull the image.

## Dependency updates

Dependabot proposes weekly Cargo and GitHub Actions updates. Review each update
and its lock/SHA diff, run the normal checks, and merge deliberately. Do not
enable automatic merging for dependencies or action pins.
