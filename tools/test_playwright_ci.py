import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import playwright_ci as pw

RUNNER = {'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64', 'ImageOS': 'ubuntu24', 'ImageVersion': '20261019.1.0'}
DRY_RUN = ('sudo -- sh -c "apt-get update&& apt-get install -y --no-install-recommends '
           'libasound2t64 libatk1.0-0t64 fonts-noto-color-emoji gstreamer1.0-libav xvfb"\n')


def archive(directory, name, data, pool_name=None, md5=True):
    """A print-uris line (MD5 or no hash, URL-encoded) and its apt-cache index record.

    The local name carries an epoch as %3a; the pool file name has no epoch.
    """
    (directory / name).write_bytes(data)
    package, version, arch = name[:-4].split('_')
    pool_name = pool_name or name
    url = f"http://azure.archive.ubuntu.com/ubuntu/pool/main/{pool_name.replace('~', '%7e').replace('+', '%2b')}"
    line = f"'{url}' {name} {len(data)} " + (f'MD5Sum:{hashlib.md5(data).hexdigest()}' if md5 else '')
    record = (f'Package: {package}\nArchitecture: {arch}\nVersion: {version}\nFilename: pool/main/{pool_name}\n'
              f'Size: {len(data)}\nMD5sum: {hashlib.md5(data).hexdigest()}\nSHA256: {hashlib.sha256(data).hexdigest()}\n'
              ' multi-line: description\n')
    return line, record


class Result:
    def __init__(self, code):
        self.returncode = code


