"""Portable runner tests: never call native APIs or launch a process."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

MODULE = Path(__file__).resolve().parents[1] / "scripts/verify_macos.py"
spec = importlib.util.spec_from_file_location("verify_macos", MODULE)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

GRANTED = "Accessibility: granted — input\nPost events: granted — Quartz\n"


def result(status="passed", stdout=""):
    return {"status": status, "returncode": 0 if status == "passed" else 1,
            "seconds": 0, "stdout": stdout, "stderr": ""}


class VerificationTests(unittest.TestCase):
    def verify(self, execute, host="Darwin"):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "report.json"
            report = runner.verify(Path("cli"), Path("Scriptaro.app"), 3, path,
                                   execute=execute, host=host)
            self.assertEqual(json.loads(path.read_text()), report)
            return report

    def test_missing_unknown_or_denied_permissions_never_launch_apps(self):
        for doctor in ["", "Accessibility: granted\n", GRANTED.replace("granted", "not granted")]:
            calls = []
            def execute(argv, timeout):
                calls.append(argv)
                return result(stdout=doctor)
            report = self.verify(execute)
            self.assertEqual(report["status"], "blocked")
            self.assertEqual(report["completed_native_runs"], 0)
            self.assertEqual(calls, [["cli", "doctor"]])

    def test_non_macos_performs_no_commands(self):
        def execute(*_):
            self.fail("must not launch anything")
        self.assertEqual(self.verify(execute, "Linux")["status"], "blocked")

    def test_bundle_failure_prevents_live_iterations(self):
        calls = []
        def execute(argv, timeout):
            calls.append(argv)
            return result(stdout=GRANTED) if len(calls) < 4 else result("failed")
        report = self.verify(execute)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(len(calls), 4)
        self.assertEqual(report["completed_native_runs"], 0)

    def test_live_failure_stops_without_retry_and_preserves_completed_runs(self):
        calls = []
        def execute(argv, timeout):
            calls.append(argv)
            return result("failed") if len(calls) == 6 else result(stdout=GRANTED)
        report = self.verify(execute)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["completed_native_runs"], 1)
        self.assertEqual(len(calls), 6)

    def test_success_is_fixture_evidence_not_real_app_evidence(self):
        report = self.verify(lambda *_: result(stdout=GRANTED))
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["completed_native_runs"], 3)
        self.assertEqual(len(report["checks"]), 7)
        self.assertTrue(all(value == "not_run" for value in report["real_application_trials"].values()))

    def test_interrupt_keeps_report_and_never_counts_unfinished_iteration(self):
        calls = []
        def execute(argv, timeout):
            calls.append(argv)
            if len(calls) == 5:
                raise KeyboardInterrupt()
            return result(stdout=GRANTED)
        report = self.verify(execute)
        self.assertEqual(report["status"], "interrupted")
        self.assertEqual(report["completed_native_runs"], 0)

    def test_permission_parser_requires_exact_status(self):
        self.assertFalse(all(runner.permission_status("Accessibility: granted later\nPost events: granted").values()))
        self.assertTrue(all(runner.permission_status(GRANTED).values()))
        for value in ["0", "101", "-1"]:
            with self.assertRaises(runner.argparse.ArgumentTypeError):
                runner.count(value)


if __name__ == "__main__":
    unittest.main()
