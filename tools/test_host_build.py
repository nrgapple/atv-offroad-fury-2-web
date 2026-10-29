"""Synthetic concurrency tests: never compile, kill existing work, or mutate host config."""
import contextlib
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import host_build as guard
import install_host_build_budget as install
import desktop_release_watcher as watcher

TOOLS = Path(__file__).resolve().parent


class GuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.env = dict(os.environ, PYTHONPATH=str(TOOLS), FURY_BUILD_STATE=self.tmp.name)
        self.env.pop('CI', None)
        self.env.pop(guard.OWNER, None)

    def spawn(self, body):
        return subprocess.Popen([sys.executable, '-c', 'import host_build as g; g.busy_reason=lambda:None; ' + body],
                                env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def test_serialization_and_nesting(self):
        code = "import time; print('ready',flush=True); time.sleep(.5)"
        first = self.spawn(f'import sys; sys.exit(g.run([sys.executable,"-c",{code!r}]))')
        self.assertEqual(first.stdout.readline().strip(), 'ready')
        nested = "import host_build as g,sys; sys.exit(g.run([sys.executable,'-c',\"print('nested')\"]))"
        second = self.spawn(f'import sys; sys.exit(g.run([sys.executable,"-c",{nested!r}]))')
        time.sleep(.1)
        self.assertIsNone(second.poll())
        first.communicate(timeout=5)
        out, err = second.communicate(timeout=8)
        self.assertEqual(second.returncode, 0, err)
        self.assertIn('nested', out)

    def test_cancellation_releases_slot_and_signals_only_owned_child(self):
        child = "import time; print('ready',flush=True); time.sleep(30)"
        first = self.spawn(f'import sys; sys.exit(g.run([sys.executable,"-c",{child!r}]))')
        self.assertEqual(first.stdout.readline().strip(), 'ready')
        first.send_signal(signal.SIGTERM)
        first.communicate(timeout=5)
        second = self.spawn("import sys; sys.exit(g.run([sys.executable,'-c',\"print('next')\"],wait=1))")
        out, err = second.communicate(timeout=5)
        self.assertEqual(second.returncode, 0, err)
        self.assertIn('next', out)

    def test_pressure_defers_without_launching(self):
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard, 'busy_reason', return_value='pressure'):
            with self.assertRaisesRegex(BlockingIOError, 'pressure'):
                with guard.lease(wait=0):
                    self.fail('must not start')

    def test_stale_owner_does_not_bypass_lock(self):
        with patch.dict(os.environ, {guard.OWNER: '99999999'}):
            self.assertFalse(guard.owns_lease())

    def test_legacy_build_detection_ignores_waiting_guard(self):
        rows = '444 1 /usr/bin/python3 /usr/bin/python3 tools/host_build.py -- tools/build_desktop.py\n'
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=({}, rows)):
            self.assertIsNone(guard.busy_reason())
        rows = '445 1 /usr/bin/python3 /usr/bin/python3 tools/build_desktop.py\n'
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=({}, rows)):
            self.assertIn('445', guard.busy_reason())

    def test_waiting_browser_parents_do_not_deadlock_each_other(self):
        rows = ('444 1 /usr/bin/node /usr/bin/node tools/run_browser_tests.mjs\n'
                '445 444 /usr/bin/python3 /usr/bin/python3 tools/host_build.py -- node tools/run_browser_tests.mjs\n')
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=({445: 444, 444: 1}, rows)):
            self.assertIsNone(guard.busy_reason())

    def test_grandfather_identity_cannot_match_a_reused_pid(self):
        import json
        directory = Path(self.tmp.name)
        (directory / 'grandfather.json').write_text(json.dumps([{'pid': 555, 'started': 'old start'}]))
        with patch.object(guard, 'STATE', directory), patch.object(guard, 'processes', return_value=({os.getpid(): 555}, '')), \
             patch.object(guard.subprocess, 'check_output', return_value='new start'):
            self.assertFalse(guard.grandfathered())
        with patch.object(guard, 'STATE', directory), patch.object(guard, 'processes', return_value=({os.getpid(): 555}, '')), \
             patch.object(guard.subprocess, 'check_output', return_value='old start'):
            self.assertTrue(guard.grandfathered())

    def test_handoff_waits_for_old_writer_before_reload_and_preserves_history(self):
        import json
        import plistlib
        home = Path(self.tmp.name)
        service = home / 'service'
        runtime = service / 'runtime'
        runtime.mkdir(parents=True)
        (runtime / 'com.furyremastered.desktop-release.plist.in').write_text(
            '<plist version="1.0"><dict><key>Label</key><string>test</string></dict></plist>')
        (home / 'Library/LaunchAgents').mkdir(parents=True)
        legacy = home / 'LargeFiles/fury-desktop'
        legacy.mkdir(parents=True)
        (legacy / 'watcher-state.json').write_text(json.dumps({'done': {'preserved': {}}, 'failed': {}}))
        with patch.object(install, 'SERVICE', service), patch.object(install.Path, 'home', return_value=home), \
             patch.object(install, 'running_watcher', side_effect=[123, None]), patch.object(install.subprocess, 'run') as run, \
             patch.object(install.time, 'sleep', side_effect=lambda _: run.assert_not_called()):
            install.finish_handoff(home / 'repo')
        self.assertIn('preserved', json.loads((service / 'watcher-state.json').read_text())['done'])
        commands = [call.args[0][1] for call in run.call_args_list]
        self.assertEqual(commands, ['bootout', 'enable', 'bootstrap'])
        self.assertEqual(json.loads((service / 'handoff.json').read_text())['status'], 'installed')

    def test_cargo_install_preserves_proxy_and_refuses_custom_binary(self):
        home = Path(self.tmp.name)
        (home / '.cargo/bin').mkdir(parents=True)
        cargo = home / '.cargo/bin/cargo'
        cargo.symlink_to('rustup')
        install.cargo_install(TOOLS, home)
        self.assertFalse(cargo.is_symlink())
        self.assertEqual(os.readlink(cargo.with_name('cargo.fury-original')), 'rustup')
        install.cargo_install(TOOLS, home)
        cargo.write_text('custom binary')
        with self.assertRaises(ValueError):
            install.cargo_install(TOOLS, home)


