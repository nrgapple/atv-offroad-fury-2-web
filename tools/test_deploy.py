"""Deployment checks target corrupted artifacts and preservation of prior releases."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

from deploy_watch import Deployer, EXPECTED_FILES, validate_release

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


class DeploymentTests(unittest.TestCase):
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

    def test_failed_server_restores_previous_release(self):
        with tempfile.TemporaryDirectory() as directory:
            storage = Path(directory)
            previous = (storage / 'releases' / COMMIT).resolve()
            previous.mkdir(parents=True)
            (storage / 'current').symlink_to(previous, target_is_directory=True)
            deployer = Deployer({'storage': directory})
            deployer.stop_server = Mock()
            deployer.start_server = Mock(side_effect=[RuntimeError('bad server'), None])
            candidate = storage / 'releases' / ('b' * 40)
            with self.assertRaisesRegex(RuntimeError, 'bad server'):
                deployer.activate(candidate, {})
            self.assertEqual((storage / 'current').resolve(), previous)
            self.assertEqual(deployer.start_server.call_args.args, (previous,))


if __name__ == '__main__':
    unittest.main()
