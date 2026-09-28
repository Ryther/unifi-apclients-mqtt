# Releases

Release Please proposes version and changelog updates from Conventional Commits
merged to `main`. Review the release pull request before merging it. Stable
release tags use the `vMAJOR.MINOR.PATCH` form.

## GitHub configuration

Use `main` as the default branch and enable Actions. In Pages settings, select
GitHub Actions as the build and deployment source. The `DOCS_PAGES_ENABLED`
Actions variable controls automatic publication from `main`; manual workflow
runs also publish when it is `true`.

Set the fine-grained repository secret `RELEASE_PLEASE_TOKEN` with repository
Contents, Issues, and Pull requests read/write permissions. The built-in
`GITHUB_TOKEN` does not trigger follow-up CI for bot-created pull requests.
Set `SONAR_TOKEN` to a project analysis token for the SonarQube Cloud project
`Ryther_unifi-apclients-mqtt`. Sonar runs only on `main` pushes and
same-repository pull requests; its Clippy report job has no Sonar secret. Allow
GitHub Actions to create pull requests in repository settings if the
organization requires that setting.

After each workflow has completed on `main`, require the Conventional Commits,
Workflow lint, Secrets, both Rust toolchain checks, Docker image, documentation,
and CodeQL checks in branch protection. Add Sonar Cloud after its first
successful baseline scan. Require pull requests and current-base checks, enable
linear history and conversation resolution, and disallow force pushes and
branch deletion. Select the exact check names shown by GitHub. Review security
and release settings before the first image publication.

## Exact-candidate publication

The release workflow creates a draft release. It checks out that tag's exact
commit, runs formatting, Clippy, tests against a disposable Mosquitto broker,
and builds the Docker image before publishing to GHCR. Only after the image
push succeeds does the workflow publish the GitHub release.

If validation or image publication fails, the release remains a draft. Use the
workflow's manual `release_tag` input to validate and resume that draft after
fixing the cause. Never move a published tag or publish an image from a
different commit.

GHCR package visibility is configured separately from repository visibility.
If the server must pull a private package, provide a read-only package token to
the deployment host. If the package is made public, anyone can pull the image.

## Dependency updates

Dependabot proposes weekly Cargo and GitHub Actions updates. Review each update
and its lock/SHA diff, run the normal checks, and merge deliberately. Do not
enable automatic merging for dependencies or action pins.
