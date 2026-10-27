"""Only merged, idle worktrees lose their regenerable target/ folder."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

import prune_build_dirs


def git(cwd, *args):
    subprocess.run(['git', *args], cwd=cwd, check=True, capture_output=True)


class PruneBuildDirsTests(unittest.TestCase):
    def test_keeps_unmerged_and_recent_targets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'repo'
            root.mkdir()
            git(root, 'init', '-q', '-b', 'main')
            git(root, '-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '--allow-empty', '-m', 'base')
            git(root, 'update-ref', 'refs/remotes/origin/main', 'HEAD')
            trees = {}
            for name in ('merged-idle', 'merged-recent', 'unmerged'):
                trees[name] = Path(temporary) / name
                git(root, 'worktree', 'add', '-q', '-b', name, str(trees[name]), 'main')
                (trees[name] / 'target/debug').mkdir(parents=True)
            git(trees['unmerged'], '-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '--allow-empty', '-m', 'wip')
            old = time.time() - 48 * 3600
            for name in ('merged-idle', 'unmerged'):
                for path in (trees[name] / 'target', trees[name] / 'target/debug'):
                    os.utime(path, (old, old))
            found = {path.parent.name for path in prune_build_dirs.candidates(root)}
            self.assertEqual(found, {'merged-idle'})


if __name__ == '__main__':
    unittest.main()
