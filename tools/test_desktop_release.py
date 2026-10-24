"""Pack builder, archive allowlist and publish guards for the desktop release flow.

Everything uses synthetic data; no game content is read.
"""
import copy
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import desktop_archive
import pack_content
import desktop_release_watcher as watcher
import publish_desktop_release as publish

ROOT = Path(__file__).resolve().parents[1]
FIXTURE_HEX = ROOT / 'crates/atv-content/tests/fixtures/mini.pack.hex'
_FIXTURE_DIR = tempfile.TemporaryDirectory()
FIXTURE = Path(_FIXTURE_DIR.name) / 'mini.pack'
FIXTURE.write_bytes(bytes.fromhex(FIXTURE_HEX.read_text()))


def synthetic_package(directory):
    """A tiny hosted package; returns its manifest dict."""
    sha = lambda data: hashlib.sha256(data).hexdigest()
    one, two = b'one-one', b'two-two-two'
    for data in (one, two):
        (directory / f'{sha(data)}.bin').write_bytes(data)
    manifest = {
        'version': 1, 'source_build_sha256': pack_content.SOURCE_SHA,
        'files': [
            {'path': '/A/ONE.RES', 'bytes': len(one), 'sha256': sha(one), 'url': 'x'},
            {'path': '/A/TWO.LSI', 'bytes': len(two), 'sha256': sha(two), 'lazy': True},
        ],
    }
    (directory / 'manifest.json').write_text(json.dumps(manifest))
    return manifest


class PackTests(unittest.TestCase):
    def test_committed_fixture_is_reproducible(self):
        with tempfile.TemporaryDirectory() as tmp:
            rebuilt = Path(tmp) / 'mini.pack'
            pack_content.write_fixture(rebuilt)
            self.assertEqual(rebuilt.read_bytes(), FIXTURE.read_bytes())
            text = Path(tmp) / 'mini.hex'
            self.assertEqual(pack_content.main(['fixture', '--hex', str(text)]), 0)
            self.assertEqual(text.read_text(), FIXTURE_HEX.read_text())
        self.assertEqual(pack_content.verify(FIXTURE)[0], 3)

    def test_only_authorized_runtime_files_are_accepted(self):
        base = pack_content.read_pack(FIXTURE)[0]
        manifest = json.loads(base)
        for path in ('/SCUS_972.11', '/GAME.ISO', '/BIOS/scph.bin', '/A/B.ELF', '/A/../B.RES', '/a b.RES', '/DATA.ZIP',
                     '/A/B.PNG', 'A/B.RES'):
            bad = copy.deepcopy(manifest)
            bad['files'][0]['path'] = path
            with self.assertRaises(pack_content.PackError, msg=path):
                pack_content.authorized_blobs(bad)
        foreign = dict(manifest, source_build_sha256='0' * 64)
        with self.assertRaises(pack_content.PackError):
            pack_content.authorized_blobs(foreign)
        gap = copy.deepcopy(manifest)
        gap['files'][1]['chunks'][1]['offset'] = 99
        with self.assertRaises(pack_content.PackError):
            pack_content.authorized_blobs(gap)

    def test_build_from_a_package_directory_and_detect_damage(self):
        with tempfile.TemporaryDirectory() as tmp:
            source, out = Path(tmp) / 'src', Path(tmp) / 'out'
            source.mkdir()
            out.mkdir()
            synthetic_package(source)
            pack = pack_content.build(source, out / 'p.pack')
            self.assertEqual(pack_content.verify(pack)[0], 2)
            # a damaged package chunk is refused while packing
            first = next(source.glob('*.bin'))
            first.write_bytes(b'x' * first.stat().st_size)
            with self.assertRaises(pack_content.PackError):
                pack_content.build(source, out / 'q.pack')
            self.assertFalse((out / 'q.pack').exists() or (out / 'q.pack.partial').exists())
            # a flipped byte in a finished pack is caught by verify
            data = bytearray(pack.read_bytes())
            data[-1] ^= 1
            pack.write_bytes(bytes(data))
            with self.assertRaises(pack_content.PackError):
                pack_content.verify(pack)

    def test_soundtrack_streams_are_packed_by_default(self):
        with tempfile.TemporaryDirectory() as tmp:
            source, out = Path(tmp) / 'src', Path(tmp) / 'out'
            source.mkdir()
            out.mkdir()
            manifest = synthetic_package(source)
            song = b'synthetic m4a'
            digest = hashlib.sha256(song).hexdigest()
            (source / f'{digest}.m4a').write_bytes(song)
            manifest['files'][0]['stream'] = {'url': f'/content/{digest}.m4a', 'bytes': len(song),
                                              'sha256': digest, 'type': 'audio/mp4', 'seconds': 1.0}
            (source / 'manifest.json').write_text(json.dumps(manifest))
            pack = pack_content.build(source, out / 'p.pack')
            self.assertEqual(pack_content.verify(pack)[0], 3)
            self.assertEqual(pack_content.stream_count(pack), (1, 1))
            bare = pack_content.build(source, out / 'bare.pack', include_streams=False)
            self.assertEqual(pack_content.stream_count(bare), (0, 1))
            self.assertEqual(pack_content.verify(bare)[0], 2)
            # a damaged song is refused while packing
            (source / f'{digest}.m4a').write_bytes(b'x' * len(song))
            with self.assertRaises(pack_content.PackError):
                pack_content.build(source, out / 'q.pack')

    def test_refuses_to_write_content_into_the_repository(self):
        with self.assertRaises(pack_content.PackError):
            pack_content.refuse_tracked_destination(ROOT / 'tools/not-ignored.pack')
        pack_content.refuse_tracked_destination(ROOT / 'private/reports/x.pack')
        pack_content.refuse_tracked_destination(Path(tempfile.gettempdir()) / 'x.pack')

    def test_attach_and_locate_a_pack_in_an_executable(self):
        with tempfile.TemporaryDirectory() as tmp:
            exe, out = Path(tmp) / 'fury', Path(tmp) / 'fury-packed'
            exe.write_bytes(b'MZ' + b'\0' * 100)
            pack_content.attach_pack(exe, FIXTURE, out)
            base, length = pack_content.attached_region(out)
            self.assertEqual((base, length), (102, FIXTURE.stat().st_size))
            self.assertEqual(pack_content.verify(out, base, length)[0], 3)
            with self.assertRaises(pack_content.PackError):
                pack_content.attach_pack(out, FIXTURE, Path(tmp) / 'twice')
            self.assertIsNone(pack_content.attached_region(exe))


