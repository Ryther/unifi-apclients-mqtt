"""Community scanning only passes a clean scanner and quality gate."""

import tempfile
import unittest
from pathlib import Path

from sonar_community import _task_id, candidate_scan_ok


class SonarCommunityTests(unittest.TestCase):
    def test_requires_scanner_success_and_quality_gate_ok(self):
        self.assertTrue(candidate_scan_ok(0, "OK"))
        self.assertFalse(candidate_scan_ok(1, "OK"))
        self.assertFalse(candidate_scan_ok(0, "ERROR"))

    def test_reads_compute_task_id_from_scanner_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            receipt = Path(directory) / "report-task.txt"
            receipt.write_text("projectKey=fixture\nceTaskId=task-123\n")
            self.assertEqual(_task_id(receipt), "task-123")

    def test_rejects_scanner_receipt_without_compute_task_id(self):
        with tempfile.TemporaryDirectory() as directory:
            receipt = Path(directory) / "report-task.txt"
            receipt.write_text("projectKey=fixture\n")
            with self.assertRaisesRegex(RuntimeError, "compute task"):
                _task_id(receipt)


if __name__ == "__main__":
    unittest.main()
