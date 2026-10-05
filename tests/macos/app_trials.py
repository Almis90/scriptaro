#!/usr/bin/env python3
"""Opt-in real-app trials on a logged-in macOS desktop; leave it untouched.

Uses new TextEdit documents, an isolated Chrome profile, and one new Terminal
window. All input goes through Scriptaro. Nothing is uploaded. Stops on failure;
effects are never retried. Scratch files and exact scripts remain in the report.
"""
import argparse
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import platform
import plistlib
import shlex
import signal
import subprocess
import sys
import tempfile
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
CHROME = Path('/Applications/Google Chrome.app')


def wait_for(predicate, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.2)
    raise TimeoutError('readiness did not arrive before the deadline')


def action(kind, **fields):
    return dict(action=kind, **fields)


def key(name, primary=False):
    return action('key_press', key=name, modifiers=['primary'] if primary else [])


class Trials:
    def __init__(self, binary, runs, *, prefix="apps-"):
        self.binary = str(binary.resolve(strict=True))
        (ROOT / 'target/verification').mkdir(parents=True, exist_ok=True)
        self.output = Path(tempfile.mkdtemp(prefix=prefix, dir=ROOT / 'target/verification'))
        self.scratch = Path(tempfile.mkdtemp(prefix='scriptaro-apps-', dir='/private/tmp'))
        self.runs = runs
        self.owned = []
        self.owned_pids = []
        self.server = None
        self.terminal = None
        self.report = dict(schema_version=1, status='running',
                           started_at=datetime.now(timezone.utc).isoformat(),
                           os=platform.platform(), binary=self.binary,
                           scratch=str(self.scratch), requested_runs=runs,
                           gui_transport='not_run', checks=[],
                           trials={name: dict(status='not_run', completed=0) for name in
                                   ('document_editing', 'browser_form', 'coding_demo')})
        self.save()
        print('Report: ' + str(self.output / 'report.json'), flush=True)

    def save(self):
        temporary = self.output / 'report.tmp'
        temporary.write_text(json.dumps(self.report, indent=2, ensure_ascii=False) + '\n')
        temporary.replace(self.output / 'report.json')

    def command(self, name, argv, check=True):
        started = time.monotonic()
        try:
            result = subprocess.run([str(arg) for arg in argv], capture_output=True,
                                    text=True, timeout=30)
        except (OSError, subprocess.TimeoutExpired) as error:
            self.report['checks'].append(dict(name=name, returncode=None,
                                             seconds=round(time.monotonic() - started, 3),
                                             error=str(error)))
            self.save()
            raise
        self.report['checks'].append(dict(name=name, returncode=result.returncode,
                                         seconds=round(time.monotonic() - started, 3),
                                         stdout=result.stdout, stderr=result.stderr))
        self.save()
        if check and result.returncode:
            raise RuntimeError(f'{name}: {result.stderr or result.stdout}')
        return result

    def cli(self, name, *args, check=True):
        return self.command(name, [self.binary, *args], check=check)

    def windows(self, pid, required=False):
        result = self.cli('discover_windows', 'windows', '--pid', pid, check=required)
        if result.returncode:
            return []  # Read-only startup discovery may be polled; effects never are.
        return [json.loads(line) for line in result.stdout.splitlines() if line]

    def window(self, pid, title):
        wait_for(lambda: title in self.windows(pid))
        return dict(app=dict(by='pid', value=pid), title=title)

    def launch(self, app, arguments):
        with (app / 'Contents/Info.plist').open('rb') as file:
            metadata = plistlib.load(file)
        self.report.setdefault('app_versions', {})[app.stem] = metadata.get('CFBundleShortVersionString')
        self.save()
        if app.stem == 'TextEdit':
            def pids():
                rows = self.cli('discover_textedit', 'apps').stdout.splitlines()
                return {int(cells[0]) for row in rows if len(cells := row.split('\t')) >= 3
                        and cells[1] == 'com.apple.TextEdit'}
            before = pids()
            self.command('launch_textedit', ['open', '-n', '-a', app, *arguments,
                                            '--args', '-ApplePersistenceIgnoreState', 'YES'])
            new = wait_for(lambda: pids() - before)
            if len(new) != 1:
                raise RuntimeError('could not uniquely identify the new TextEdit instance')
            pid = new.pop()
            self.owned_pids.append(pid)
            return pid
        executable = app / 'Contents/MacOS' / (
            'Google Chrome' if app == CHROME else 'TextEdit')
        stream = (self.output / (app.stem + '.log')).open('w')
        child = subprocess.Popen([str(executable), *map(str, arguments)], stdout=stream, stderr=stream)
        stream.close()
        self.owned.append(child)
        return child.pid

    def play(self, name, setup, reset, steps, retake=False, pause=False):
        script = dict(version=1, defaults=dict(character_delay_ms=20, timeout_ms=5000),
                      sections=[dict(name='Take', setup=setup, reset=reset, steps=steps)])
        path = self.output / (name + '.yaml')
        # Literal Unicode avoids JSON surrogate escapes, which YAML does not accept.
        path.write_text(json.dumps(script, indent=2, ensure_ascii=False) + '\n')
        argv = [self.binary, 'run', str(path), '--section', 'Take', '--start-delay-ms', '0']
        if retake:
            argv.append('--retake')
        if not pause:
            return self.command(name, argv)
        log = self.output / (name + '.stdout')
        err = self.output / (name + '.stderr')
        started = time.monotonic()
        # First body action is a 600 ms wait. Pause inside it, past its deadline;
        # ensure no subsequent action starts until resume.
        wait_step = (len(reset) if retake else 0) + len(setup) + 1
        failure = None
        with log.open('w') as out, err.open('w') as errors:
            process = subprocess.Popen(argv, stdout=out, stderr=errors)
            try:
                wait_for(lambda: f'  {wait_step}: wait\n' in log.read_text())
                process.send_signal(signal.SIGUSR1)
                time.sleep(1.0)
                if process.poll() is not None or f'  {wait_step + 1}:' in log.read_text():
                    raise AssertionError('playback advanced while paused')
                process.send_signal(signal.SIGUSR2)
                process.wait(timeout=45)
            except (Exception, KeyboardInterrupt) as error:
                failure = error
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
        self.report['checks'].append(dict(name=name, returncode=process.returncode,
                                         seconds=round(time.monotonic() - started, 3),
                                         pause_resume=True, stdout=log.read_text(), stderr=err.read_text()))
        self.save()
        if failure is not None:
            raise failure
        if process.returncode:
            raise RuntimeError(f'{name}: {err.read_text()}')

    @staticmethod
    def select(window, identifier='First Text View'):
        control = dict(window=window, role='text_area', identifier=identifier)
        return [action('activate_window', window=window), action('focus_control', control=control)]

    def completed(self, name, iteration, **evidence):
        self.report['trials'][name].update(status='running', completed=iteration, **evidence)
        self.save()
        print(f'{name}: {iteration}/{self.runs}', flush=True)

    def documents(self):
        token = uuid.uuid4().hex[:10]
        self.document = self.scratch / f'Scriptaro Notes {token}.txt'
        self.source = self.scratch / f'Scriptaro Source {token}.txt'
        self.document.write_text('')
        self.source.write_text('')
        self.editor_pid = self.launch(Path('/System/Applications/TextEdit.app'), [self.document, self.source])
        self.document_window = self.window(self.editor_pid, self.document.name)
        self.source_window = self.window(self.editor_pid, self.source.name)
        self.cli('textedit_controls', 'controls', '--pid', self.editor_pid, '--window', self.document.name)
        setup = self.select(self.document_window)
        reset = setup + [key('a', True), key('backspace')]
        for index in range(1, self.runs + 1):
            expected = f'Scriptaro take {index}\nHello café Καλημέρα 🦀\nEnd\n'
            self.play(f'document-{index:03}', setup, reset,
                      [action('wait', duration_ms=600), action('type_text', text=expected),
                       key('s', True), action('wait', duration_ms=350)],
                      retake=index > 1, pause=index in (1, self.runs))
            actual = self.document.read_text()
            if actual != expected:
                raise AssertionError(f'document mismatch: expected {expected!r}, observed {actual!r}')
            self.completed('document_editing', index, expected=expected, observed=actual)
        self.report['trials']['document_editing']['status'] = 'passed'
        self.save()

    def browser(self):
        self.receipts = []
        self.previews = []
        self.web_lock = threading.Lock()
        token = uuid.uuid4().hex[:10]
        self.form_title = 'Scriptaro Form ' + token
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                if self.path == '/preview':
                    result = owner.scratch / 'result.txt'
                    value = result.read_text() if result.exists() else 'pending'
                    import html
                    body = f'''<!doctype html><title>Scriptaro Preview {token}</title>
<pre id="result">{html.escape(value)}</pre><script>
fetch('/seen', {{method:'POST',body:JSON.stringify({{value:document.querySelector('pre').textContent}})}});
</script>'''
                else:
                    body = f'''<!doctype html><title>{owner.form_title}</title>
<form><label>Trial name <input aria-label="Trial name" id="name" autocomplete="off"></label>
<label>Trial notes <textarea aria-label="Trial notes" id="notes"></textarea></label>
<button type="submit" aria-label="Submit trial">Submit trial</button></form>
<script>document.querySelector('form').onsubmit = e => {{e.preventDefault();
fetch('/submit', {{method:'POST',body:JSON.stringify({{name:document.getElementById('name').value,
notes:document.getElementById('notes').value}})}});}};</script>'''
                data = body.encode()
                self.send_response(200)
                self.send_header('Content-Type', 'text/html; charset=utf-8')
                self.send_header('Cache-Control', 'no-store')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_POST(self):
                value = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                with owner.web_lock:
                    (owner.receipts if self.path == '/submit' else owner.previews).append(value)
                self.send_response(204)
                self.end_headers()

        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = f'http://127.0.0.1:{self.server.server_port}'
        self.browser_pid = self.launch(CHROME, [
            '--user-data-dir=' + str(self.scratch / 'chrome-profile'), '--no-first-run',
            '--no-default-browser-check', '--disable-background-networking',
            '--force-renderer-accessibility', '--new-window', self.url])
        title = wait_for(lambda: next((title for title in self.windows(self.browser_pid)
                                     if self.form_title in title), None))
        self.browser_window = self.window(self.browser_pid, title)
        self.cli('chrome_controls', 'controls', '--pid', self.browser_pid, '--window', title)
        fields = [dict(window=self.browser_window, role=role, label=label) for role, label in
                  [('text_field', 'Trial name'), ('text_area', 'Trial notes'), ('button', 'Submit trial')]]
        setup = [action('activate_window', window=self.browser_window)]
        reset = setup + [step for field in fields[:2] for step in
                         [action('focus_control', control=field), key('a', True), key('backspace'),
                          action('wait', duration_ms=200)]]
        for index in range(1, self.runs + 1):
            expected = dict(name=f'Take {index}', notes=f'Hello café 🦀\nTake {index}')
            steps = [action('wait', duration_ms=600)]
            for field, value in zip(fields, expected.values()):
                # CGEvent posting is asynchronous; allow the last character to
                # reach this field before an AX focus operation overtakes it.
                steps += [action('focus_control', control=field), action('type_text', text=value),
                          action('wait', duration_ms=200)]
            steps += [action('invoke_control', control=fields[2]), action('wait', duration_ms=350)]
            self.play(f'browser-{index:03}', setup, reset, steps,
                      retake=index > 1, pause=index in (1, self.runs))
            wait_for(lambda: len(self.receipts) >= index)
            with self.web_lock:
                actual = list(self.receipts)
            if len(actual) != index or actual[-1] != expected:
                raise AssertionError(f'browser submission mismatch: expected {expected!r}, receipts {actual!r}')
            self.completed('browser_form', index, submissions=len(actual), expected=expected, observed=actual[-1])
        self.report['trials']['browser_form']['status'] = 'passed'
        self.preview_title = 'Scriptaro Preview ' + token
        self.save()

    def coding(self):
        token = 'Scriptaro Terminal ' + uuid.uuid4().hex[:10]
        self.terminal_token = token
        launcher = self.scratch / 'trial.command'
        launcher.write_text('#!/bin/zsh\ncd ' + shlex.quote(str(self.scratch)) +
                            '\nprintf "\\033]0;' + token + '\\007"\n' +
                            'export HISTFILE=/dev/null ZDOTDIR=' + shlex.quote(str(self.scratch)) +
                            '\nexport SHELL_SESSIONS_DISABLE=1\nexec /bin/zsh -f\n')
        launcher.chmod(0o700)
        self.command('open_owned_terminal', ['open', '-a', 'Terminal', launcher])
        last_title = None
        stable_since = time.monotonic()

        def locate():
            nonlocal last_title, stable_since
            apps = self.cli('discover_terminal', 'apps').stdout.splitlines()
            for row in apps:
                cells = row.split('\t')
                if len(cells) >= 3 and cells[1] == 'com.apple.Terminal':
                    pid = int(cells[0])
                    for title in self.windows(pid):
                        if token in title:
                            # Terminal briefly exposes the launching login shell
                            # before reflecting our zsh. Do not save that title.
                            if title != last_title:
                                last_title, stable_since = title, time.monotonic()
                            if time.monotonic() - stable_since < 1.0:
                                return None
                            return dict(app=dict(by='pid', value=pid), title=title)
            return None

        self.terminal = wait_for(locate)
        self.report['terminal_window'] = self.terminal
        with Path('/System/Applications/Utilities/Terminal.app/Contents/Info.plist').open('rb') as file:
            self.report['app_versions']['Terminal'] = plistlib.load(file).get('CFBundleShortVersionString')
        self.play('open-preview', [action('activate_window', window=self.browser_window)], [],
                  [key('l', True), action('type_text', text=self.url + '/preview'), key('enter'),
                   action('wait', duration_ms=1000)])
        title = wait_for(lambda: next((title for title in self.windows(self.browser_pid)
                                     if self.preview_title in title), None))
        preview_window = self.window(self.browser_pid, title)
        setup = self.select(self.source_window)
        reset = setup + [key('a', True), key('backspace')]
        for index in range(1, self.runs + 1):
            source = f'print(42 + {index})\n'
            receipt = self.scratch / f'command-{index}.txt'
            result = self.scratch / 'result.txt'
            command = ('/usr/bin/python3 ' + shlex.quote(str(self.source)) + ' > ' + shlex.quote(str(result)) +
                       ' && printf done >> ' + shlex.quote(str(receipt)))
            with self.web_lock:
                before = len(self.previews)
            self.play(f'coding-{index:03}', setup, reset,
                      [action('wait', duration_ms=600), action('type_text', text=source), key('s', True),
                       action('wait', duration_ms=350), action('activate_window', window=self.terminal),
                       action('type_text', text=command), key('enter'), action('wait', duration_ms=750),
                       action('activate_window', window=preview_window), key('r', True),
                       action('wait', duration_ms=500), *self.select(self.source_window)],
                      retake=index > 1, pause=index in (1, self.runs))
            if self.source.read_text() != source:
                raise AssertionError('coding source differs from prepared source')
            wait_for(receipt.exists)
            expected = str(42 + index) + '\n'
            if receipt.read_text() != 'done' or result.read_text() != expected:
                raise AssertionError('command did not execute exactly once with expected output')
            wait_for(lambda: len(self.previews) > before)
            with self.web_lock:
                actual = self.previews[-1]['value']
            if actual != expected:
                raise AssertionError(f'preview mismatch: {actual!r} != {expected!r}')
            self.completed('coding_demo', index, source=source, command_receipt=receipt.read_text(),
                           expected=expected, browser_observed=actual)
        self.report['trials']['coding_demo']['status'] = 'passed'
        self.save()

    def close(self):
        if self.terminal and self.report['status'] == 'passed':
            try:
                self.play('exit-owned-shell', [action('activate_window', window=self.terminal)], [],
                          [action('type_text', text='exit'), key('enter'), action('wait', duration_ms=500)])
                titles = self.windows(self.terminal['app']['value'], required=True)
                # Terminal can append a completion suffix after the shell exits.
                matches = [title for title in titles if self.terminal_token in title]
                if len(matches) == 1:
                    window = dict(self.terminal, title=matches[0])
                    self.play('close-owned-terminal', [action('activate_window', window=window)], [],
                              [key('w', True), action('wait', duration_ms=200)])
                    if any(self.terminal_token in title for title in
                           self.windows(self.terminal['app']['value'], required=True)):
                        raise RuntimeError('owned Terminal window remained open after close')
                elif len(matches) > 1:
                    raise RuntimeError('ambiguous owned Terminal window during cleanup')
                self.report['terminal_cleanup'] = 'closed'
            except Exception as error:
                self.report['terminal_cleanup'] = f'Close the scratch Terminal window manually: {error}'
        # Only processes launched directly here are terminated; never kill the
        # user's shared Terminal process or existing Chrome/TextEdit instances.
        for process in reversed(self.owned):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for pid in self.owned_pids:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        if self.server:
            self.server.shutdown()
            self.server.server_close()
        if self.terminal and 'terminal_cleanup' not in self.report:
            self.report['terminal_cleanup'] = 'Owned scratch Terminal window left open for inspection; close it manually.'
        self.save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/scriptaro')
    parser.add_argument('--runs', type=int, default=20)
    args = parser.parse_args()
    if sys.platform != 'darwin' or not 1 <= args.runs <= 100:
        parser.error('requires macOS and --runs between 1 and 100')
    trials = Trials(args.binary, args.runs)
    active = None
    try:
        doctor = trials.cli('permissions', 'doctor').stdout
        statuses = [line.split(' — ', 1)[0] for line in doctor.splitlines()]
        if not all(line in statuses for line in ('Accessibility: granted', 'Post events: granted')):
            raise RuntimeError('Accessibility and event posting permissions are required')
        for active, operation in [('document_editing', trials.documents), ('browser_form', trials.browser),
                                  ('coding_demo', trials.coding)]:
            trials.report['trials'][active]['status'] = 'running'
            trials.save()
            operation()
        trials.report['status'] = 'passed'
    except (Exception, KeyboardInterrupt) as error:
        trials.report.update(status='interrupted' if isinstance(error, KeyboardInterrupt) else 'failed',
                             reason=str(error))
        if active:
            trials.report['trials'][active]['status'] = trials.report['status']
        print(f'FAILED: {error}', file=sys.stderr, flush=True)
    finally:
        trials.report['finished_at'] = datetime.now(timezone.utc).isoformat()
        trials.close()
    return 0 if trials.report['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
