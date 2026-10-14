"""Compatibility failures must trigger locked installation, never stale tooling."""
import hashlib
import json
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch

import worker_build_cache as cache

STAMP = {'schema': 1, 'worker_build': '0.8.7', 'os': 'Linux', 'arch': 'X64',
         'rustc': 'rustc 1.99.0\nhost: x86_64-unknown-linux-gnu\ncommit-hash: exact', 'cargo': 'cargo 1.99.0'}
RUN = subprocess.run


def fixture(root, version='0.8.7'):
    binary = root / 'bin/worker-build'
    binary.parent.mkdir(parents=True, exist_ok=True)
    binary.write_text(f'#!/bin/sh\nprintf \'{version}\\n\'\n')
    binary.chmod(0o755)
    (root / 'identity.json').write_text(json.dumps({'identity': STAMP, 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}))


class WorkerBuildCacheTests(unittest.TestCase):
    def test_key_changes_for_toolchain_os_arch_and_tool_version(self):
        base = cache.cache_key(STAMP)
        for field in ['rustc', 'cargo', 'os', 'arch', 'worker_build', 'schema']:
            changed = dict(STAMP); changed[field] = str(STAMP[field]) + '-different'
            self.assertNotEqual(cache.cache_key(changed), base, field)
        self.assertEqual(cache.cache_key(dict(reversed(list(STAMP.items())))), base)

    def test_exact_valid_hit_reuses_tool_and_never_calls_installer(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); fixture(root)
            def run(args, **kwargs):
                if args[0] == 'cargo': raise AssertionError('installer must not run')
                return RUN(args, **kwargs)
            with patch.object(cache.subprocess, 'run', side_effect=run):
                self.assertEqual(cache.prepare(root, STAMP, exact_hit=True)['source'], 'cache')

    def cold(self, root, exact_hit=True):
        calls = []
        def install(args, **kwargs):
            if args[0] != 'cargo': return RUN(args, **kwargs)
            calls.append((args, kwargs)); fixture(root)
            return subprocess.CompletedProcess(args, 0)
        with patch.object(cache.subprocess, 'run', side_effect=install):
            result = cache.prepare(root, STAMP, exact_hit=exact_hit)
        self.assertEqual(result['source'], 'cold'); self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0], ['cargo', 'install', 'worker-build', '--version', '0.8.7', '--locked', '--force', '--root', str(root)])
        self.assertEqual(Path(calls[0][1]['env']['CARGO_TARGET_DIR']).parent, root.parent)
        self.assertTrue(cache.valid(root, STAMP))

    def test_miss_and_partial_hit_ignore_even_a_valid_existing_binary(self):
        for existing in [False, True]:
            with self.subTest(existing=existing), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                if existing: fixture(root)
                self.cold(root, exact_hit=False)

    def test_corrupt_bytes_and_wrong_identity_do_not_execute_cached_binary(self):
        for failure in ['bytes', 'identity', 'non-object', 'invalid-json']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); fixture(root)
                record = root / 'identity.json'
                if failure == 'bytes': (root / 'bin/worker-build').write_text('do not execute')
                elif failure == 'identity':
                    data = json.loads(record.read_text()); data['identity']['rustc'] += '-old'; record.write_text(json.dumps(data))
                else: record.write_text('[]' if failure == 'non-object' else 'bad json')
                with patch.object(cache, 'command', side_effect=AssertionError('bad cache must not execute')):
                    self.assertFalse(cache.valid(root, STAMP))
                self.cold(root)

    def test_wrong_binary_version_and_missing_permissions_fall_back_to_cold(self):
        for failure in ['version', 'permissions']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); fixture(root, '0.8.6' if failure == 'version' else '0.8.7')
                if failure == 'permissions': (root / 'bin/worker-build').chmod(0o644)
                self.cold(root)

    def test_symlinked_binary_is_replaced_without_modifying_its_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'tool'; fixture(root)
            target = Path(temporary) / 'outside'; target.write_text('preserve')
            binary = root / 'bin/worker-build'; binary.unlink(); binary.symlink_to(target)
            self.cold(root)
            self.assertEqual(target.read_text(), 'preserve')

    def test_failed_install_leaves_no_valid_record(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); fixture(root)
            with patch.object(cache.subprocess, 'run', side_effect=RuntimeError('install failed')):
                with self.assertRaises(RuntimeError): cache.prepare(root, STAMP, exact_hit=False)
            self.assertFalse((root / 'identity.json').exists())

    def test_cache_paths_and_save_scope_exclude_cargo_home_credentials_and_game_outputs(self):
        workflow = (Path(__file__).resolve().parents[1] / '.github/workflows/ci.yml').read_text()
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        self.assertIn("github.event_name == 'push' || github.event_name == 'workflow_dispatch'", workflow)
        self.assertEqual(workflow.count('${{ runner.temp }}/fury-worker-build/bin/worker-build'), 2)
        self.assertEqual(workflow.count('${{ runner.temp }}/fury-worker-build/identity.json'), 2)
        worker_restore = workflow.split('      - name: Restore pinned deployment tool')[1].split('      - name:')[0]
        self.assertNotIn('restore-keys:', worker_restore)
        for path in ['~/.cargo', '/private', '/target', '.wrangler']:
            self.assertNotIn('            ' + path + '\n', workflow)
