"""Regression gates for local provenance and late-CI rollback protection."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from test_release import fixture, COMMIT
from local_release import local_artifact
from deployment_gate import decision
from deploy_local import require_clean_main, deploy
import deploy_local
import upload_cloudflare_content as uploader


class LocalReleaseTests(unittest.TestCase):
    def checked(self, root):
        folder = root / 'private/reports/local-releases' / COMMIT
        artifact = folder / 'artifact'; fixture(artifact)
        record = {'commit': COMMIT, 'public_verified': True, 'checks': [['make', 'check']],
                  'owner': 'owner', 'checkpoint': 'checkpoint',
                  'manifest_sha256': hashlib.sha256((artifact / 'release.json').read_bytes()).hexdigest()}
        (folder / 'receipt.json').write_text(json.dumps(record))
        return folder, artifact, record

    def test_receipt_requires_public_verification_and_matching_allowlisted_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.assertIsNone(local_artifact(COMMIT, root))
            folder, artifact, record = self.checked(root)
            self.assertEqual(local_artifact(COMMIT, root), artifact)
            record['public_verified'] = False
            (folder / 'receipt.json').write_text(json.dumps(record))
            with self.assertRaisesRegex(ValueError, 'public verification'):
                local_artifact(COMMIT, root)
            record['public_verified'] = True
            (folder / 'receipt.json').write_text(json.dumps(record))
            (artifact / 'static/game.js').write_text('tampered')
            with self.assertRaisesRegex(ValueError, 'checksum'):
                local_artifact(COMMIT, root)

    def test_valid_rehashed_artifact_still_must_match_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); _, artifact, _ = self.checked(root)
            manifest = artifact / 'release.json'
            manifest.write_text(manifest.read_text() + '\n')
            with self.assertRaisesRegex(ValueError, 'receipt does not match'):
                local_artifact(COMMIT, root)

    def test_r2_accepts_checked_local_without_ci_but_still_requires_live_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self.checked(root)
            with patch.object(uploader, 'ROOT', root), patch.object(uploader, 'public_commit', return_value=COMMIT), patch('fetch_site_release.gh_json', side_effect=AssertionError('must not require Actions')):
                uploader.require_deployed_release(COMMIT)
            with patch.object(uploader, 'ROOT', root), patch.object(uploader, 'public_commit', return_value='b'*40):
                with self.assertRaisesRegex(ValueError, 'no longer deployed'):
                    uploader.require_deployed_release(COMMIT)

    def test_delayed_actions_cannot_rollback_or_overlap_local_owner(self):
        self.assertFalse(decision('old', 'new', 'live', 'ahead')[0])
        self.assertFalse(decision('main', 'main', 'live', 'ahead', True)[0])
        self.assertFalse(decision('main', 'main', 'main', 'identical')[0])
        self.assertFalse(decision('old', 'old', 'new', 'behind')[0])
        self.assertTrue(decision('main', 'main', 'old', 'ahead')[0])
        self.assertTrue(decision('app', 'later-docs', 'old', 'ahead', main_compatible=True)[0])
        with self.assertRaisesRegex(ValueError, 'descendant'):
            decision('main', 'main', 'foreign', 'diverged')

    def test_uncommitted_or_unmerged_checkout_is_rejected(self):
        with patch('deploy_local.git', return_value=' M web/game.js'):
            with self.assertRaisesRegex(ValueError, 'commit all'):
                require_clean_main(COMMIT)
        with patch('deploy_local.git', side_effect=['', COMMIT, 'b'*40]):
            with self.assertRaisesRegex(ValueError, 'exact merged'):
                require_clean_main(COMMIT)
        with patch('deploy_local.git', side_effect=['', COMMIT, COMMIT]):
            require_clean_main(COMMIT)

    def test_failed_local_check_prevents_worker_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def run(*command):
                if command == ('make', 'check'):
                    raise ValueError('test failure')
            with patch.object(deploy_local, 'ROOT', root), patch('deploy_local.run', side_effect=run), patch('deploy_local.require_clean_main'), patch('deploy_local.require_owner'), patch('deploy_local.cancel_main_runs'), patch('deploy_local.gh_json', side_effect=[{'private': True}, {'variables': []}]), patch('deploy_local.stage') as stage:
                with self.assertRaisesRegex(ValueError, 'test failure'):
                    deploy(COMMIT, 'checkpoint', 'owner', 'stalled')
                stage.assert_not_called()
                self.assertFalse((root / 'private/reports/local-releases').exists())


if __name__ == '__main__':
    unittest.main()
