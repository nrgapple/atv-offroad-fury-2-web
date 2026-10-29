"""Desktop resource/cadence regressions; no real builds, VMs or live processes."""
import argparse
from contextlib import ExitStack, nullcontext
import io
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import build_android
import build_desktop as desktop
import desktop_release_watcher as watcher
import test_desktop_release as release_tests


class DesktopBudgetTests(unittest.TestCase):
    def exercise_build(self, requested, failure=None):
        events = []

        class VM:
            def __init__(self, profile):
                pass

            def __enter__(self):
                events.append('vm-start')

            def __exit__(self, *exc):
                events.append('vm-stop')

        with tempfile.TemporaryDirectory() as tmp, ExitStack() as stack:
            out = Path(tmp)
            (out / 'work').mkdir()
            archive = out / 'synthetic.zip'
            archive.write_bytes(b'fixture')
            ctx = SimpleNamespace(out=out, work=out / 'work', timings={}, commit='a' * 40,
                                  short='aaaaaaa', version='0.3.0', dirty=False)

            def mac(*args):
                events.append('mac')
                if failure == 'mac':
                    raise OSError('synthetic native failure')
                return archive

            def docker(*args):
                events.append('docker')
                if failure == 'docker':
                    raise OSError('synthetic docker failure')
                return out / 'binary'

            for name, replacement in [('Context', lambda args: ctx), ('ensure_pack', lambda args: out / 'pack'),
                                      ('BuilderVM', VM), ('build_macos', mac), ('docker_build', docker),
                                      ('attach_and_archive', lambda *a: archive)]:
                stack.enter_context(patch.object(desktop, name, replacement))
            stack.enter_context(patch.object(desktop.desktop_archive, 'check_contents'))
            stack.enter_context(patch.object(desktop.desktop_archive, 'write_checksums'))
            stack.enter_context(patch('sys.stdout', new_callable=io.StringIO))
            args = SimpleNamespace(smoke_only=None, platforms=requested, builder_vm='test', smoke=False, keep_work=False)
            if failure:
                with self.assertRaises(OSError):
                    desktop.build(args, argparse.ArgumentParser())
            else:
                self.assertEqual(desktop.build(args, argparse.ArgumentParser()), 0)
        return events

    def test_native_finishes_before_one_docker_session_even_when_requested_last(self):
        self.assertEqual(self.exercise_build('windows-x86_64,macos-universal,linux-aarch64'),
                         ['mac', 'vm-start', 'docker', 'docker', 'vm-stop'])

    def test_native_only_never_opens_a_vm(self):
        self.assertEqual(self.exercise_build('macos-universal'), ['mac'])

    def test_native_failure_never_starts_vm(self):
        self.assertEqual(self.exercise_build('macos-universal,linux-aarch64', 'mac'), ['mac'])

    def test_docker_failure_still_closes_owned_vm_context(self):
        self.assertEqual(self.exercise_build('linux-aarch64', 'docker'), ['vm-start', 'docker', 'vm-stop'])

    def test_reused_vm_is_not_resized_or_stopped_and_docker_host_is_restored(self):
        with patch.dict(os.environ, {'DOCKER_HOST': 'unix://live-services'}, clear=True), \
             patch.object(desktop.shutil, 'which', return_value='/colima'), \
             patch.object(desktop.BuilderVM, 'socket', return_value='unix://existing-builder'), \
             patch.object(desktop.BuilderVM, 'colima') as command:
            with self.assertRaisesRegex(ValueError, 'synthetic'):
                with desktop.BuilderVM('test'):
                    self.assertEqual(os.environ['DOCKER_HOST'], 'unix://existing-builder')
                    raise ValueError('synthetic')
            self.assertEqual(os.environ['DOCKER_HOST'], 'unix://live-services')
            command.assert_not_called()

    def test_new_vm_stops_and_removes_temporary_docker_host(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True), \
             patch.dict(desktop.BUILDER_VM, {'home': Path(tmp)}), \
             patch.object(desktop.shutil, 'which', return_value='/colima'), \
             patch.object(desktop.BuilderVM, 'socket', side_effect=[None, 'unix://new-builder']), \
             patch.object(desktop.BuilderVM, 'colima') as command:
            with desktop.BuilderVM('test'):
                self.assertEqual(os.environ['DOCKER_HOST'], 'unix://new-builder')
            self.assertNotIn('DOCKER_HOST', os.environ)
            self.assertEqual(command.call_args_list[0].args,
                             ('start', '--vm-type', 'vz', '--vz-rosetta', '--cpus', '2', '--memory', '8',
                              '--disk', '60', '--mount', f'{Path.home() / ".cargo/registry"}:w'))
            self.assertEqual(command.call_args_list[1].args, ('stop',))

    def test_missing_socket_after_start_cleans_up_owned_vm(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True), \
             patch.dict(desktop.BUILDER_VM, {'home': Path(tmp)}), \
             patch.object(desktop.shutil, 'which', return_value='/colima'), \
             patch.object(desktop.BuilderVM, 'socket', return_value=None), \
             patch.object(desktop.BuilderVM, 'colima') as command:
            with self.assertRaisesRegex(OSError, 'no Docker socket'):
                with desktop.BuilderVM('test'):
                    self.fail('must fail closed')
            self.assertEqual(command.call_args_list[-1].args, ('stop',))
            self.assertNotIn('DOCKER_HOST', os.environ)

    def test_android_requests_and_returns_actual_shared_library(self):
        with tempfile.TemporaryDirectory() as tmp:
            library = Path(tmp) / build_android.TRIPLE / 'release' / build_android.LIBRARY
            library.parent.mkdir(parents=True)
            library.write_bytes(b'synthetic')
            sdk = SimpleNamespace(cargo_env=lambda: {'CARGO_TARGET_DIR': tmp})
            with patch.object(build_android, 'run') as run:
                self.assertEqual(build_android.build_library(sdk, None), library)
            command = run.call_args.args[0]
            self.assertEqual(command[1], 'rustc')
            self.assertEqual(command[command.index('--crate-type') + 1], 'cdylib')
            self.assertIn('--release', command)


