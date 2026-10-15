"""Exercise artifact reuse, publication ownership and honest milestone timing."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from release_checkpoint import claim, record, status, publication_lock, require_owner
from reuse_site_release import refresh, prepare
from test_release import fixture, COMMIT
from test_fixtures import git
import upload_cloudflare_content as uploader
import fetch_site_release as fetcher


class WorkflowTests(unittest.TestCase):
    def repository(self, root):
        git(root, 'init', '-b', 'main')
        git(root, 'config', 'user.name', 'Release tests')
        git(root, 'config', 'user.email', 'release@example.test')
        path = root / 'research/devlog.json'; path.parent.mkdir(parents=True)
        path.write_text(json.dumps({'entries': [], 'updated_at': '2026-10-01T00:00:00Z'}))
        (root / 'game.rs').write_text('runtime')
        git(root, 'add', '.'); git(root, 'commit', '-m', 'application')
        application = git(root, 'rev-parse', 'HEAD')
        release = root / 'work/base'; fixture(release)
        metadata = json.loads((release / 'release.json').read_text())
        metadata['commit'] = application
        (release / 'static/deployment.json').write_text(json.dumps({'commit': application}))
        metadata['sha256']['static/deployment.json'] = hashlib.sha256((release / 'static/deployment.json').read_bytes()).hexdigest()
        (release / 'release.json').write_text(json.dumps(metadata))
        (root / '.gitignore').write_text('/work\n')
        git(root, 'add', '.gitignore'); git(root, 'commit', '-m', 'ignore output')
        # Include ignore policy in the base application, since unknown paths must
        # trigger runtime checks rather than qualify for reuse.
        application = git(root, 'rev-parse', 'HEAD')
        metadata['commit'] = application
        (release / 'static/deployment.json').write_text(json.dumps({'commit': application}))
        metadata['sha256']['static/deployment.json'] = hashlib.sha256((release / 'static/deployment.json').read_bytes()).hexdigest()
        (release / 'release.json').write_text(json.dumps(metadata))
        return application, release

    def test_reuse_changes_only_devlog_and_identity_and_reads_committed_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); application, base = self.repository(root)
            source = {p.relative_to(base).as_posix(): p.read_bytes() for p in base.rglob('*') if p.is_file()}
            log = root / 'research/devlog.json'
            log.write_text(json.dumps({'entries': [{'id': 'new', 'title': 'Verified', 'image': '/devlog-images/private.png'}]}))
            git(root, 'add', 'research'); git(root, 'commit', '-m', 'metadata')
            revision = git(root, 'rev-parse', 'HEAD')
            log.write_text('dirty metadata must not be used')
            target = root / 'work/release'
            refresh(base, target, revision, root)
            changed = {name for name, data in source.items() if (target / name).read_bytes() != data}
            self.assertEqual(changed, {'release.json', 'static/devlog.json', 'static/deployment.json'})
            metadata = json.loads((target / 'release.json').read_text())
            self.assertEqual(metadata['application_commit'], application)
            self.assertEqual(metadata['reused_from'], application)
            self.assertEqual(json.loads((target / 'static/devlog.json').read_text())['entries'], [{'id': 'new', 'title': 'Verified'}])
            self.assertEqual({p.relative_to(base).as_posix(): p.read_bytes() for p in base.rglob('*') if p.is_file()}, source)

    def test_runtime_changes_and_damaged_artifacts_cannot_be_reused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); _, base = self.repository(root)
            (root / 'game.rs').write_text('changed runtime')
            git(root, 'add', 'game.rs'); git(root, 'commit', '-m', 'runtime')
            with self.assertRaisesRegex(ValueError, 'runtime changed'):
                refresh(base, root / 'work/release', git(root, 'rev-parse', 'HEAD'), root)
            (base / 'static/game.js').write_text('corrupt')
            with self.assertRaisesRegex(ValueError, 'checksum'):
                refresh(base, root / 'work/release', git(root, 'rev-parse', 'HEAD'), root)

    def test_pr_metadata_and_pure_docs_do_no_build_and_no_network(self):
        with patch('reuse_site_release.public_commit', side_effect=AssertionError('no network')):
            self.assertEqual(prepare('metadata', 'pull_request', COMMIT)['build'], False)
            self.assertEqual(prepare('docs', 'push', COMMIT)['build'], False)
            self.assertEqual(prepare('runtime', 'push', COMMIT)['build'], True)
        with patch('reuse_site_release.public_commit', side_effect=OSError('unavailable')):
            self.assertEqual(prepare('metadata', 'push', COMMIT), {'build': True, 'reused': False})

    def test_claim_prevents_duplicate_owners_and_publisher_lock_is_nonblocking(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            claim(root, 'combined', 'owner-one')
            self.assertEqual(claim(root, 'combined', 'owner-one')['owner'], 'owner-one')
            with self.assertRaisesRegex(ValueError, 'belongs to owner-one'):
                claim(root, 'combined', 'owner-two')
            with self.assertRaisesRegex(ValueError, 'belongs to owner-one'):
                require_owner(root, 'combined', 'owner-two')
            with publication_lock(root):
                with self.assertRaisesRegex(ValueError, 'another release owner'):
                    with publication_lock(root):
                        self.fail('must not acquire second publisher lock')
            with publication_lock(root):
                pass

    def test_milestones_separate_implementation_from_delivery_without_shared_git_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            claim(root, 'feature', 'owner')
            with self.assertRaisesRegex(ValueError, 'implementation-ready'):
                record(root, 'feature', 'owner', 'live', COMMIT, 'proof')
            for index, stage in enumerate(['implementation-ready', 'merged', 'live', 'gallery-complete']):
                record(root, 'feature', 'owner', stage, COMMIT, 'verified', f'2026-10-14T01:{index:02d}:00Z')
            receipt = status(root, 'feature')
            self.assertEqual(receipt['elapsed_seconds'], {'merged': 60, 'live': 60, 'gallery-complete': 60})
            self.assertEqual(record(root, 'feature', 'owner', 'live', COMMIT, 'retry'), receipt['milestones']['live'])
            with self.assertRaisesRegex(ValueError, 'already recorded'):
                record(root, 'feature', 'owner', 'live', 'b' * 40, 'later revision')
            self.assertFalse((root / 'research').exists())

    def test_publication_follows_validated_live_commit_even_when_main_has_advanced(self):
        run = {'head_sha': COMMIT, 'head_branch': 'main', 'status': 'completed', 'conclusion': 'success', 'event': 'push'}
        with patch('fetch_site_release.gh_json', return_value={'workflow_runs': [run]}), patch.object(uploader, 'public_commit', return_value=COMMIT):
            uploader.require_deployed_release(COMMIT)
        with patch('fetch_site_release.gh_json', return_value={'workflow_runs': [run]}), patch.object(uploader, 'public_commit', return_value='b' * 40):
            with self.assertRaisesRegex(ValueError, 'no longer deployed'):
                uploader.require_deployed_release(COMMIT)

    def test_cli_passes_required_checkpoint_and_owner_to_publication_guard(self):
        with patch.object(uploader, 'upload') as upload:
            uploader.main(['--release', 'work/release', '--commit', COMMIT,
                           '--checkpoint', 'combined', '--owner', 'only-owner'])
            self.assertEqual(upload.call_args.kwargs['checkpoint'], 'combined')
            self.assertEqual(upload.call_args.kwargs['owner'], 'only-owner')

    def test_fetch_deployed_artifact_does_not_query_newer_main_or_skip_download(self):
        run = {'id': 123, 'html_url': 'https://example.test/run', 'head_sha': COMMIT,
               'head_branch': 'main', 'status': 'completed', 'conclusion': 'success', 'event': 'push'}
        def github(_gh, endpoint):
            if '/runs?' in endpoint:
                return {'workflow_runs': [run]}
            raise AssertionError('must pin deployed artifact, never query current main')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture(root / 'work/github-releases' / COMMIT)
            with patch.object(fetcher, 'ROOT', root), patch.object(fetcher, 'gh_json', side_effect=github), patch('reuse_site_release.public_commit', return_value=COMMIT):
                result = fetcher.fetch('nrgapple/atv-offroad-fury-2-web', 'https://furyremastered.com', deployed=True)
                self.assertEqual(result['status'], 'ready')
                self.assertEqual(result['commit'], COMMIT)


if __name__ == '__main__':
    unittest.main()
