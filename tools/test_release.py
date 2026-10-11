"""Release checks reject damaged artifacts and accidental private-content inclusion."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from package_release import EXPECTED_FILES, validate_release

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
