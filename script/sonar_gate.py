"""Fail closed when required coverage or the trusted Sonar scan is missing."""

from __future__ import annotations

import os


def evaluate(
    event_name: str,
    trusted_pr: bool,
    coverage: str,
    cloud: str,
    community: str,
    report_present: bool,
) -> bool:
    """Check the expected scanner and report for each supported event."""
    if coverage != "success" or not report_present:
        return False
    if event_name == "push":
        return cloud == "success" and community == "skipped"
    if event_name == "pull_request":
        if trusted_pr:
            return cloud == "success" and community == "skipped"
        return cloud == "skipped" and community == "success"
    return False


def _flag(name: str) -> bool:
    value = os.environ[name]
    if value not in {"true", "false"}:
        raise SystemExit(f"Invalid {name} value")
    return value == "true"


def main() -> None:
    """Evaluate GitHub job results without using any publication secret."""
    if not evaluate(
        os.environ["SONAR_EVENT_NAME"],
        _flag("SONAR_TRUSTED_PR"),
        os.environ["SONAR_COVERAGE_RESULT"],
        os.environ["SONAR_CLOUD_RESULT"],
        os.environ["SONAR_COMMUNITY_RESULT"],
        _flag("SONAR_REPORT_PRESENT"),
    ):
        raise SystemExit("Required coverage or expected Sonar analysis report is missing")


if __name__ == "__main__":
    main()
