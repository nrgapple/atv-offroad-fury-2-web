"""Release checks reject damaged artifacts and accidental private-content inclusion."""
import hashlib
import json
import re
from pathlib import Path
import tempfile
import unittest

from package_release import EXPECTED_FILES, ROOT, STATIC_FILES, validate_release, versioned_source
from fetch_site_release import eligible_run

COMMIT = 'a' * 40


def fixture(root):
    for name in EXPECTED_FILES - {'release.json'}:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b'\0asm\x01\0\0\0' if path.suffix == '.wasm' else b'authored content')
    (root / 'static/deployment.json').write_text(json.dumps({'commit': COMMIT}))
    hashes = {name: hashlib.sha256((root / name).read_bytes()).hexdigest()
              for name in EXPECTED_FILES - {'release.json'}}
    (root / 'release.json').write_text(json.dumps({'commit': COMMIT, 'sha256': hashes}))


class ReleaseTests(unittest.TestCase):
    def test_release_versions_styles_modules_worker_and_wasm_together(self):
        for name in STATIC_FILES:
            if not name.endswith(('.html', '.js', '.css')):
                continue
            source = versioned_source((ROOT / 'web' / name).read_text(), COMMIT)
            for quote, relative, target, version in re.findall(
                    r'''(["'])(\./)?([a-z0-9_-]+\.(?:js|css|wasm))(\?v=[a-f0-9]+)?\1''', source):
                if target in STATIC_FILES:
                    self.assertEqual(version, '?v=' + COMMIT, f'{name} references unversioned {target}')
        sample = 'import "./hud.js"; new Worker(\'worker.js\'); fetch("atv_web.wasm");'
        self.assertEqual(versioned_source(sample, COMMIT).count('?v=' + COMMIT), 3)
        self.assertIn(f'fetch("atv_web.wasm?v={COMMIT}")', versioned_source(sample, COMMIT))
        self.assertIn(f'import "./hud.js?v={COMMIT}"', versioned_source(sample, COMMIT))
        self.assertEqual(versioned_source("fetch('/content/manifest.json');", COMMIT), "fetch('/content/manifest.json');")

    def test_authored_module_dependencies_are_packaged(self):
        # A valid hash list alone cannot catch an omitted imported module.
        for name in STATIC_FILES:
            if not name.endswith(".js"):
                continue
            source = (ROOT / "web" / name).read_text()
            dependencies = re.findall(r"(?:from\s+|import\s*(?:\(\s*)?)[\"'](\./[^\"']+)", source)
            for dependency in dependencies:
                path = (Path(name).parent / dependency).as_posix()
                self.assertIn(path, STATIC_FILES, f"{name} imports unpackaged {path}")

    def test_valid_release_is_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            self.assertEqual(validate_release(root, COMMIT)['commit'], COMMIT)

    def test_publication_gate_rejects_pull_request_and_stale_commit(self):
        base = {'head_sha': COMMIT, 'head_branch': 'main', 'status': 'completed',
                'conclusion': 'success', 'event': 'push'}
        self.assertIsNone(eligible_run([{**base, 'event': 'pull_request'}], COMMIT))
        self.assertIsNone(eligible_run([{**base, 'head_sha': 'b' * 40}], COMMIT))
        self.assertIsNone(eligible_run([{**base, 'conclusion': 'failure'}], COMMIT))
        self.assertEqual(eligible_run([base], COMMIT), base)

    def test_rejects_corrupted_wasm(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            (root / 'static/atv_web.wasm').write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                validate_release(root, COMMIT)

    def test_rejects_unexpected_private_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            (root / 'disc.iso').write_text('private')
            with self.assertRaisesRegex(ValueError, 'allowlist mismatch'):
                validate_release(root, COMMIT)

    def test_rejects_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            (root / 'static/index.html').unlink()
            (root / 'static/index.html').symlink_to('/etc/hosts')
            with self.assertRaisesRegex(ValueError, 'symlink'):
                validate_release(root, COMMIT)

    def test_rejects_wrong_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            with self.assertRaisesRegex(ValueError, 'commit does not match'):
                validate_release(root, 'b' * 40)

    def test_rejects_missing_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            manifest = json.loads((root / 'release.json').read_text())
            del manifest['sha256']['static/game.js']
            (root / 'release.json').write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'checksum list mismatch'):
                validate_release(root, COMMIT)


if __name__ == '__main__':
    unittest.main()
