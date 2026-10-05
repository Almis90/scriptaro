#!/usr/bin/env python3
"""Run from a normal macOS Terminal after building and granting Accessibility.

No downloads, uploads, permission changes, or user documents. Each live iteration
uses the existing disposable native receiver fixture. Simulation and live results
are recorded separately; receiver passes are not real-application reliability data.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
REQUIRED_PERMISSIONS = ("Accessibility", "Post events")


def permission_status(output):
    statuses = {}
    for line in output.splitlines():
        name, separator, value = line.partition(": ")
        if separator and name in REQUIRED_PERMISSIONS:
            statuses[name] = value.split(" — ", 1)[0] == "granted"
    return {name: statuses.get(name, False) for name in REQUIRED_PERMISSIONS}


def run_command(argv, timeout):
    started = time.monotonic()
    try:
        result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True,
                                errors="replace", timeout=timeout)
        return {"status": "passed" if result.returncode == 0 else "failed",
                "returncode": result.returncode, "seconds": round(time.monotonic() - started, 3),
                "stdout": result.stdout, "stderr": result.stderr}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"status": "failed", "returncode": None,
                "seconds": round(time.monotonic() - started, 3),
                "stdout": "", "stderr": str(error)}


def verify(binary, bundle, runs, report_path, *, execute=run_command, host=None):
    report = {
        "schema_version": 1,
        "started_at": datetime.now(timezone.utc).isoformat(),
        "host": host or platform.system(),
        "status": "running",
        "requested_native_runs": runs,
        "completed_native_runs": 0,
        "checks": [],
        "real_application_trials": {name: "not_run" for name in
                                    ("document_editing", "browser_form", "coding_demo")},
    }

    def save():
        report_path.parent.mkdir(parents=True, exist_ok=True)
        # Atomic replacement leaves the last completed check recoverable after interruption.
        temporary = report_path.with_suffix(".tmp")
        temporary.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        temporary.replace(report_path)

    def finish(status, reason=None):
        report["status"] = status
        report["finished_at"] = datetime.now(timezone.utc).isoformat()
        if reason:
            report["reason"] = reason
        save()
        return report

    def check(name, argv, timeout=30):
        print(f"Checking {name}…", flush=True)
        result = execute([str(arg) for arg in argv], timeout)
        report["checks"].append({"name": name, **result})
        save()
        return result

    save()
    try:
        if report["host"] != "Darwin":
            return finish("blocked", "This verification requires a logged-in macOS desktop.")
        doctor = check("permissions", [binary, "doctor"])
        if doctor["status"] != "passed":
            return finish("blocked", "Could not inspect permissions; build the CLI first.")
        report["permissions"] = permission_status(doctor["stdout"])
        if not all(report["permissions"].values()):
            return finish("blocked", "Accessibility/event posting unavailable. Enable Accessibility for the launching terminal or Scriptaro, restart it, then rerun from a normal Terminal. No app or receiver was launched.")
        for name, command in [
            ("bundle_plist", ["plutil", "-lint", bundle / "Contents/Info.plist"]),
            ("bundle_signature", ["codesign", "--verify", "--deep", "--strict", bundle]),
            ("bundled_simulated_transport", [bundle / "Contents/MacOS/Scriptaro", "--smoke-test", ROOT / "examples/sections.yaml"]),
        ]:
            if check(name, command)["status"] != "passed":
                return finish("failed", f"{name} failed; remaining native iterations were not run.")
        for iteration in range(1, runs + 1):
            # The fixture bounds individual operations and owns receiver cleanup.
            # Do not kill its Python process on an outer timeout and bypass its finally block.
            result = check(f"native_receiver_{iteration:03}",
                           [sys.executable, ROOT / "tests/macos/smoke.py", binary], timeout=None)
            if result["status"] != "passed":
                return finish("failed", f"Native iteration {iteration} failed; stopped without retrying effects.")
            report["completed_native_runs"] = iteration
            save()
        return finish("passed", "Bundle simulation and disposable receiver checks passed. Real-application trials remain not run.")
    except KeyboardInterrupt:
        return finish("interrupted", "Verification interrupted; completed checks are preserved.")


def count(value):
    result = int(value)
    if not 1 <= result <= 100:
        raise argparse.ArgumentTypeError("runs must be between 1 and 100")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=count, default=20,
                        help="consecutive disposable receiver runs; stops on first failure (default: 20)")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/scriptaro")
    parser.add_argument("--bundle", type=Path, default=ROOT / "target/Scriptaro.app")
    parser.add_argument("--output", type=Path, help="new report directory (must not already exist)")
    args = parser.parse_args()
    if args.output:
        output = args.output.resolve()
        output.mkdir(parents=True, exist_ok=False)
    else:
        parent = ROOT / "target/verification"
        parent.mkdir(parents=True, exist_ok=True)
        output = Path(tempfile.mkdtemp(prefix="macos-", dir=parent))
    report_path = output / "report.json"
    report = verify(args.binary.resolve(), args.bundle.resolve(), args.runs, report_path)
    print(f"{report['status'].upper()}: {report['completed_native_runs']}/{args.runs} native receiver runs")
    print(report.get("reason", ""))
    print(f"Report: {report_path}")
    return {"passed": 0, "blocked": 2, "interrupted": 130}.get(report["status"], 1)


if __name__ == "__main__":
    sys.exit(main())
