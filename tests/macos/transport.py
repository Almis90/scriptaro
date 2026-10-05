#!/usr/bin/env python3
"""Live GUI transport clicks against a new scratch TextEdit document. macOS only.

Actual Quartz mouse clicks exercise Pause/Resume/Stop and Reset + retake. No
signals or direct button callbacks drive transport. Leave the desktop untouched.
"""
import argparse
from datetime import datetime, timezone
import json
import hashlib
from pathlib import Path
import subprocess
import sys
import time
import uuid
from app_trials import ROOT, Trials, action, key, wait_for


class Transport(Trials):
    def exercise(self, bundle, stop_while_paused=False):
        self.report.pop('trials')
        executable = bundle / 'Contents/MacOS/Scriptaro'
        self.report['desktop_bundle'] = dict(path=str(bundle), sha256=hashlib.sha256(executable.read_bytes()).hexdigest())
        self.report['gui_transport'] = dict(status='running', completed=0, cycles=[],
                                            stop_while_paused=stop_while_paused)
        doctor = self.cli('permissions', 'doctor').stdout
        if not all(name + ': granted' in doctor for name in ('Accessibility', 'Post events')):
            raise RuntimeError('Accessibility and event posting permissions required')
        self.driver = self.output / 'driver'
        self.command('compile_driver', ['xcrun', 'clang', '-fobjc-arc', '-framework', 'Cocoa',
                                      '-framework', 'ApplicationServices', ROOT / 'tests/macos/transport_driver.m',
                                      '-o', self.driver])
        document = self.scratch / ('Scriptaro Transport ' + uuid.uuid4().hex[:10] + '.txt')
        document.write_text('')
        self.editor = self.launch(Path('/System/Applications/TextEdit.app'), [document])
        window = self.window(self.editor, document.name)
        setup = self.select(window)
        expected = 'Scriptaro transport: café 🦀.\n' + '0123456789 ' * 8 + '\n'
        path = self.output / 'transport.yaml'
        path.write_text(json.dumps(dict(version=1, defaults=dict(character_delay_ms=40), sections=[
            dict(name='Take', setup=setup,
                 reset=setup + [key('a', True), key('backspace'), action('wait', duration_ms=200)],
                 steps=[action('type_text', text=expected), key('s', True), action('wait', duration_ms=250)])
        ]), ensure_ascii=False, indent=2))
        stream = (self.output / 'desktop.log').open('w')
        process = subprocess.Popen([str(bundle / 'Contents/MacOS/Scriptaro'), str(path)], stdout=stream, stderr=stream)
        stream.close()
        self.owned.append(process)
        self.desktop = process.pid
        wait_for(lambda: self.button('Play'))
        self.drive('pick', 'Simulation', 'Desktop playback')
        wait_for(lambda: self.selection('Desktop playback'))
        self.drive('pick', 'All sections', 'Take')
        wait_for(lambda: self.selection('Take'))
        for index in range(1, self.runs + 1):
            self.drive('click', 'Reset + retake')
            wait_for(lambda: 8 <= len(self.contents()) < len(expected))
            self.assert_focus(window)
            self.drive('click', 'Pause')
            wait_for(lambda: self.button('Resume'))
            time.sleep(0.2)  # Allow an already-posted key to settle before measuring.
            paused = self.contents()
            self.assert_focus(window)
            time.sleep(0.6)
            if self.contents() != paused:
                raise AssertionError('text changed while transport was paused')
            self.drive('click', 'Resume')
            wait_for(lambda: len(self.contents()) >= len(paused) + 8)
            self.assert_focus(window)
            if stop_while_paused:
                self.drive('click', 'Pause')
                wait_for(lambda: self.button('Resume'))
                self.assert_focus(window)
            self.drive('click', 'Stop')
            wait_for(lambda: self.button('Reset + retake'))
            time.sleep(0.2)
            stopped = self.contents()
            time.sleep(0.6)
            if self.contents() != stopped or not expected.startswith(stopped) or len(stopped) >= len(expected):
                raise AssertionError('Stop did not cancel with a stable, partial prefix')
            log = self.snapshot(self.desktop)
            if not any('Cancelled:' in str(row.get('AXValue', '')) for row in log['rows']):
                raise AssertionError('desktop did not report cancellation')
            self.drive('click', 'Reset + retake')
            wait_for(lambda: self.button('Pause'))
            wait_for(lambda: self.button('Reset + retake'), timeout=20)
            if document.read_text() != expected or self.contents() != expected:
                raise AssertionError('fresh retake after cancellation did not save exact content')
            log = self.snapshot(self.desktop)
            if not any('Completed:' in str(row.get('AXValue', '')) for row in log['rows']):
                raise AssertionError('desktop did not report completed retake')
            self.report['gui_transport']['cycles'].append(dict(iteration=index, paused=paused,
                cancelled=stopped, expected=expected, observed=document.read_text(), focus_preserved=True))
            self.report['gui_transport']['completed'] = index
            self.save()
            print(f'GUI transport: {index}/{self.runs}', flush=True)
        self.report['gui_transport']['status'] = 'passed'

    def drive(self, *args):
        self.command('transport_' + args[0], [self.driver, self.desktop, *args])

    def snapshot(self, pid):
        try:
            result = subprocess.run([str(self.driver), str(pid), 'snapshot'], check=True,
                                    capture_output=True, text=True, timeout=15)
        except subprocess.TimeoutExpired as error:
            raise RuntimeError(f'snapshot timeout: {error.stderr!r}') from error
        data = json.loads(result.stdout)
        (self.output / f'last-snapshot-{pid}.json').write_text(json.dumps(data, indent=2, ensure_ascii=False))
        return data

    def selection(self, value):
        return any(row.get('AXRole') == 'AXPopUpButton' and row.get('AXValue') == value
                   for row in self.snapshot(self.desktop)['rows'])

    def button(self, title):
        return any(row.get('AXRole') == 'AXButton' and row.get('AXTitle') == title and row.get('AXEnabled')
                   for row in self.snapshot(self.desktop)['rows'])

    def contents(self):
        fields = [row.get('AXValue', '') for row in self.snapshot(self.editor)['rows']
                  if row.get('AXRole') == 'AXTextArea']
        if len(fields) != 1:
            raise AssertionError('scratch TextEdit instance must expose exactly one editor')
        return fields[0]

    def assert_focus(self, window):
        snapshot = self.snapshot(self.editor)
        if (snapshot['frontmost_pid'] != self.editor or snapshot['window'] != window['title'] or
                snapshot['focused_identifier'] != 'First Text View'):
            raise AssertionError('transport click stole target app/window/control focus')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', type=int, default=20)
    parser.add_argument('--stop-while-paused', action='store_true')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/scriptaro')
    parser.add_argument('--bundle', type=Path, default=ROOT / 'target/Scriptaro.app')
    args = parser.parse_args()
    if sys.platform != 'darwin' or not 1 <= args.runs <= 100:
        parser.error('requires macOS and 1–100 runs')
    test = Transport(args.binary, args.runs, prefix='transport-')
    try:
        test.exercise(args.bundle.resolve(strict=True), args.stop_while_paused)
        test.report['status'] = 'passed'
    except (Exception, KeyboardInterrupt) as error:
        test.report['status'] = 'interrupted' if isinstance(error, KeyboardInterrupt) else 'failed'
        test.report['reason'] = str(error)
        if isinstance(test.report['gui_transport'], dict):
            test.report['gui_transport']['status'] = test.report['status']
        if hasattr(test, 'desktop'):
            try:
                test.snapshot(test.desktop)
            except Exception:
                pass
        print(f'FAILED: {error}', file=sys.stderr, flush=True)
    finally:
        test.report['finished_at'] = datetime.now(timezone.utc).isoformat()
        test.close()
    return 0 if test.report['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
