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


def run_command(*args, **kwargs):
    try:
        return subprocess.run(*args, **kwargs)
    except subprocess.CalledProcessError as error:
        # Discovery failures must retain the native diagnostic in saved reports.
        if error.stdout:
            print(error.stdout, file=sys.stderr)
        if error.stderr:
            print(error.stderr, file=sys.stderr)
        raise


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
    doctor = run_command([str(binary), "doctor"], check=True, capture_output=True, text=True, timeout=15)
    if "Accessibility: granted" not in doctor.stdout or "Post events: granted" not in doctor.stdout:
        raise RuntimeError("native smoke requires existing Accessibility and event-posting permission; no receiver was opened")
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
        run_command(["xcrun", "clang", "-fobjc-arc", "-framework", "Cocoa", str(fixture), "-o", str(executable)], check=True, timeout=60)
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
  - action: wait_until
    condition:
      kind: window_exists
      window:
        app: {{by: identifier, value: {identifier}}}
        title: Seed ready
  - action: activate_window
    window:
      app: {{by: identifier, value: {identifier}}}
      title: Seed ready
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
            run_command(["open", "-a", str(app)], check=True, timeout=15)
            wait_for(root / "pid")
            pid = int((root / "pid").read_text())
            run_command([str(binary), "run", str(root / "smoke.yaml"), "--start-delay-ms", "300"], check=True, timeout=30)
            if not (root / "opened").exists():
                raise AssertionError("native file-open callback did not reach the receiver")
            if not (root / "busy-tested").exists():
                raise AssertionError("busy receiver regression was not exercised")
            observed = (root / "observed.txt").read_text(encoding="utf-8")
            if observed != expected:
                raise AssertionError(f"text mismatch\nexpected: {expected!r}\nobserved: {observed!r}")
            windows = run_command([str(binary), "windows", "--pid", str(pid)], check=True,
                                     capture_output=True, text=True, timeout=15)
            if '"Seed ready"' not in windows.stdout or '"Other document"' not in windows.stdout:
                raise AssertionError(f"window discovery missed the test windows: {windows.stdout}")

            def play(name, steps, success=True):
                path = root / f"{name}.yaml"
                path.write_text("version: 1\ndefaults: {timeout_ms: 5000, character_delay_ms: 30}\nsteps:\n" + steps,
                                encoding="utf-8")
                result = run_command([str(binary), "run", str(path), "--start-delay-ms", "0"],
                                        capture_output=True, text=True, timeout=30)
                if (result.returncode == 0) != success:
                    raise AssertionError(f"{name} unexpected result: {result.stdout}\n{result.stderr}")
                return result

            app_selector = f"{{by: pid, value: {pid}}}"
            controls = run_command([str(binary), "controls", "--pid", str(pid), "--window", "Seed ready"],
                                      check=True, capture_output=True, text=True, timeout=15)
            for identifier in ["notes-editor", "secondary-field", "button-0"]:
                if identifier not in controls.stdout:
                    raise AssertionError(f"control discovery missed {identifier}")
            # Unlabeled NSTextView exposes AXDescription as unreadable. Listing
            # and exact identifier targeting must work, but label selection must fail.
            editor_row = next(line for line in controls.stdout.splitlines() if 'notes-editor' in line)
            if 'label_available=false' not in editor_row:
                raise AssertionError("unreadable label was not surfaced in discovery")
            result = play("unknown-label", f"""  - action: focus_control
    control:
      window: {{app: {app_selector}, title: Seed ready}}
      role: text_area
      label: Unavailable label
""", success=False)
            if "does not expose a readable label" not in result.stderr:
                raise AssertionError(f"unknown label was silently excluded: {result.stderr}")
            play("control-focus", f"""  - action: activate_window
    window: {{app: {app_selector}, title: Seed ready}}
  - action: focus_control
    control:
      window: {{app: {app_selector}, title: Seed ready}}
      role: text_area
      identifier: notes-editor
  - action: type_text
    text: " focused"
  - action: invoke_control
    control:
      window: {{app: {app_selector}, title: Seed ready}}
      role: button
      identifier: button-0
  - action: wait
    duration_ms: 200
""")
            expected += " focused"
            wait_for(root / "invoked")
            if (root / "observed.txt").read_text() != expected:
                raise AssertionError("control focus did not type into the editor")
            result = play("control-focus-loss", f"""  - action: activate_window
    window: {{app: {app_selector}, title: Seed ready}}
  - action: focus_control
    control:
      window: {{app: {app_selector}, title: Seed ready}}
      role: text_area
      identifier: notes-editor
  - action: key_press
    key: g
    modifiers: [primary]
  - action: wait_until
    condition:
      kind: control_focused
      control:
        window: {{app: {app_selector}, title: Seed ready}}
        role: text_field
        identifier: secondary-field
  - action: type_text
    text: MUST NOT TYPE
""", success=False)
            if "selected control is no longer focused" not in result.stderr or (root / "secondary-observed.txt").exists():
                raise AssertionError(f"same-window control guard failed: {result.stderr}")
            result = play("ambiguous-control", f"""  - action: invoke_control
    control:
      window: {{app: {app_selector}, title: Seed ready}}
      role: button
      label: Duplicate label
""", success=False)
            if "multiple controls match" not in result.stderr:
                raise AssertionError(f"ambiguous control was not rejected: {result.stderr}")
            # Restore the editor explicitly before the existing window identity test.
            play("restore-editor", f"""  - action: focus_control
    control:
      window: {{app: {app_selector}, title: Seed ready}}
      role: text_area
      identifier: notes-editor
""")
            play("rename", f"""  - action: activate_window
    window: {{app: {app_selector}, title: Seed ready}}
  - action: key_press
    key: r
    modifiers: [primary]
  - action: wait_until
    condition:
      kind: window_active
      window: {{app: {app_selector}, title: Renamed notes}}
  - action: type_text
    text: " retained"
  - action: wait
    duration_ms: 200
""")
            if (root / "observed.txt").read_text() != expected + " retained":
                raise AssertionError("selected window identity did not survive a title change")
            result = play("focus-loss", f"""  - action: activate_window
    window: {{app: {app_selector}, title: Renamed notes}}
  - action: key_press
    key: l
    modifiers: [primary]
  - action: wait_until
    condition:
      kind: window_active
      window: {{app: {app_selector}, title: Other document}}
  - action: type_text
    text: "MUST NOT TYPE"
""", success=False)
            if "focus left the selected window" not in result.stderr or (root / "other-observed.txt").exists():
                raise AssertionError(f"same-app window focus guard failed: {result.stderr}")
            result = play("readonly-control", f"""  - action: focus_control
    control:
      window: {{app: {app_selector}, title: Other document}}
      role: text_area
      identifier: readonly-editor
    timeout_ms: 150
  - action: type_text
    text: MUST NOT TYPE
""", success=False)
            if "waiting for control_enabled" not in result.stderr:
                raise AssertionError(f"read-only text area was treated as enabled: {result.stderr}")
            play("duplicate-title", f"""  - action: activate_window
    window: {{app: {app_selector}, title: Renamed notes}}
  - action: key_press
    key: d
    modifiers: [primary]
  - action: wait
    duration_ms: 200
""")
            wait_for(root / "duplicate-ready")
            result = play("ambiguous", f"""  - action: activate_window
    window: {{app: {app_selector}, title: Renamed notes}}
  - action: type_text
    text: "MUST NOT TYPE"
""", success=False)
            if "multiple windows match" not in result.stderr:
                raise AssertionError(f"ambiguous title was not rejected: {result.stderr}")
            if (root / "observed.txt").read_text() != expected + " retained":
                raise AssertionError("failed targeting emitted unexpected text")
            print("Native smoke passed: busy target, unknown labels, read-only text areas, control discovery/focus/invocation/ambiguity, same-window focus loss, file readiness, window selection/discovery, title changes, same-app focus loss, ambiguity, and Unicode input.")
        finally:
            # Only terminate the receiver launched by this test, never a user application.
            if pid is not None:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass


if __name__ == "__main__":
    main()