class PlaywrightCacheTest(unittest.TestCase):
    def test_keys_are_exact_and_reject_unknown_runner_identity(self):
        keys = pw.cache_keys(pw.identity(RUNNER))
        self.assertEqual(keys['browsers-key'], 'fury-playwright-v1-Linux-X64-ubuntu24-1.58.2-chromium-headless-shell-webkit')
        self.assertEqual(keys['apt-key'], 'fury-playwright-apt-v1-Linux-X64-ubuntu24-20261019.1.0-1.58.2-chromium-headless-shell-webkit')
        for name in RUNNER:
            with self.assertRaisesRegex(ValueError, 'runner identity'):
                pw.identity({**RUNNER, name: ''})
        with self.assertRaisesRegex(ValueError, 'runner identity'):
            pw.identity({**RUNNER, 'ImageVersion': '1 2'})

    def test_dry_run_package_list_is_parsed_and_validated(self):
        self.assertEqual(pw.parse_dry_run(DRY_RUN)[:2], ['libasound2t64', 'libatk1.0-0t64'])
        self.assertEqual(len(pw.parse_dry_run(DRY_RUN.replace('sudo -- ', ''))), 5)
        for text in ('', 'Cannot install dependencies for mac15-arm64', DRY_RUN.replace('xvfb', 'xvfb;rm')):
            with self.assertRaisesRegex(ValueError, 'dependency list'):
                pw.parse_dry_run(text)

    def test_restored_archives_must_match_signed_index_hashes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source, cache = root / 'source', root / 'cache'
            source.mkdir(), cache.mkdir()
            rows = [archive(source, 'good_1~b+2_amd64.deb', b'good'),
                     archive(source, 'stale_2_all.deb', b'new bytes'),
                     archive(source, 'missing_1%3a1_amd64.deb', b'missing', 'missing_1_amd64.deb', md5=False)]
            archives = pw.parse_print_uris('Reading package lists...\n' + '\n'.join(line for line, _ in rows))
            self.assertEqual(archives['good_1~b+2_amd64.deb'][2], 'good=1~b+2')
            self.assertEqual(archives['missing_1%3a1_amd64.deb'][2], 'missing=1:1')
            expected = pw.index_hashes(archives, '\n'.join(record for _, record in rows))
            self.assertEqual(set(expected), {'good_1~b+2_amd64.deb', 'stale_2_all.deb', 'missing_1%3a1_amd64.deb'})
            self.assertEqual(expected['stale_2_all.deb'][1:], ('SHA256', hashlib.sha256(b'new bytes').hexdigest()))
            (cache / 'good_1~b+2_amd64.deb').write_bytes(b'good')
            (cache / 'stale_2_all.deb').write_bytes(b'old bytes')  # same size, wrong hash
            (cache / 'unlisted_1_amd64.deb').write_bytes(b'extra')
            (cache / 'link_1_amd64.deb').symlink_to(source / 'good_1~b+2_amd64.deb')
            self.assertEqual(pw.prune_archives(cache, expected), (1, 3))
            self.assertEqual(sorted(p.name for p in cache.iterdir()), ['good_1~b+2_amd64.deb'])
            self.assertTrue((source / 'good_1~b+2_amd64.deb').exists())
            with self.assertRaisesRegex(ValueError, 'signed index'):
                pw.index_hashes(archives, rows[0][1])
            with self.assertRaisesRegex(ValueError, 'apt archive line'):
                pw.parse_print_uris("'http://x/a.deb' a.deb")
            for name in ('.hidden.deb', 'a_1_x_y.deb', 'a_1;rm_amd64.deb'):
                with self.assertRaisesRegex(ValueError, 'archive name'):
                    pw.parse_print_uris(f"'http://x/{name}' {name} 3 MD5Sum:ab")

    def test_retry_drops_only_the_slow_mirror(self):
        mirrors = ('http://azure.archive.ubuntu.com/ubuntu/\tpriority:1\n'
                   'https://archive.ubuntu.com/ubuntu/\tpriority:2\n'
                   'https://security.ubuntu.com/ubuntu/\tpriority:3\n')
        self.assertEqual(pw.fallback_mirrors(mirrors), mirrors.split('\n', 1)[1])
        self.assertIsNone(pw.fallback_mirrors('http://azure.archive.ubuntu.com/ubuntu/\n'))
        self.assertIsNone(pw.fallback_mirrors('https://archive.ubuntu.com/ubuntu/\n'))

    def test_attempts_are_bounded_by_timeout_and_retried_once(self):
        calls = []
        def run(command, check):
            calls.append(command)
            return Result(124 if len(calls) == 1 else 0)
        retried = mock.Mock()
        with tempfile.TemporaryDirectory() as temp:
            installer = pw.Installer(Path(temp), False, False, run=run, log=lambda _: None)
            installer.attempts('download', ['apt-get', 'install'], root=True, before_retry=retried)
            self.assertEqual(calls[0], ['sudo', 'env', 'DEBIAN_FRONTEND=noninteractive', 'timeout', '-k', '10', '120', 'apt-get', 'install'])
            self.assertEqual(calls[1][6], '180')
            retried.assert_called_once()
            self.assertEqual(installer.report['phases']['download']['attempts'], 2)
            failing = pw.Installer(Path(temp), False, False, run=lambda *a, **k: Result(1), log=lambda _: None)
            with self.assertRaisesRegex(RuntimeError, 'browsers failed after 2'):
                failing.attempts('browsers', ['playwright', 'install'])

    def test_restored_package_must_report_the_pinned_version(self):
        with tempfile.TemporaryDirectory() as temp:
            installer = pw.Installer(Path(temp), True, True, log=lambda _: None)
            for name in ('playwright', 'playwright-core'):
                manifest = installer.package / 'node_modules' / name / 'package.json'
                manifest.parent.mkdir(parents=True)
                manifest.write_text(json.dumps({'version': pw.VERSION}))
            self.assertFalse(installer.package_valid())  # no CLI
            installer.cli.parent.mkdir(parents=True)
            installer.cli.write_text('')
            self.assertTrue(installer.package_valid())
            (installer.package / 'node_modules/playwright-core/package.json').write_text('{"version": "1.58.1"}')
            self.assertFalse(installer.package_valid())

    def test_browsers_that_do_not_launch_are_reinstalled_clean(self):
        with tempfile.TemporaryDirectory() as temp:
            installer = pw.Installer(Path(temp), True, True, log=lambda _: None)
            installer.browsers = Path(temp) / 'browsers'
            (installer.browsers / 'webkit-2248').mkdir(parents=True)
            launches = iter([False, True])
            with mock.patch.object(installer, 'ensure_package'), mock.patch.object(installer, 'install_packages'), \
                    mock.patch.object(installer, 'attempts') as attempts, mock.patch.object(installer, 'launch', lambda: next(launches)):
                report = installer.install()
            self.assertEqual(attempts.call_count, 2)
            self.assertEqual(report['browsers'], {'source': 'cold', 'installed': []})


if __name__ == '__main__':
    unittest.main()
