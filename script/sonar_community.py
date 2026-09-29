"""Analyze an untrusted Rust pull request on a disposable Sonar Community server."""

from __future__ import annotations

import argparse
import base64
import json
import os
import secrets
import subprocess
import time
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

SERVER = os.environ.get("UNIFI_SONAR_URL", "http://127.0.0.1:9000")
SCANNER = "sonarsource/sonar-scanner-cli@sha256:a3f4215076706c95a17a68c19322ee916e40a3acd081a8c1a1e839e0194afa57"
PROJECT_KEY = "unifi-apclients-mqtt-community-pr"


def request(
    path: str,
    params: dict[str, Any] | None = None,
    *,
    post: bool = False,
    token: str = "",
) -> dict[str, Any]:
    data = urllib.parse.urlencode(params or {}).encode() if post else None
    query = "" if post or not params else "?" + urllib.parse.urlencode(params)
    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    else:
        credentials = base64.b64encode(b"admin:admin").decode()
        headers["Authorization"] = f"Basic {credentials}"
    request = urllib.request.Request(SERVER + path + query, data=data, headers=headers)
    with urllib.request.urlopen(request, timeout=30) as response:
        if response.status == 204:
            return {}
        return json.load(response)


def candidate_scan_ok(exit_code: int, gate_status: str) -> bool:
    """Accept only a successful scanner run and a passing community gate."""
    return exit_code == 0 and gate_status == "OK"


def wait_for_server() -> None:
    for _ in range(120):
        try:
            if request("/api/system/status").get("status") == "UP":
                return
        except (OSError, KeyError, ValueError):
            pass
        time.sleep(2)
    raise RuntimeError("Disposable Sonar Community server did not become healthy")


def _task_id(receipt: Path) -> str:
    fields = {}
    for line in receipt.read_text().splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            fields[key] = value
    task_id = fields.get("ceTaskId")
    if not task_id:
        raise RuntimeError("Sonar scanner receipt did not include a compute task")
    return task_id


def analyze(source: Path, output: Path) -> None:
    source = source.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if not (source / "clippy-report.json").is_file():
        raise RuntimeError("Clippy report is missing from the proposed revision")
    if not (source / "lcov.info").is_file():
        raise RuntimeError("LCOV coverage report is missing from the proposed revision")

    wait_for_server()
    request(
        "/api/projects/create",
        {"project": PROJECT_KEY, "name": "UniFi AP Clients MQTT external PR"},
        post=True,
    )
    token_name = "unifi-ci-" + secrets.token_hex(6)
    token = request(
        "/api/user_tokens/generate", {"name": token_name}, post=True
    )["token"]
    if os.environ.get("GITHUB_ACTIONS") == "true":
        print(f"::add-mask::{token}", flush=True)

    report = output / "report.md"
    task = output / "sonar-task"
    task.mkdir(parents=True, exist_ok=True)
    task.chmod(0o777)
    receipt = task / "report-task.txt"
    receipt.unlink(missing_ok=True)
    scanner_log = output / "scanner.log"
    command = [
        "docker",
        "run",
        "--rm",
        "--network",
        "host",
        "-w",
        str(source),
        "-e",
        "SONAR_TOKEN",
        "-e",
        f"SONAR_HOST_URL={SERVER}",
        "-e",
        "SONAR_USER_HOME=/tmp/sonar-user",
        "-v",
        f"{source}:{source}:ro",
        "-v",
        f"{task}:/sonar-task",
        SCANNER,
        f"-Dsonar.projectKey={PROJECT_KEY}",
        "-Dsonar.projectName=UniFi AP Clients MQTT external PR",
        "-Dsonar.sources=src",
        "-Dsonar.tests=tests",
        "-Dsonar.test.inclusions=tests/**",
        "-Dsonar.exclusions=target/**,site/**",
        "-Dsonar.rust.clippy.enabled=false",
        "-Dsonar.rust.clippyReport.reportPaths=clippy-report.json",
        "-Dsonar.rust.lcov.reportPaths=lcov.info",
        "-Dsonar.working.directory=/sonar-task",
        "-Dsonar.scm.disabled=false",
        f"-Dsonar.host.url={SERVER}",
        "-Dsonar.qualitygate.wait=true",
    ]
    details = "Sonar scan did not submit an analysis."
    try:
        with scanner_log.open("w") as log:
            result = subprocess.run(
                command,
                env={**os.environ, "SONAR_TOKEN": token},
                stdout=log,
                stderr=subprocess.STDOUT,
                check=False,
            )
        if not receipt.is_file():
            raise RuntimeError(f"{details} Scanner exit code: {result.returncode}")

        status = request(
            "/api/qualitygates/project_status",
            {"projectKey": PROJECT_KEY},
            token=token,
        )["projectStatus"]["status"]
        measures = request(
            "/api/measures/component",
            {
                "component": PROJECT_KEY,
                "metricKeys": "coverage,bugs,vulnerabilities,code_smells",
            },
            token=token,
        ).get("component", {}).get("measures", [])
        values = {item["metric"]: item.get("value", "n/a") for item in measures}
        report.write_text(
            "\n".join(
                [
                    "# Sonar Community analysis",
                    "",
                    f"Quality gate: **{status}**",
                    f"Line coverage: **{values.get('coverage', 'n/a')}%**",
                    f"Bugs: **{values.get('bugs', 'n/a')}**",
                    f"Vulnerabilities: **{values.get('vulnerabilities', 'n/a')}**",
                    f"Code smells: **{values.get('code_smells', 'n/a')}**",
                    f"Scanner exit code: **{result.returncode}**",
                    "",
                    "The complete scanner log is attached to this workflow run.",
                    "",
                ]
            )
        )
        if not candidate_scan_ok(result.returncode, status):
            raise RuntimeError(
                f"Sonar Community analysis failed (scanner {result.returncode}, gate {status})"
            )
    finally:
        request("/api/user_tokens/revoke", {"name": token_name}, post=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    try:
        analyze(args.source, args.output)
    except Exception as error:
        (args.output / "report.md").write_text(
            f"# Sonar Community analysis\n\nAnalysis failed: `{type(error).__name__}`. "
            "See `scanner.log` and the workflow step output for details.\n"
        )
        raise


if __name__ == "__main__":
    main()
