"""The required Sonar status must represent the expected PR analysis."""

import unittest
from unittest.mock import patch

from sonar_gate import evaluate, main


class SonarGateTests(unittest.TestCase):
    def test_requires_completed_coverage_and_trusted_cloud_analysis(self):
        self.assertTrue(evaluate("push", False, "success", "success", "skipped", True))
        self.assertTrue(
            evaluate("pull_request", True, "success", "success", "skipped", True)
        )
        self.assertFalse(
            evaluate("pull_request", True, "success", "failure", "skipped", True)
        )
        self.assertFalse(
            evaluate("pull_request", True, "success", "skipped", "skipped", True)
        )

    def test_untrusted_pr_requires_the_community_scan_and_report(self):
        self.assertTrue(
            evaluate("pull_request", False, "success", "skipped", "success", True)
        )
        self.assertFalse(
            evaluate("pull_request", False, "failure", "skipped", "success", True)
        )
        self.assertFalse(
            evaluate("pull_request", False, "success", "skipped", "failure", True)
        )
        self.assertFalse(
            evaluate("pull_request", False, "success", "success", "success", True)
        )
        self.assertFalse(
            evaluate("pull_request", False, "success", "skipped", "success", False)
        )

    def test_rejects_missing_coverage_and_unexpected_events(self):
        self.assertFalse(evaluate("push", False, "failure", "success", "skipped", True))
        self.assertFalse(
            evaluate("workflow_dispatch", False, "success", "skipped", "skipped", True)
        )

    def test_command_rejects_an_invalid_trust_flag(self):
        environment = {
            "SONAR_EVENT_NAME": "pull_request",
            "SONAR_TRUSTED_PR": "maybe",
            "SONAR_COVERAGE_RESULT": "success",
            "SONAR_CLOUD_RESULT": "success",
            "SONAR_COMMUNITY_RESULT": "skipped",
            "SONAR_REPORT_PRESENT": "true",
        }
        with patch.dict("os.environ", environment, clear=True):
            with self.assertRaisesRegex(SystemExit, "SONAR_TRUSTED_PR"):
                main()

    def test_command_accepts_only_a_completed_trusted_scan(self):
        environment = {
            "SONAR_EVENT_NAME": "pull_request",
            "SONAR_TRUSTED_PR": "true",
            "SONAR_COVERAGE_RESULT": "success",
            "SONAR_CLOUD_RESULT": "success",
            "SONAR_COMMUNITY_RESULT": "skipped",
            "SONAR_REPORT_PRESENT": "true",
        }
        with patch.dict("os.environ", environment, clear=True):
            main()
        environment["SONAR_CLOUD_RESULT"] = "skipped"
        with patch.dict("os.environ", environment, clear=True):
            with self.assertRaisesRegex(SystemExit, "Required coverage"):
                main()

    def test_command_requires_community_report_for_fork_pr(self):
        environment = {
            "SONAR_EVENT_NAME": "pull_request",
            "SONAR_TRUSTED_PR": "false",
            "SONAR_COVERAGE_RESULT": "success",
            "SONAR_CLOUD_RESULT": "skipped",
            "SONAR_COMMUNITY_RESULT": "success",
            "SONAR_REPORT_PRESENT": "true",
        }
        with patch.dict("os.environ", environment, clear=True):
            main()
        environment["SONAR_REPORT_PRESENT"] = "false"
        with patch.dict("os.environ", environment, clear=True):
            with self.assertRaisesRegex(SystemExit, "Required coverage"):
                main()


if __name__ == "__main__":
    unittest.main()
