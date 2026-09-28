# Releases

Release Please proposes version and changelog updates from Conventional Commits
merged to `main`. Review the release pull request before merging it. Stable
release tags use the `vMAJOR.MINOR.PATCH` form.

## GitHub configuration

Configure the repository to use `main` as its default branch and enable Actions.
Set the fine-grained repository secret `RELEASE_PLEASE_TOKEN` with repository
Contents, Issues, and Pull requests read/write permissions. The built-in
`GITHUB_TOKEN` does not trigger follow-up CI for bot-created pull requests.
Allow GitHub Actions to create pull requests in repository settings if the
organization requires that setting.

After CI has run once on `main`, require the Conventional Commits, Workflow
lint, Secrets, both Rust toolchain checks, and Docker image checks in the branch
rules. Also require both CodeQL checks when code scanning is available. Require
pull requests and current-base checks; disallow force pushes and branch
deletion. Select the exact check names shown by GitHub. CodeQL is skipped for a
private repository when its GitHub plan does not provide code scanning. Review
security and release settings before the first image publication.

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
