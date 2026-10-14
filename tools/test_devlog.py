"""Parallel checkpoint recording, normal Git merges and pinned release metadata."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import tempfile
from threading import Thread
import unittest
from unittest.mock import patch
from urllib.request import Request, urlopen
from urllib.error import HTTPError

from devlog import BASE, ENTRIES, read_devlog
import package_release
import release_resources
import serve_web
from test_release_queue import content, git
from update_devlog import main as record


def baseline(root):
    path = root / BASE
    path.parent.mkdir(parents=True)
    log = {'status': 'Historical', 'working_on': 'Port', 'updated_at': '2026-10-01T00:00:00Z',
           'port_status': {'features': []}, 'entries': [{'id': 'historical', 'title': 'History'}]}
    path.write_text(json.dumps(log))
    return log


def checkpoint(root, identifier, recorded='2026-10-13T10:00:00Z', **fields):
    entry = {'id': identifier, 'recorded_at': recorded, 'timestamp': '2026-10-02T00:00:00Z',
             'title': identifier, 'summary': 'Test', 'kind': 'verification', **fields}
    path = root / ENTRIES / (identifier + '.json')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(entry))
    return entry


class DevlogTests(unittest.TestCase):
    def test_history_remains_identical_without_new_entries(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            expected = baseline(root)
            self.assertEqual(read_devlog(root), expected)

    def test_parallel_recorders_never_rewrite_shared_history_or_drop_entries(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline(root)
            original = (root / BASE).read_bytes()
            def write(index):
                record(['--title', 'Same title', '--summary', str(index), '--status', str(index)], root=root)
            with patch('builtins.print'), ThreadPoolExecutor(max_workers=8) as pool:
                list(pool.map(write, range(24)))
            log = read_devlog(root)
            self.assertEqual((root / BASE).read_bytes(), original)
            self.assertEqual(len(log['entries']), 25)
            self.assertEqual(len(list((root / ENTRIES).glob('*.json'))), 24)
            self.assertEqual({entry['summary'] for entry in log['entries'][1:]}, {str(i) for i in range(24)})
            self.assertEqual(log['status'], log['entries'][-1]['summary'])
            self.assertFalse(list((root / ENTRIES).glob('*.tmp')))

    def test_order_and_status_use_recording_time_with_timezone_and_tie_breaker(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline(root)
            checkpoint(root, 'b', '2026-10-13T11:00:00+01:00', status='Latest')
            checkpoint(root, 'a', '2026-10-13T10:00:00Z', status='Earlier', working_on='Maps')
            checkpoint(root, 'z', '2026-10-12T20:00:00-04:00', working_on='Old')
            log = read_devlog(root)
            self.assertEqual([entry['id'] for entry in log['entries']], ['historical', 'z', 'a', 'b'])
            self.assertEqual((log['status'], log['working_on']), ('Latest', 'Maps'))
            self.assertTrue(all('status' not in entry and 'working_on' not in entry for entry in log['entries']))
            self.assertEqual(log['port_status'], {'features': []})

    def test_rejects_duplicate_ids_filename_mismatch_and_timezone_free_dates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline(root)
            path = root / ENTRIES / 'historical.json'
            checkpoint(root, 'historical')
            with self.assertRaisesRegex(ValueError, 'duplicate checkpoint'):
                read_devlog(root)
            path.unlink()
            checkpoint(root, 'wrong')
            (root / ENTRIES / 'wrong.json').rename(root / ENTRIES / 'other.json')
            with self.assertRaisesRegex(ValueError, 'filename must match'):
                read_devlog(root)
            (root / ENTRIES / 'other.json').unlink()
            checkpoint(root, 'bad', '2026-10-13T10:00:00')
            with self.assertRaisesRegex(ValueError, 'timezone'):
                read_devlog(root)

    def test_two_branches_merge_normally_and_committed_reads_ignore_dirty_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            git(root, 'init', '-b', 'main')
            git(root, 'config', 'user.name', 'Devlog test')
            git(root, 'config', 'user.email', 'devlog@example.test')
            history = baseline(root)
            git(root, 'add', '.'); git(root, 'commit', '-m', 'history')
            base = git(root, 'rev-parse', 'HEAD')
            self.assertEqual(read_devlog(root, base), history)
            for branch in ('first', 'second'):
                git(root, 'checkout', '-b', branch, base)
                checkpoint(root, branch, status=branch, working_on=branch)
                git(root, 'add', '.'); git(root, 'commit', '-m', branch)
            git(root, 'checkout', 'main')
            git(root, 'merge', '--no-ff', 'first', '-m', 'first')
            git(root, 'merge', '--no-ff', 'second', '-m', 'second')
            commit = git(root, 'rev-parse', 'HEAD')
            expected = read_devlog(root, commit)
            self.assertEqual([entry['id'] for entry in expected['entries']], ['historical', 'first', 'second'])
            (root / BASE).write_text('dirty invalid JSON')
            checkpoint(root, 'third')
            (root / ENTRIES / 'first.json').write_text('dirty invalid JSON')
            self.assertEqual(read_devlog(root, commit), expected)
            self.assertEqual(json.loads(git(root, 'show', f'{commit}:{BASE}')), history)
            # Private release snapshots must discover images in committed entry files.
            git(root, 'restore', '.')
            (root / ENTRIES / 'third.json').unlink()
            checkpoint(root, 'image', image='/devlog-images/image.png')
            git(root, 'add', '.'); git(root, 'commit', '-m', 'image')
            content(root / 'queue/published/content')
            (root / 'queue/published/images').mkdir()
            (root / 'images').mkdir()
            (root / 'images/image.png').write_bytes(b'\x89PNG\r\n\x1a\nfixture')
            release_resources.snapshot(root, git(root, 'rev-parse', 'HEAD'), root / 'job', root / 'queue',
                                       root / 'queue/published/content', root / 'images', reviewed=True)
            release_resources.verify_snapshot(root / 'job')
            self.assertTrue((root / 'job/resources/images/image.png').is_file())

    def test_release_packages_combined_text_and_excludes_screenshot_references(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline(root)
            checkpoint(root, 'new', image='/devlog-images/new.png', caption='Private pixels')
            for name in package_release.STATIC_FILES:
                path = root / 'web/dist' / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'\0asmfixture' if name.endswith('.wasm') else b'authored')
            for name in package_release.RELAY_FILES:
                path = root / 'crates/atv-relay/build' / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'\0asmfixture' if name.endswith('.wasm') else b'authored')
            with patch.object(package_release, 'ROOT', root):
                package_release.package('a' * 40, root / 'work/release')
            log = json.loads((root / 'work/release/static/devlog.json').read_text())
            self.assertEqual([entry['id'] for entry in log['entries']], ['historical', 'new'])
            self.assertNotIn('image', log['entries'][-1])
            self.assertNotIn('caption', log['entries'][-1])
            self.assertFalse(any(path.name.endswith('.png') and path.name == 'new.png' for path in (root / 'work/release').rglob('*')))

    def test_live_http_log_aggregates_and_head_matches_get_without_exposing_entries(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline(root)
            checkpoint(root, 'served')
            with patch.object(serve_web, 'PROJECT', root), patch.object(serve_web, 'ROOT', root):
                server = serve_web.StaticServer(('127.0.0.1', 0), serve_web.Handler)
                thread = Thread(target=server.serve_forever, daemon=True)
                thread.start()
                try:
                    url = f'http://127.0.0.1:{server.server_port}/devlog.json'
                    with urlopen(url) as response:
                        data = response.read()
                    self.assertEqual(json.loads(data), read_devlog(root))
                    with urlopen(Request(url, method='HEAD')) as response:
                        self.assertEqual(int(response.headers['Content-Length']), len(data))
                        self.assertEqual(response.read(), b'')
                    with self.assertRaises(HTTPError) as caught:
                        urlopen(url.replace('/devlog.json', f'/{ENTRIES}/served.json'))
                    self.assertEqual(caught.exception.code, 404)
                finally:
                    server.shutdown(); server.server_close(); thread.join()


if __name__ == '__main__':
    unittest.main()
