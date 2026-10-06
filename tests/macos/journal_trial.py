#!/usr/bin/env python3
"""One opt-in native TextEdit take; intentionally stops at a failing assertion.

Reuses app_trials' isolated app lifecycle. No repeats, user documents, preference
changes or external services. All scripted input uses the native Scriptaro CLI.
"""
import argparse
from datetime import datetime, timezone
import json
import hashlib
from pathlib import Path
import subprocess
import sys
import uuid

from app_trials import ROOT, Trials, action, key


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def check_trial(trial):
    doctor = json.loads(trial.cli('permissions', 'doctor', '--json').stdout)
    permissions = {item['name']: item['granted'] for item in doctor['data']['permissions']}
    require(all(permissions.get(name) for name in ('Accessibility', 'Post events')),
            'Accessibility and event posting permissions are required; no app was launched')
    trial.report['permissions'] = permissions
    trial.report['commit'] = subprocess.check_output(
        ['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip()
    trial.report['binary_version'] = trial.cli('binary_version', '--version').stdout.strip()
    trial.report['binary_sha256'] = hashlib.sha256(Path(trial.binary).read_bytes()).hexdigest()
    trial.save()

    document = trial.scratch / f'Scriptaro Journal {uuid.uuid4().hex[:10]}.txt'
    document.write_text('', encoding='utf-8')
    pid = trial.launch(Path('/System/Applications/TextEdit.app'), [document])
    window = trial.window(pid, document.name)
    control = dict(window=window, role='text_area', identifier='First Text View')
    trial.cli('textedit_controls', 'controls', '--pid', pid, '--window', document.name)
    expected = 'Journal fixture: café Καλημέρα 🦀\nOne controlled native take.\n'
    blocked = 'THIS ACTION MUST NEVER RUN'
    matches = dict(kind='control_matches', control=control,
                   expect=dict(property='text', equals=expected))
    save = key('s', True)
    save['unverified'] = True  # File contents are checked independently below.
    script = dict(
        version=2, name='Single TextEdit journal qualification', input_boundaries='strict',
        defaults=dict(character_delay_ms=25, timeout_ms=5000),
        sequences=dict(
            enter=dict(params=dict(text=None), steps=[
                action('type_text', text='${text}', after=dict(condition=matches))]),
            reject=[action('assert_control', control=control,
                           expect=dict(property='text', equals='INTENTIONAL MISMATCH'))],
            take=[action('call', sequence='enter', **{'with': dict(text=expected)}),
                  save, action('wait', duration_ms=500, scale_with_speed=False),
                  action('call', sequence='reject'),
                  action('type_text', text=blocked, unverified=True)]),
        sections=[dict(name='Verification', setup=trial.select(window),
                       requires=[dict(kind='control_matches', control=control,
                                      expect=dict(property='text', equals=''))],
                       steps=[action('call', sequence='take')])])
    source = trial.output / 'take.yaml'
    source.write_text(json.dumps(script, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    plan = json.loads(trial.cli('plan', 'plan', source, '--section', 'Verification', '--json').stdout)
    planned = plan['data']['steps']
    failure = next(step for step in planned if step['action'] == 'assert_control')
    typing = next(step for step in planned if step['action'] == 'type_text')
    after = next(step for step in planned if step['source']['generated'] == 'after')
    journal_path = trial.output / 'take.jsonl'
    report_path = trial.output / 'take-result.json'
    trial.report.update(document=str(document), source=str(source), journal=str(journal_path),
                        run_report=str(report_path), expected_failure_step=failure['step'])
    trial.save()

    # Exactly one playback invocation. An expected assertion failure is the end
    # of this take; there is no reset, retry, recovery input or second playback.
    trial.report['native_playback_attempts'] = 1
    trial.save()
    result = trial.cli('native_take', 'run', source, '--section', 'Verification',
                       '--start-delay-ms', '0', '--journal', journal_path,
                       '--report', report_path, '--json', check=False)
    outcome = json.loads(result.stdout)
    # Preserve diagnostic evidence even when the failure happens earlier than
    # planned. Inspecting an existing journal is read-only, not another take.
    summary = json.loads(trial.cli('inspect_journal', 'journal', journal_path, '--json').stdout)['data']
    require(json.loads(report_path.read_text()) == outcome, 'saved report differs from CLI result')
    require(summary['incomplete'] is False and summary['truncated_tail'] is False,
            'failure did not produce a complete journal')
    require(summary['uncertain_action'] is None, 'unexpected unresolved action intent')
    require(summary['run_id'] == outcome['data']['run_id'], 'run ID mismatch')
    require(summary['last_finished_action'] == outcome['data']['last_action'], 'last action mismatch')
    trial.report['observed_playback'] = dict(
        exit_code=result.returncode, error=outcome.get('error'),
        completed_steps=outcome['data']['completed_steps'],
        last_action=outcome['data']['last_action'],
        journal_complete=True, journal_matches_saved_report=True)
    trial.save()
    require(result.returncode == 1, 'take did not stop with the expected failure exit code')
    error = outcome.get('error') or {}
    require(error.get('code') == 'assertion_failed',
            f"unexpected playback failure: {error.get('code')}: {error.get('message')}")
    require(outcome['data']['failed_step'] == failure['step'], 'wrong failed action')
    require(outcome['data']['completed_steps'] == failure['step'] - 1, 'incorrect completed count')
    require(outcome['error']['source'] == failure['source'], 'failure source differs from plan')
    require(outcome['error']['source']['location'] == 'sequences.reject[1]', 'wrong definition path')
    require([call['sequence'] for call in outcome['error']['source']['call_chain']] == ['take', 'reject'],
            'nested call chain was lost')
    require(summary['result']['status'] == 'failed', 'inspector lost the intentional failure')
    contents = journal_path.read_text(encoding='utf-8')
    records = [json.loads(line) for line in contents.splitlines()]
    intents = [record['action'] for record in records if record['event'] == 'step_started']
    finished = {record['action']['step']: record['action'] for record in records
                if record['event'] == 'step_finished'}
    require([item['step'] for item in intents] == list(range(1, failure['step'] + 1)),
            'action was repeated, skipped or dispatched after failure')
    typed = finished[typing['step']]
    require(typed['evidence']['outcome'] == 'dispatched', 'typing was not native dispatch')
    require(typed['evidence']['characters_dispatched'] == len(expected), 'typing count mismatch')
    require(finished[after['step']]['evidence']['outcome'] == 'observed', 'postcondition was not observed')
    require(finished[after['step']]['source'] == after['source'], 'generated wait source was lost')
    require(expected not in contents and 'Journal fixture' not in contents
            and blocked not in contents and 'INTENTIONAL MISMATCH' not in contents,
            'prepared text leaked into journal')
    actual = document.read_text(encoding='utf-8')
    require(actual == expected, 'independently saved TextEdit contents differ from expected text')
    trial.report.update(
        status='passed',
        qualification=dict(app='TextEdit', playback_status='expected_assertion_failure',
                           native_playback_attempts=1, completed_steps=outcome['data']['completed_steps'],
                           failed_step=failure['step'], characters_dispatched=len(expected),
                           saved_file_exact_match=True, postcondition_observed=True,
                           nested_source_match=True, no_action_after_failure=True,
                           prepared_text_absent_from_journal=True, run_id=summary['run_id']),
        limits=['One default-speed take on the recorded macOS/TextEdit versions.',
                'No native partial-typing cancellation or process-kill check.',
                'Browser and cross-application qualification not run.'])
    trial.save()
    print('PASSED: one native take; expected assertion failure, exact saved text, and matching journal evidence.', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/scriptaro')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('requires a logged-in macOS desktop')
    trial = Trials(args.binary, 1, prefix='journal-')
    trial.report.pop('trials')
    trial.report.update(native_playback_attempts=0, scenario='TextEdit journal and verified input boundary')
    trial.save()
    try:
        check_trial(trial)
    except (Exception, KeyboardInterrupt) as error:
        trial.report.update(status='interrupted' if isinstance(error, KeyboardInterrupt) else 'failed',
                            reason=str(error))
        print(f'FAILED: {error}', file=sys.stderr, flush=True)
    finally:
        trial.report['finished_at'] = datetime.now(timezone.utc).isoformat()
        trial.close()
    return 0 if trial.report['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