def windows_archive(path, extra=None, drop=None, tamper=False):
    with tempfile.TemporaryDirectory() as tmp:
        exe = Path(tmp) / 'fury.exe'
        exe.write_bytes(b'MZ' + b'\0' * 64)
        packed = Path(tmp) / 'packed'
        pack_content.attach_pack(exe, FIXTURE, packed)
        data = bytearray(packed.read_bytes())
        if tamper:
            data[-17] ^= 1
        top = 'fury-remastered-0.1.0-abc1234-windows-x86_64'
        files = {f'{top}/fury.exe': bytes(data), f'{top}/README.txt': b'readme'}
        files.update({f'{top}/{k}': v for k, v in (extra or {}).items()})
        for name in drop or ():
            files.pop(f'{top}/{name}')
        with zipfile.ZipFile(path, 'w') as z:
            for name, content in files.items():
                z.writestr(name, content)
    return path


class ArchiveTests(unittest.TestCase):
    def check(self, **kwargs):
        with tempfile.TemporaryDirectory() as tmp:
            archive = windows_archive(Path(tmp) / 'fury-remastered-0.1.0-abc1234-windows-x86_64.zip', **kwargs)
            return desktop_archive.check_contents(archive, scratch=tmp)

    def test_valid_archive_passes_with_its_content_counted(self):
        self.assertEqual(self.check()[:2], ('windows-x86_64', 3))

    def test_foreign_members_are_refused(self):
        for name in ('SCUS_972.11', 'game.iso', 'private/reports/x.json', 'bios.bin', 'notes.txt', '.pi-web/x'):
            with self.assertRaises(desktop_archive.ArchiveError, msg=name):
                self.check(extra={name: b'x'})

    def test_missing_executable_or_tampered_pack_is_refused(self):
        with self.assertRaises(desktop_archive.ArchiveError):
            self.check(drop=['fury.exe'])
        with self.assertRaises(desktop_archive.ArchiveError):
            self.check(tamper=True)

    def test_linux_tarball_layout(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'fury-remastered-0.1.0-abc1234-linux-x86_64.tar.gz'
            with tarfile.open(path, 'w:gz') as t:
                for name in ('fury', 'README.txt'):
                    info = tarfile.TarInfo(f'top/{name}')
                    info.size = 3
                    t.addfile(info, io.BytesIO(b'ELF'))
            self.assertEqual(desktop_archive.check_layout(path)[0], 'linux-x86_64')


class PublishTests(unittest.TestCase):
    def test_refuses_in_ci(self):
        for env in ({'GITHUB_ACTIONS': 'true'}, {'CI': '1'}):
            with self.assertRaises(publish.PublishError):
                publish.refuse_in_ci(env)
        publish.refuse_in_ci({})
        with patch.dict('os.environ', {'GITHUB_ACTIONS': 'true'}):
            self.assertEqual(publish.main(['--dry-run']), 1)

    def test_refuses_dirty_trees_and_commits_not_on_origin_main(self):
        def fake(dirty, on_main):
            def git(*args):
                if args[0] == 'status':
                    return ' M file' if dirty else ''
                if args[0] == 'merge-base':
                    if not on_main:
                        raise publish.subprocess.CalledProcessError(1, 'git')
                    return ''
                return 'a' * 40
            return git
        with patch.object(publish, 'git', fake(True, True)), self.assertRaises(publish.PublishError):
            publish.check_source()
        with patch.object(publish, 'git', fake(False, False)), self.assertRaises(publish.PublishError):
            publish.check_source()
        with patch.object(publish, 'git', fake(False, True)):
            self.assertEqual(publish.check_source(), 'a' * 40)

    def test_release_must_match_head_and_cover_every_platform(self):
        with tempfile.TemporaryDirectory() as tmp:
            record = {p: {'file': f'{p}.zip'} for p in publish.EXPECTED_PLATFORMS}
            release = {'commit': 'a' * 40, 'dirty': False, 'archives': record}
            (Path(tmp) / 'release.json').write_text(json.dumps(release))
            self.assertEqual(publish.load_release(tmp, 'a' * 40)['commit'], 'a' * 40)
            with self.assertRaises(publish.PublishError):
                publish.load_release(tmp, 'b' * 40)
            release['dirty'] = True
            (Path(tmp) / 'release.json').write_text(json.dumps(release))
            with self.assertRaises(publish.PublishError):
                publish.load_release(tmp, 'a' * 40)
            release.update(dirty=False, archives={'linux-x86_64': {}})
            (Path(tmp) / 'release.json').write_text(json.dumps(release))
            with self.assertRaises(publish.PublishError):
                publish.load_release(tmp, 'a' * 40)

    def test_checksum_mismatch_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            archive = windows_archive(tmp / 'fury-remastered-0.1.0-abc1234-windows-x86_64.zip')
            digest = desktop_archive.sha256(archive)
            release = {'archives': {'windows-x86_64': {'file': archive.name, 'sha256': digest}}}
            (tmp / 'SHA256SUMS.txt').write_text(f'{digest}  {archive.name}\n')
            self.assertEqual(len(publish.check_archives(tmp, release, scratch=tmp)), 2)
            (tmp / 'SHA256SUMS.txt').write_text(f'{"0" * 64}  {archive.name}\n')
            with self.assertRaises(publish.PublishError):
                publish.check_archives(tmp, release, scratch=tmp)

    def test_dry_run_never_calls_gh_release_create(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            files, archives = [], {}
            for platform in publish.EXPECTED_PLATFORMS:
                archives[platform] = {'file': f'{platform}.zip', 'bytes': 5, 'sha256': '1' * 64}
                files.append(tmp / f'{platform}.zip')
            (tmp / 'release.json').write_text(json.dumps({
                'commit': 'a' * 40, 'short': 'abc1234', 'version': '0.1.0', 'dirty': False, 'archives': archives}))
            calls = []

            def run(cmd, **kwargs):
                calls.append(cmd)
                return type('R', (), {'returncode': 1})()  # `gh release view`: no such release

            with patch.object(publish, 'check_source', return_value='a' * 40), \
                    patch.object(publish, 'check_archives', return_value=files), \
                    patch.object(publish, 'refuse_in_ci'), \
                    patch.object(publish.subprocess, 'run', side_effect=run), \
                    patch('sys.stdout', new_callable=io.StringIO) as out:
                self.assertEqual(publish.main(['--dir', str(tmp), '--dry-run']), 0)
            self.assertTrue(all(c[:3] != ['gh', 'release', 'create'] for c in calls))
            text = out.getvalue()
            self.assertIn('desktop-v0.1.0-abc1234', text)
            self.assertIn('--prerelease', text)

    def test_notes_state_status_and_verification(self):
        release = {'short': 'abc1234', 'archives': {'linux-x86_64': {'file': 'f.tar.gz', 'bytes': 5, 'sha256': '1' * 64}}}
        text = publish.notes(release, 'Launched on Linux only.')
        for phrase in ('Unofficial, incomplete', 'Same engine as the browser game', 'Unsigned builds', 'What was tested',
                       'Launched on Linux only.'):
            self.assertIn(phrase, text)
        self.assertNotIn('What was tested', publish.notes(release))

    def test_android_apk_is_verified_and_described(self):
        import build_android
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            apk = tmp / 'fury-remastered-0.3.0-abc1234-android-arm64.apk'
            apk.write_bytes(b'apk')
            record = {'commit': 'a' * 40, 'short': 'abc1234',
                      'artifacts': [{'name': apk.name, 'sha256': desktop_archive.sha256(apk)}]}
            (tmp / 'release.json').write_text(json.dumps(record))
            with patch.object(build_android, 'check_apk', return_value=3) as checked:
                path, row = publish.load_android(tmp, 'a' * 40)
            checked.assert_called_once_with(apk)
            self.assertEqual((path, row['bytes']), (apk, 3))
            with self.assertRaises(publish.PublishError):
                publish.load_android(tmp, 'b' * 40)  # built from another commit
            apk.write_bytes(b'changed')
            with self.assertRaises(publish.PublishError):
                publish.load_android(tmp, 'a' * 40)
            release = {'short': 'abc1234', 'archives': {'linux-x86_64': {'file': 'f.tar.gz', 'bytes': 5, 'sha256': '1' * 64}}}
            text = publish.notes(release, '', row)
            self.assertIn('android-arm64', text)
            self.assertIn('contains no game content', text)
            self.assertNotIn('android-arm64', publish.notes(release))


class WatcherTests(unittest.TestCase):
    def test_new_tags_are_found_once_and_annotated_tags_are_peeled(self):
        listing = ('1' * 40 + '\trefs/tags/desktop-v0.1.0-a\n'
                   '2' * 40 + '\trefs/tags/desktop-v0.1.1-b\n' + '3' * 40 + '\trefs/tags/desktop-v0.1.1-b^{}\n')
        result = type('R', (), {'stdout': listing})()
        with patch.object(watcher.subprocess, 'run', return_value=result):
            tags = watcher.remote_tags(ROOT)
        self.assertEqual(tags, {'desktop-v0.1.0-a': '1' * 40, 'desktop-v0.1.1-b': '3' * 40})
        state = {'done': {'desktop-v0.1.0-a': {}}, 'failed': {}}
        self.assertEqual(watcher.pending(tags, state), {'desktop-v0.1.1-b': '3' * 40})
        state['failed']['desktop-v0.1.1-b'] = {}
        self.assertEqual(watcher.pending(tags, state), {})

    def make_repo(self, tmp):
        """A throwaway repo: a release commit, a docs-only commit, then optionally an app change."""
        import subprocess
        run = lambda *a: subprocess.run(['git', *a], cwd=tmp, check=True, capture_output=True, text=True).stdout.strip()  # noqa: E731
        run('init', '-q', '-b', 'main')
        run('config', 'user.email', 't@example.com')
        run('config', 'user.name', 't')
        (tmp / 'Cargo.toml').write_text('[workspace.package]\nversion = "0.3.0"\n')
        (tmp / 'crates').mkdir()
        (tmp / 'crates/a.rs').write_text('1')
        run('add', '.')
        import os
        subprocess.run(['git', 'commit', '-qm', 'release'], cwd=tmp, check=True,
                       env={**os.environ, 'GIT_COMMITTER_DATE': '@1000000000 +0000'})
        released = run('rev-parse', 'HEAD')
        (tmp / 'README.md').write_text('docs')
        run('add', '.')
        run('commit', '-qm', 'docs')
        run('update-ref', 'refs/remotes/origin/main', 'HEAD')
        return run, released

    def test_main_is_released_only_after_app_changes_and_the_interval(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            run, released = self.make_repo(tmp)
            tags = {'desktop-v0.2.0-' + released[:7]: released}
            tag, why = watcher.main_candidate(tmp, tags, {}, 12)
            self.assertIsNone(tag)
            self.assertIn('no app changes', why)
            (tmp / 'crates/a.rs').write_text('2')
            run('commit', '-qam', 'app')
            run('update-ref', 'refs/remotes/origin/main', 'HEAD')
            head = run('rev-parse', 'HEAD')
            tag, commit = watcher.main_candidate(tmp, tags, {}, 12, now=1000000000 + 3600)
            self.assertIsNone(tag)
            self.assertIn('waiting', commit)
            tag, commit = watcher.main_candidate(tmp, tags, {}, 12, now=1000000000 + 13 * 3600)
            self.assertEqual((tag, commit), (f'desktop-v0.3.0-{head[:7]}', head))
            state = {'done': {tag: {'commit': head, 'at': 1}}, 'failed': {}}
            self.assertIsNone(watcher.main_candidate(tmp, tags, state, 12)[0])

    def test_main_without_any_release_is_due(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            run, _ = self.make_repo(tmp)
            self.assertTrue(watcher.main_candidate(tmp, {}, {}, 12)[0].startswith('desktop-v0.3.0-'))

    def test_failed_launches_block_publication_and_notes_say_what_ran(self):
        release = {'archives': {'linux-x86_64': {'smoke': {'ok': False, 'emulated': True}},
                                'linux-aarch64': {'smoke': {'ok': True, 'emulated': False}},
                                'macos-universal': {}}}
        self.assertEqual(watcher.failed_smokes(release), ['linux-x86_64'])
        text = watcher.verification(release)
        self.assertIn('linux-aarch64: launched natively', text)
        self.assertIn('macos-universal: built and structurally verified, not launched', text)

    def test_only_published_builds_beyond_the_newest_are_pruned(self):
        import os
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            names = ['aaaaaaa', 'bbbbbbb', 'ccccccc', 'ddddddd', 'eeeeeee', 'packs']
            for i, name in enumerate(names):
                (tmp / name).mkdir()
                os.utime(tmp / name, (i, i))
            state = {'done': {f't{n}': {'commit': n * 40} for n in 'abcd'}}  # eeeeeee is not published
            with patch('sys.stdout', new_callable=io.StringIO):
                watcher.prune_builds(state, out_root=tmp, keep=2)
            self.assertEqual(sorted(p.name for p in tmp.iterdir()), ['ccccccc', 'ddddddd', 'eeeeeee', 'packs'])

    def test_first_pass_adopts_published_tags(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / 'state.json'
            state = watcher.load_state(missing, {'desktop-v0.1.0-a': '1' * 40})
            self.assertEqual(watcher.pending({'desktop-v0.1.0-a': '1' * 40}, state), {})
            missing.write_text('{"done": {}, "failed": {}}')
            self.assertEqual(watcher.load_state(missing, {'desktop-v0.1.0-a': '1' * 40})['done'], {})


if __name__ == '__main__':
    unittest.main()
