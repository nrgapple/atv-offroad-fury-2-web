"""Documentation lanes must fail closed for runtime, tooling and mixed changes."""
from pathlib import Path
import tempfile
import unittest

from ci_changes import classify, plan
from test_release_queue import git


class ChangePlanTests(unittest.TestCase):
    def test_docs_metadata_runtime_and_unknown_paths(self):
        self.assertEqual(classify(['README.md', 'research/DEPLOYMENT.md', 'AGENTS.md']), 'docs')
        self.assertEqual(classify(['research/devlog/entries/checkpoint.json']), 'metadata')
        self.assertEqual(classify(['research/devlog.json', 'research/MAP_FIDELITY.md']), 'metadata')
        self.assertEqual(classify(['research/BROWSER_TESTS.md']), 'docs')
        for path in ['tools/build_web.py', '.github/workflows/ci.yml', 'Cargo.lock',
                     'crates/atv-core/src/lib.rs', 'web/devlog.js', 'research/fixture.bin',
                     'research/runtime.json', 'new-file', 'Makefile', 'tools/browser/harness.mjs',
                     'tools/browser/manifest.mjs', 'tools/run_browser_tests.mjs', 'tools/test_fog.mjs']:
            with self.subTest(path=path):
                self.assertEqual(classify(['research/devlog/entries/new.json', path]), 'runtime')

    def test_git_diff_includes_deleted_and_renamed_runtime_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            git(root, 'init', '-b', 'main')
            git(root, 'config', 'user.name', 'CI tests')
            git(root, 'config', 'user.email', 'ci@example.test')
            (root / 'runtime.py').write_text('authored')
            git(root, 'add', '.'); git(root, 'commit', '-m', 'base')
            base = git(root, 'rev-parse', 'HEAD')
            (root / 'research').mkdir()
            git(root, 'mv', 'runtime.py', 'research/README.md')
            git(root, 'commit', '-m', 'rename')
            self.assertEqual(plan({'before': base}, 'push', root)['lane'], 'runtime')

    def test_dispatch_and_missing_base_keep_full_validation(self):
        for event in [{}, {'before': '0' * 40}]:
            self.assertEqual(plan(event, 'push'), {'lane': 'runtime', 'publish': True})
        self.assertEqual(plan({}, 'workflow_dispatch')['lane'], 'runtime')


if __name__ == '__main__':
    unittest.main()
