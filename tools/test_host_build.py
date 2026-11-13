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

    def spawn(self, body, cwd=None):
        return subprocess.Popen([sys.executable, '-c', 'import host_build as g; g.busy_reason=lambda *args:None; ' + body],
                                env=self.env, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

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

    def hold(self, kind, name):
        directory = Path(self.tmp.name) / name
        directory.mkdir(exist_ok=True)
        body = (f"import time; "
                f"lease=g.lease(kind={kind!r},wait=0); lease.__enter__(); "
                "print('ready',flush=True); time.sleep(30)")
        child = self.spawn(body, cwd=directory)
        self.addCleanup(lambda: self.stop(child))
        self.assertEqual(child.stdout.readline().strip(), 'ready')
        return child

    @staticmethod
    def stop(child):
        if child.poll() is None:
            child.terminate()
        child.communicate(timeout=5)

    def attempt(self, kind, name):
        directory = Path(self.tmp.name) / name
        directory.mkdir(exist_ok=True)
        child = self.spawn(f"lease=g.lease(kind={kind!r},wait=0); lease.__enter__(); print('admitted')", cwd=directory)
        out, err = child.communicate(timeout=5)
        return child.returncode, out, err

    def test_compile_and_browser_overlap_in_distinct_checkouts(self):
        self.hold('compile', 'a')
        code, out, err = self.attempt('browser', 'b')
        self.assertEqual(code, 0, err)
        self.assertIn('admitted', out)

    def test_browser_and_compile_overlap_in_reverse_order(self):
        self.hold('browser', 'a')
        self.assertEqual(self.attempt('compile', 'b')[0], 0)

    def test_same_kind_stays_serialized(self):
        for kind in ('compile', 'browser'):
            with self.subTest(kind=kind):
                child = self.hold(kind, 'a')
                self.assertNotEqual(self.attempt(kind, 'b')[0], 0)
                self.stop(child)

    def test_same_checkout_cannot_rebuild_under_browser(self):
        self.hold('browser', 'a')
        self.assertNotEqual(self.attempt('compile', 'a')[0], 0)

    def test_subdirectory_resolves_to_same_checkout(self):
        root = Path(self.tmp.name) / 'repo'
        root.mkdir()
        subprocess.run(['git', 'init', '-q', str(root)], check=True)
        (root / 'sub').mkdir()
        self.hold('browser', 'repo')
        self.assertNotEqual(self.attempt('compile', 'repo/sub')[0], 0)

    def test_legacy_exclusive_lock_blocks_new_kinds(self):
        import fcntl
        with open(Path(self.tmp.name) / 'heavy.lock', 'a+') as old:
            fcntl.flock(old, fcntl.LOCK_EX | fcntl.LOCK_NB)
            for kind in ('compile', 'browser'):
                self.assertNotEqual(self.attempt(kind, 'a')[0], 0)

    def test_shared_work_blocks_exclusive_and_old_guard(self):
        import fcntl
        self.hold('compile', 'a')
        self.assertNotEqual(self.attempt('exclusive', 'b')[0], 0)
        with open(Path(self.tmp.name) / 'heavy.lock', 'a+') as old:
            with self.assertRaises(BlockingIOError):
                fcntl.flock(old, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_cross_kind_nesting_fails_instead_of_deadlocking(self):
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard, 'busy_reason', return_value=None):
            with guard.lease(kind='browser', wait=0):
                self.assertTrue(guard.owns_lease('browser'))
                self.assertFalse(guard.owns_lease('compile'))
                with self.assertRaisesRegex(ValueError, 'cannot nest compile'):
                    with guard.lease(kind='compile', wait=0):
                        self.fail('cross-kind promotion must fail')

    def test_nested_compile_cannot_switch_checkouts_without_a_lock(self):
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard, 'busy_reason', return_value=None):
            with guard.lease(kind='compile', wait=0):
                with patch.object(guard, 'checkout_root', return_value=Path(self.tmp.name)):
                    self.assertFalse(guard.owns_lease('compile'))
                    with self.assertRaisesRegex(ValueError, 'across workloads/checkouts'):
                        with guard.lease(kind='compile', wait=0):
                            self.fail('must not mutate another unlocked checkout')

    def test_stale_metadata_cannot_hide_unguarded_compiler(self):
        import json
        (Path(self.tmp.name) / 'compile.lock').write_text(json.dumps({'pid': 400}))
        rows = '401 400 rustc foo.rs\n'
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard.sys, 'platform', 'linux'), \
                patch.object(guard, 'processes', return_value=({401: 400, 400: 1}, rows)):
            self.assertIn('401', guard.busy_reason('browser'))

    def test_active_compiler_lease_allows_browser_but_not_unrelated_compiler(self):
        child = self.hold('compile', 'a')
        rows = f'401 {child.pid} rustc foo.rs\n'
        parents = {401: child.pid, child.pid: 1}
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard.sys, 'platform', 'linux'), \
                patch.object(guard, 'processes', return_value=(parents, rows)):
            self.assertIsNone(guard.busy_reason('browser'))
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard.sys, 'platform', 'linux'), \
                patch.object(guard, 'processes', return_value=(parents, rows + '402 1 rustc other.rs\n')):
            self.assertIn('402', guard.busy_reason('browser'))

    def test_only_matching_sccache_request_is_attributed_to_compile_owner(self):
        child = self.hold('compile', 'a')
        rows = (f'401 {child.pid} sccache rustc --crate-name game --out-dir /a/target\n'
                '500 1 /opt/homebrew/bin/sccache\n'
                '501 500 /rust/bin/rustc --crate-name game --out-dir /a/target\n')
        parents = {401: child.pid, child.pid: 1, 500: 1, 501: 500, 502: 500}
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard.sys, 'platform', 'linux'), \
                patch.object(guard, 'processes', return_value=(parents, rows)):
            self.assertIsNone(guard.busy_reason('browser'))
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard.sys, 'platform', 'linux'), \
                patch.object(guard, 'processes', return_value=(parents, rows + '502 500 /rust/bin/rustc --crate-name game --out-dir /b/target\n')):
            self.assertIn('502', guard.busy_reason('browser'))

    def test_parallel_admission_requires_memory_headroom(self):
        for report in ('System-wide memory free percentage: 24%', 'unknown'):
            with self.subTest(report=report), patch.object(guard.sys, 'platform', 'darwin'), \
                    patch.object(guard.subprocess, 'check_output', side_effect=['1', report]):
                self.assertIn('headroom', guard.busy_reason('browser'))

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

    def test_cancellation_releases_compile_and_checkout_permits(self):
        code = "import time; print('ready',flush=True); time.sleep(30)"
        directory = Path(self.tmp.name) / 'a'
        directory.mkdir()
        first = self.spawn(f'import sys; sys.exit(g.run([sys.executable,"-c",{code!r}],kind="compile"))', cwd=directory)
        self.addCleanup(lambda: self.stop(first))
        self.assertEqual(first.stdout.readline().strip(), 'ready')
        first.send_signal(signal.SIGTERM)
        first.communicate(timeout=5)
        self.assertEqual(self.attempt('compile', 'b')[0], 0)
        self.assertEqual(self.attempt('browser', 'a')[0], 0)

    def test_pressure_defers_without_launching(self):
        with patch.dict(os.environ, self.env, clear=True), patch.object(guard, 'busy_reason', return_value='pressure'):
            with self.assertRaisesRegex(BlockingIOError, 'pressure'):
                with guard.lease(wait=0):
                    self.fail('must not start')

    def test_stale_owner_does_not_bypass_lock(self):
        with patch.dict(os.environ, {guard.OWNER: '99999999'}):
            self.assertFalse(guard.owns_lease())

    def test_legacy_build_detection_ignores_waiting_guard(self):
        rows = '444 1 /usr/bin/python3 tools/host_build.py -- tools/build_desktop.py\n'
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=({}, rows)):
            self.assertIsNone(guard.busy_reason())
        rows = '445 1 /usr/bin/python3 tools/build_desktop.py\n'
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=({}, rows)):
            self.assertIn('445', guard.busy_reason())

    def test_waiting_browser_parents_do_not_deadlock_each_other(self):
        rows = ('444 1 /usr/bin/node tools/run_browser_tests.mjs\n'
                '445 444 /usr/bin/python3 tools/host_build.py -- node tools/run_browser_tests.mjs\n')
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=({445: 444, 444: 1}, rows)):
            self.assertIsNone(guard.busy_reason())

    def test_truncated_comm_python_guards_do_not_deadlock_runners(self):
        # #497: macOS ps truncates comm to 16 characters (/opt/homebrew/Ce). The
        # guard must be recognised from args, so two waiting runners in different
        # worktrees do not count each other as unguarded heavy work.
        python = '/opt/homebrew/Cellar/python@3.15/3.15.0/Frameworks/Python.framework/Versions/3.15/Resources/Python.app/Contents/MacOS/Python'
        rows = (f'500 1 node tools/run_browser_tests.mjs --only test_engine_boot.mjs\n'
                f'501 500 {python} /Volumes/External SSD/wt-a/tools/host_build.py -- node tools/run_browser_tests.mjs\n'
                f'600 1 node tools/run_browser_tests.mjs --only test_engine_boot.mjs\n'
                f'601 600 {python} /Volumes/External SSD/wt-b/tools/host_build.py -- node tools/run_browser_tests.mjs\n')
        parents = {500: 1, 501: 500, 600: 1, 601: 600}
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=(parents, rows)):
            self.assertIsNone(guard.busy_reason())
        # A runner without its guard is still heavy work.
        with patch.object(guard.sys, 'platform', 'linux'), patch.object(guard, 'processes', return_value=(parents, rows.split('601 ')[0])):
            self.assertIn('600', guard.busy_reason())

    def test_process_rows_never_use_the_truncated_comm_column(self):
        calls = []
        def fake(command, text):
            calls.append(command)
            return '1 0 /sbin/launchd\n' if command[-1].endswith('args=') else '1 0\n'
        with patch.object(guard.subprocess, 'check_output', side_effect=fake):
            guard.processes()
        self.assertTrue(all('comm' not in command[-1] for command in calls))

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
            '<plist version="1.0"><dict><key>Label</key><string>test</string>'
            '<key>ProgramArguments</key><array><string>@PYTHON@</string></array></dict></plist>')
        (home / 'Library/LaunchAgents').mkdir(parents=True)
        legacy = home / 'LargeFiles/fury-desktop'
        legacy.mkdir(parents=True)
        (legacy / 'watcher-state.json').write_text(json.dumps({'done': {'preserved': {}}, 'failed': {}}))
        with patch.object(install, 'SERVICE', service), patch.object(install.Path, 'home', return_value=home), \
             patch.object(install, 'running_watcher', side_effect=[123, None]), patch.object(install, 'launch_python', return_value=sys.executable), \
             patch.object(install.subprocess, 'run') as run, \
             patch.object(install.time, 'sleep', side_effect=lambda _: run.assert_not_called()):
            install.finish_handoff(home / 'repo')
        config = plistlib.loads((home / 'Library/LaunchAgents' / (install.LABEL + '.plist')).read_bytes())
        self.assertEqual(config['ProgramArguments'][0], sys.executable)
        self.assertIn('preserved', json.loads((service / 'watcher-state.json').read_text())['done'])
        commands = [call.args[0][1] for call in run.call_args_list]
        self.assertEqual(commands, ['bootout', 'enable', 'bootstrap'])
        self.assertEqual(json.loads((service / 'handoff.json').read_text())['status'], 'installed')

    def test_guard_only_install_does_not_touch_watcher(self):
        args = ['install_host_build_budget.py', '--repo', self.tmp.name, '--guard-only', '--apply']
        with patch.object(sys, 'argv', args), patch.object(install, 'cargo_install') as cargo, \
                patch.object(install.subprocess, 'run', side_effect=AssertionError('must not touch launchd')):
            install.main()
        cargo.assert_called_once_with(install.ROOT / 'tools')

    def test_launch_interpreter_uses_actual_mac_executable(self):
        actual = Path(self.tmp.name) / 'Python.app/Contents/MacOS/Python'
        actual.parent.mkdir(parents=True)
        actual.touch()
        with patch.object(install.sys, 'platform', 'darwin'), \
             patch.object(install.subprocess, 'check_output', return_value=str(actual) + '\n'):
            self.assertEqual(install.launch_python(), str(actual))

    def test_launch_interpreter_fails_closed_if_process_identity_is_missing(self):
        with patch.object(install.sys, 'platform', 'darwin'), \
             patch.object(install.subprocess, 'check_output', return_value='missing'):
            with self.assertRaises(ValueError):
                install.launch_python()

    def test_non_mac_interpreter_is_unchanged(self):
        with patch.object(install.sys, 'platform', 'linux'):
            self.assertEqual(install.launch_python(), sys.executable)

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
            with patch.object(watcher.sys, 'platform', 'darwin'), patch.object(watcher, 'probe_tools'), \
                 patch.object(watcher.subprocess, 'check_output', return_value=data), \
                 patch.object(watcher, 'OUT_ROOT', root / 'out'), patch.object(watcher, 'WORKTREES', root / 'trees'):
                watcher.preflight(root)
            bad = plistlib.dumps({'MountPoint': '/Volumes/External SSD', 'VolumeUUID': 'wrong'})
            with patch.object(watcher.sys, 'platform', 'darwin'), patch.object(watcher.subprocess, 'check_output', return_value=bad):
                with self.assertRaises(OSError):
                    watcher.preflight(root)

    def test_storage_preflight_reports_a_tool_blocked_on_the_ssd(self):
        hung = watcher.subprocess.TimeoutExpired(['cargo', '--version'], 60)
        with patch.object(watcher.subprocess, 'run', side_effect=hung) as run, \
                self.assertRaisesRegex(OSError, 'Cargo did not start'):
            watcher.probe_tools()
        self.assertEqual(run.call_args.kwargs['timeout'], watcher.TOOL_PROBE_SECONDS)
        self.assertEqual(run.call_args.kwargs['cwd'], watcher.WORKTREES)

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
