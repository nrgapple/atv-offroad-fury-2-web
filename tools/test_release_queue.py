"""Queue dependency isolation, merging and real-Git integration checks."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from package_content import SOURCE_SHA
from release_resources import (cached_bytes, copy_content, integrate, merge_devlog, merge_manifest,
                               snapshot, verify_snapshot)


def content(root, records=None):
    root.mkdir(parents=True, exist_ok=True)
    records = records or {'/STARTAPP.RES': b'first'}
    files = []
    for name, data in records.items():
        digest = hashlib.sha256(data).hexdigest()
        (root / f'{digest}.bin').write_bytes(data)
        files.append({'path': name, 'sha256': digest, 'bytes': len(data), 'url': f'/content/{digest}.bin'})
    manifest = {'version': 1, 'source_build_sha256': SOURCE_SHA, 'files': files}
    (root / 'manifest.json').write_text(json.dumps(manifest))
    return manifest


def git(root, *args):
    return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()


class QueueResources(unittest.TestCase):
    def test_cache_deduplicates_snapshots_without_linking_mutable_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            data = b'approved dependency bytes'
            first, second = root / 'first.bin', root / 'second.bin'
            cached_bytes(data, first, root / 'objects', '.bin')
            cached_bytes(data, second, root / 'objects', '.bin')
            self.assertEqual(first.stat().st_ino, second.stat().st_ino)
            self.assertEqual(len(list((root / 'objects').iterdir())), 1)
            self.assertEqual(first.stat().st_mode & 0o222, 0)
            second.unlink()
            cached_bytes(b'new dependency', second, root / 'objects', '.bin')
            self.assertEqual(first.read_bytes(), data)
            self.assertNotEqual(first.stat().st_ino, second.stat().st_ino)

    def test_parallel_resource_changes_preserve_both_and_conflicts_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            base = content(root / 'base', {'/STARTAPP.RES': b'a', '/OVERLAYS.RES': b'b'})
            current = content(root / 'current', {'/STARTAPP.RES': b'c', '/OVERLAYS.RES': b'b'})
            proposed = content(root / 'proposed', {'/STARTAPP.RES': b'a', '/OVERLAYS.RES': b'd'})
            merged = merge_manifest(base, current, proposed)
            self.assertEqual(merged['files'][0], current['files'][0])
            self.assertEqual(merged['files'][1], proposed['files'][1])
            conflicting = content(root / 'conflict', {'/STARTAPP.RES': b'e', '/OVERLAYS.RES': b'b'})
            with self.assertRaisesRegex(ValueError, 'conflicting queued changes'):
                merge_manifest(base, current, conflicting)

    def test_independent_devlog_entries_merge_and_same_entry_conflicts(self):
        base = {'entries': [{'id': 'old', 'title': 'old'}], 'updated_at': '1'}
        current = {**base, 'entries': base['entries'] + [{'id': 'first'}], 'updated_at': '2'}
        proposed = {**base, 'entries': base['entries'] + [{'id': 'second'}], 'updated_at': '3'}
        self.assertEqual([e['id'] for e in merge_devlog(base, current, proposed)['entries']], ['old', 'first', 'second'])
        with self.assertRaises(ValueError):
            merge_devlog(base, {'entries': [{'id': 'old', 'title': 'a'}]}, {'entries': [{'id': 'old', 'title': 'b'}]})

    def test_snapshot_pins_bytes_rejects_tampering_and_copies_only_allowlist(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            queue, job, source = root / 'queue', root / 'job', root / 'source'
            source.mkdir(); job.mkdir()
            content(queue / 'published/content')
            (queue / 'published/images').mkdir()
            (queue / 'published/content/secret.iso').write_bytes(b'never copied')
            log = {'entries': []}
            with patch('release_resources.read_devlog') as mocked:
                mocked.return_value = log
                snapshot(source, 'a' * 40, job, queue, queue / 'published/content', source)
            verify_snapshot(job)
            self.assertFalse((job / 'resources/content/secret.iso').exists())
            (queue / 'published/content/manifest.json').write_text('changed source')
            verify_snapshot(job)
            (job / 'resources/content/manifest.json').write_text('changed queued bytes')
            with self.assertRaisesRegex(ValueError, 'changed after submission'):
                verify_snapshot(job)

    def test_new_screenshot_requires_review_and_old_url_cannot_mutate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            content(root / 'queue/published/content')
            (root / 'queue/published/images').mkdir()
            (root / 'images').mkdir()
            data = b'\x89PNG\r\n\x1a\nexample'
            (root / 'images/new.png').write_bytes(data)
            log = {'entries': [{'id': 'new', 'image': '/devlog-images/new.png'}]}
            with patch('release_resources.read_devlog') as mocked:
                mocked.return_value = log
                with self.assertRaisesRegex(ValueError, 'inspect new/changed'):
                    snapshot(root, 'a' * 40, root / 'job1', root / 'queue', root / 'queue/published/content', root / 'images')
                snapshot(root, 'a' * 40, root / 'job2', root / 'queue', root / 'queue/published/content', root / 'images', True)
                (root / 'queue/published/images/new.png').write_bytes(data + b'old')
                with self.assertRaisesRegex(ValueError, 'immutable'):
                    snapshot(root, 'a' * 40, root / 'job3', root / 'queue', root / 'queue/published/content', root / 'images', True)

    def test_actual_git_merges_two_worktrees_and_aborts_code_conflict(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            git(root, 'init', '-b', 'main'); git(root, 'config', 'user.email', 'queue@example.test'); git(root, 'config', 'user.name', 'Queue test')
            (root / 'research').mkdir()
            log = {'entries': [], 'updated_at': '0'}
            (root / 'research/devlog.json').write_text(json.dumps(log))
            (root / 'code.txt').write_text('base\n')
            git(root, 'add', '.'); git(root, 'commit', '-m', 'base')
            base = git(root, 'rev-parse', 'HEAD')
            git(root, 'checkout', '-b', 'first')
            (root / 'research/devlog.json').write_text(json.dumps({'entries': [{'id': 'first'}], 'updated_at': '1'}))
            (root / 'code.txt').write_text('first\n')
            git(root, 'commit', '-am', 'first'); first = git(root, 'rev-parse', 'HEAD')
            git(root, 'checkout', '-b', 'second', base)
            (root / 'research/devlog.json').write_text(json.dumps({'entries': [{'id': 'second'}], 'updated_at': '2'}))
            git(root, 'commit', '-am', 'second'); second = git(root, 'rev-parse', 'HEAD')
            git(root, 'checkout', 'main')
            previous = Path.cwd()
            try:
                os.chdir(root)
                integrate(first); integrate(second)
                self.assertEqual([e['id'] for e in json.loads((root / 'research/devlog.json').read_text())['entries']], ['first', 'second'])
                git(root, 'checkout', '-b', 'conflict', base)
                (root / 'code.txt').write_text('conflict\n'); git(root, 'commit', '-am', 'conflict')
                conflict = git(root, 'rev-parse', 'HEAD'); git(root, 'checkout', 'main')
                with self.assertRaisesRegex(ValueError, 'code merge conflict'):
                    integrate(conflict)
                self.assertFalse((root / '.git/MERGE_HEAD').exists())
                self.assertEqual((root / 'code.txt').read_text(), 'first\n')
            finally:
                os.chdir(previous)

@unittest.skipUnless(os.environ.get('ATV_RELEASE_TEST_BIN'), 'host-only compiled Rust CLI integration')
class RustQueueIntegration(unittest.TestCase):
    def test_concurrent_worktree_submission_duplicate_and_worker_gate_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            owner, remote, queue = root / 'owner', root / 'remote.git', root / 'queue'
            owner.mkdir()
            git(root, 'init', '--bare', str(remote))
            git(owner, 'init', '-b', 'main')
            git(owner, 'config', 'user.email', 'queue@example.test'); git(owner, 'config', 'user.name', 'Queue test')
            (owner / 'research').mkdir(); (owner / 'tools').mkdir()
            (owner / '.gitignore').write_text('/private/\n/work/\n/target/\n')
            (owner / 'research/devlog.json').write_text(json.dumps({'entries': [], 'updated_at': '0'}))
            for helper in ('release_resources.py', 'package_content.py', 'devlog.py'):
                (owner / 'tools' / helper).write_text((Path(__file__).parent / helper).read_text())
            (owner / 'tools/verify_repository.py').write_text("print('fixture hygiene passes')\n")
            (owner / 'Makefile').write_text('check:\n\tfalse\n')
            git(owner, 'add', '.'); git(owner, 'commit', '-m', 'baseline')
            git(owner, 'remote', 'add', 'origin', str(remote)); git(owner, 'push', 'origin', 'main')
            (queue / 'jobs').mkdir(parents=True)
            content(queue / 'published/content'); (queue / 'published/images').mkdir()
            config = {'owner': str(owner), 'queue': str(queue),
                      'controller': str(Path(__file__).parent.resolve() / 'release_resources.py'),
                      'target': str(root / 'target'), 'playwright': str(root), 'url': 'https://example.invalid'}
            (owner / '.git/atv-release.json').write_text(json.dumps(config))
            worktrees = []
            for name in ['one', 'two']:
                worktree = root / name
                git(owner, 'worktree', 'add', '-b', name, str(worktree), 'main')
                (worktree / f'{name}.txt').write_text(name)
                (worktree / 'research/devlog.json').write_text(json.dumps({'entries': [{'id': name}], 'updated_at': name}))
                git(worktree, 'add', '.'); git(worktree, 'commit', '-m', name)
                worktrees.append(worktree)
            binary = os.environ['ATV_RELEASE_TEST_BIN']
            processes = [subprocess.Popen([binary, 'submit'], cwd=path, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for path in worktrees]
            submitted = []
            for process in processes:
                stdout, stderr = process.communicate(timeout=30)
                self.assertEqual(process.returncode, 0, stderr)
                submitted.append(json.loads(stdout))
            self.assertEqual(len(list((queue / 'jobs').glob('*/job.json'))), 2)
            duplicate = subprocess.run([binary, 'submit'], cwd=worktrees[0], check=True, capture_output=True, text=True)
            self.assertEqual(json.loads(duplicate.stdout)['id'], submitted[0]['id'])
            # Later branch edits cannot enter the pinned job.
            (worktrees[0] / 'one.txt').write_text('later uncommitted work')
            dirty = subprocess.run([binary, 'submit'], cwd=worktrees[0], capture_output=True, text=True)
            self.assertNotEqual(dirty.returncode, 0)
            subprocess.run([binary, 'worker', '--once'], cwd=owner, check=True, capture_output=True, text=True, timeout=60)
            receipts = [json.loads(path.read_text()) for path in (queue / 'jobs').glob('*/job.json')]
            self.assertEqual({(j['status'], j['stage']) for j in receipts}, {('failed', 'checking')})
            self.assertEqual(git(owner, 'rev-parse', 'main'), git(owner, 'rev-parse', 'origin/main'))
            self.assertEqual((worktrees[0] / 'one.txt').read_text(), 'later uncommitted work')
            self.assertFalse(any('wrangler' in path.read_text() for path in (queue / 'jobs').glob('*/worker.log')))
            first = next(j for j in receipts if j['source'] == submitted[0]['source'])
            first['status'] = 'failed'; first['stage'] = 'verifying'
            first_path = queue / 'jobs' / first['id'] / 'job.json'
            first_path.write_text(json.dumps(first))
            unrelated = subprocess.run([binary, 'submit', '--supersede', first['id']], cwd=worktrees[1], capture_output=True, text=True)
            self.assertNotEqual(unrelated.returncode, 0, 'repair must include old integrated commit')
            git(worktrees[0], 'restore', 'one.txt')
            git(worktrees[0], 'merge', '--no-ff', first['integrated'], '-m', 'Include failed integrated candidate')
            (worktrees[0] / 'one.txt').write_text('committed repair')
            git(worktrees[0], 'commit', '-am', 'repair')
            repaired = subprocess.run([binary, 'submit', '--supersede', first['id']], cwd=worktrees[0], check=True, capture_output=True, text=True)
            repaired = json.loads(repaired.stdout)
            self.assertEqual(repaired['supersedes'], first['id'])
            self.assertEqual(json.loads(first_path.read_text())['status'], 'superseded')
            repeated = subprocess.run([binary, 'submit', '--supersede', first['id']], cwd=worktrees[0], check=True, capture_output=True, text=True)
            self.assertEqual(json.loads(repeated.stdout)['id'], repaired['id'])


if __name__ == '__main__':
    unittest.main()
