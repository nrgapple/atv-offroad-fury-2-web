"""Cloud artifacts must match the exact local source, never another PR or tree."""
import unittest
import tempfile
from pathlib import Path
from unittest.mock import patch

import fetch_pr_build as fetcher
from test_release import fixture, COMMIT

HEAD, TREE = 'a' * 40, 'b' * 40


def run():
    return {'event': 'pull_request', 'path': '.github/workflows/ci.yml',
            'status': 'completed', 'conclusion': 'success', 'head_sha': HEAD,
            'repository': {'full_name': fetcher.REPOSITORY},
            'head_repository': {'full_name': fetcher.REPOSITORY}}


class PrBuildTests(unittest.TestCase):
    def test_only_successful_same_repo_pr_at_head_is_accepted(self):
        fetcher.require_run(run(), HEAD)
        for key, value in [('event', 'push'), ('path', '.github/workflows/other.yml'),
                           ('status', 'in_progress'), ('conclusion', 'failure'),
                           ('head_sha', 'c' * 40), ('repository', {'full_name': 'other/repo'}),
                           ('head_repository', {'full_name': 'fork/repo'})]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                fetcher.require_run({**run(), key: value}, HEAD)

    def test_merge_tree_and_parent_must_match(self):
        commit = {'tree': {'sha': TREE}, 'parents': [{'sha': HEAD}]}
        fetcher.require_tree(commit, HEAD, TREE)
        for head, tree in [('c' * 40, TREE), (HEAD, 'c' * 40)]:
            with self.assertRaises(ValueError):
                fetcher.require_tree(commit, head, tree)

    def test_download_validates_bytes_and_never_installs_bad_artifacts(self):
        for corrupt in [False, True]:
            with self.subTest(corrupt=corrupt), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                info = {**run(), 'run_attempt': 1, 'html_url': 'https://github.com/run'}
                built = {'tree': {'sha': TREE}, 'parents': [{'sha': HEAD}]}
                def download(command, **kwargs):
                    stage = Path(command[command.index('--dir') + 1])
                    fixture(stage)
                    if corrupt:
                        (stage / 'static/atv_app_bg.wasm').write_bytes(b'corrupt')
                with patch.object(fetcher, 'ROOT', root), \
                        patch.object(fetcher, 'source_identity', return_value=(HEAD, TREE)), \
                        patch.object(fetcher.shutil, 'which', return_value='gh'), \
                        patch.object(fetcher, 'gh_json', side_effect=[info, built, info]), \
                        patch.object(fetcher.subprocess, 'run', side_effect=download):
                    if corrupt:
                        with self.assertRaises(ValueError):
                            fetcher.fetch(42)
                        self.assertFalse((root / 'work/pr-builds/42-1').exists())
                    else:
                        result = fetcher.fetch(42)
                        self.assertEqual(result['commit'], COMMIT)
                        self.assertTrue((Path(result['root']) / 'atv_app_bg.wasm').is_file())
                    self.assertFalse((root / 'web/dist').exists())

    def test_dirty_and_untracked_source_are_rejected(self):
        for status in [' M crates/atv-app/src/lib.rs', '?? web/new.js']:
            with patch.object(fetcher, 'git', return_value=status), self.assertRaises(ValueError):
                fetcher.source_identity()
        with patch.object(fetcher, 'git', side_effect=['', HEAD, TREE]):
            self.assertEqual(fetcher.source_identity(), (HEAD, TREE))


if __name__ == '__main__':
    unittest.main()