class ReleaseTests(unittest.TestCase):
    def test_attempt_failure_throttles_even_new_commit(self):
        for state in [{'last_attempt': {'at': 100}}, {'failed': {'old': {'at': 100}}}]:
            with patch.object(watcher, 'git', side_effect=AssertionError('must throttle before inspecting new main')):
                tag, reason = watcher.main_candidate(Path('.'), {}, state, 12, now=200)
                self.assertIsNone(tag)
                self.assertIn('cooldown', reason)

    def test_atomic_state_preserves_previous_on_replace_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'state.json'
            watcher.save_state(path, {'done': {'old': {}}})
            with patch.object(watcher.os, 'replace', side_effect=OSError('disk unavailable')):
                with self.assertRaises(OSError):
                    watcher.save_state(path, {'done': {}})
            self.assertIn('old', watcher.load_state(path, {})['done'])
            self.assertEqual([p.name for p in Path(tmp).iterdir()], ['state.json'])

    def test_storage_preflight_uses_mountpoint_and_uuid(self):
        import plistlib
        data = plistlib.dumps({'MountPoint': '/Volumes/External SSD',
                              'VolumeUUID': '4CC9A66C-3ECB-4EC5-A1AF-221A7A925267'})
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'Cargo.toml').write_text('test')
            content = root / 'private/reports/hosted-content'
            content.mkdir(parents=True)
            (content / 'manifest.json').write_text('{}')
            with patch.object(watcher.sys, 'platform', 'darwin'), patch.object(watcher.subprocess, 'check_output', return_value=data), \
                 patch.object(watcher, 'OUT_ROOT', root / 'out'), patch.object(watcher, 'WORKTREES', root / 'trees'):
                watcher.preflight(root)
            bad = plistlib.dumps({'MountPoint': '/Volumes/External SSD', 'VolumeUUID': 'wrong'})
            with patch.object(watcher.sys, 'platform', 'darwin'), patch.object(watcher.subprocess, 'check_output', return_value=bad):
                with self.assertRaises(OSError):
                    watcher.preflight(root)

    def test_corrupt_state_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'state.json'
            path.write_text('broken')
            with self.assertRaises(ValueError):
                watcher.load_state(path, {})

    def test_busy_pass_does_not_mark_attempt_or_build(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'state.json'
            watcher.save_state(path, {'done': {}, 'failed': {}})
            with patch.dict(os.environ, {}, clear=True), patch.object(watcher, 'remote_tags', return_value={'desktop-v1': 'abc'}), \
                 patch.object(watcher, 'lease', side_effect=BlockingIOError('busy')), patch.object(watcher, 'process') as build:
                self.assertEqual(watcher.main(['--state', str(path)]), 0)
                build.assert_not_called()
            self.assertNotIn('last_attempt', watcher.load_state(path, {}))


if __name__ == '__main__':
    unittest.main()