class WeeklyReleaseTests(unittest.TestCase):
    def test_weekly_interval_coalesces_changed_main_and_includes_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            run, released = release_tests.WatcherTests().make_repo(root)
            (root / 'crates/a.rs').write_text('changed')
            run('commit', '-qam', 'app')
            run('update-ref', 'refs/remotes/origin/main', 'HEAD')
            tags = {'desktop-v0.2.0-' + released[:7]: released}
            start = 1000000000
            for state in [{'last_attempt': {'at': start}}, {'failed': {'previous': {'at': start}}},
                          {'done': {'previous': {'at': start}}}]:
                self.assertIsNone(watcher.main_candidate(root, tags, state, watcher.AUTO_RELEASE_HOURS,
                                                        now=start + 168 * 3600 - 1)[0])
                self.assertIsNotNone(watcher.main_candidate(root, tags, state, watcher.AUTO_RELEASE_HOURS,
                                                           now=start + 168 * 3600)[0])

    def test_explicit_tag_backlog_builds_one_and_leaves_rest_pending(self):
        tags = {'desktop-v1-a': 'a' * 40, 'desktop-v1-b': 'b' * 40}
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True):
            path = Path(tmp) / 'state.json'
            watcher.save_state(path, {'done': {}, 'failed': {}})
            with patch.object(watcher, 'remote_tags', return_value=tags), \
                 patch.object(watcher, 'lease', side_effect=lambda **k: nullcontext()), \
                 patch.object(watcher, 'preflight'), patch.object(watcher, 'prune_builds'), \
                 patch.object(watcher, 'notify'), patch.object(watcher, 'main_candidate') as candidate, \
                 patch.object(watcher, 'process', return_value=None) as build, \
                 patch('sys.stdout', new_callable=io.StringIO):
                self.assertEqual(watcher.main(['--state', str(path), '--auto-main']), 0)
                self.assertEqual(build.call_count, 1)
                candidate.assert_not_called()
            self.assertEqual(set(watcher.pending(tags, json.loads(path.read_text()))), {'desktop-v1-b'})

    def test_default_cadence_is_weekly(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True), \
             patch.object(watcher, 'remote_tags', return_value={}), \
             patch.object(watcher.subprocess, 'run'), \
             patch.object(watcher, 'main_candidate', return_value=(None, 'not due')) as candidate, \
             patch('sys.stdout', new_callable=io.StringIO):
            self.assertEqual(watcher.main(['--state', str(Path(tmp) / 'state.json'), '--auto-main']), 0)
            self.assertEqual(candidate.call_args.args[3], 168)

    def test_invalid_cooldowns_fail_before_host_or_git_access(self):
        for value in ['0', '-1', 'nan', 'inf']:
            with patch('sys.stderr', new_callable=io.StringIO), self.assertRaises(SystemExit):
                watcher.main(['--min-hours', value])


if __name__ == '__main__':
    unittest.main()
