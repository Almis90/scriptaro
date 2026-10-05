#!/usr/bin/env python3
"""Opt-in live native test. Opens only a disposable receiver app, then closes it.

Usage: python3 tests/macos/smoke.py target/debug/scriptaro
Requires a logged-in macOS desktop, Xcode tools, and Accessibility permission.
Not part of cargo test or unattended CI.
"""
import os
import pathlib
import plistlib
import signal
import subprocess
import sys
import tempfile
import time
import uuid


def wait_for(path, timeout=10):
    deadline = time.monotonic() + timeout
    while not path.exists():
        if time.monotonic() >= deadline:
            raise RuntimeError(f"timed out waiting for {path.name}")
        time.sleep(0.05)


def main():
    if sys.platform != "darwin":
        raise RuntimeError("this test requires macOS")
    binary = pathlib.Path(sys.argv[1]).resolve(strict=True)
    fixture = pathlib.Path(__file__).with_name("receiver.m")
    with tempfile.TemporaryDirectory(prefix="scriptaro-native-smoke-") as temporary:
        root = pathlib.Path(temporary).resolve()
        app = root / "Scriptaro Receiver.app"
        executable = app / "Contents/MacOS/receiver"
        executable.parent.mkdir(parents=True)
        identifier = "dev.scriptaro.smoke." + uuid.uuid4().hex
        with (app / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump({"CFBundleIdentifier": identifier, "CFBundleExecutable": "receiver",
                          "CFBundleName": "Scriptaro Receiver", "CFBundlePackageType": "APPL",
                          "CFBundleVersion": "1", "NSHighResolutionCapable": True,
                          "CFBundleDocumentTypes": [{"CFBundleTypeName": "Text",
                              "CFBundleTypeRole": "Editor", "LSItemContentTypes": ["public.plain-text"]}]}, stream)
        subprocess.run(["xcrun", "clang", "-fobjc-arc", "-framework", "Cocoa", str(fixture), "-o", str(executable)], check=True)
        (root / "seed.txt").write_text("replace this seed", encoding="utf-8")
        expected = "Hello, café Καλημέρα 🦀\nSecond line\tEND\nDone!"
        # Paths and identifiers are generated locally and have no shell interpolation.
        (root / "smoke.yaml").write_text(f"""version: 1
defaults:
  character_delay_ms: 35
  timeout_ms: 5000
steps:
  - action: activate_app
    app: {{by: identifier, value: {identifier}}}
  - action: open_file
    path: seed.txt
    app: {{by: identifier, value: {identifier}}}
  - action: wait
    duration_ms: 300
  - action: key_press
    key: a
    modifiers: [primary]
  - action: type_text
    text: "Hello, café Καλημέρα 🦀\\nSecond line\\tEND"
  - action: key_press
    key: enter
  - action: type_text
    text: "Done!"
  - action: wait
    duration_ms: 300
""", encoding="utf-8")
        pid = None
        try:
            subprocess.run(["open", "-a", str(app)], check=True)
            wait_for(root / "pid")
            pid = int((root / "pid").read_text())
            subprocess.run([str(binary), "run", str(root / "smoke.yaml"), "--start-delay-ms", "300"], check=True, timeout=30)
            if not (root / "opened").exists():
                raise AssertionError("native file-open callback did not reach the receiver")
            observed = (root / "observed.txt").read_text(encoding="utf-8")
            if observed != expected:
                raise AssertionError(f"text mismatch\nexpected: {expected!r}\nobserved: {observed!r}")
            print("Native smoke passed: activation, file opening, Command+A, Unicode, Enter and Tab.")
        finally:
            # Only terminate the receiver launched by this test, never a user application.
            if pid is not None:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass


if __name__ == "__main__":
    main()
