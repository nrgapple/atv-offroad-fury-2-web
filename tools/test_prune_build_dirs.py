"""Only finished worktrees lose their build folder or are removed."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

import prune_build_dirs


def git(cwd, *args):
    subprocess.run(['git', '-c', 'user.name=t', '-c', 'user.email=t@t', *args], cwd=cwd, check=True,
                   capture_output=True)


def age(paths, hours=48):
    old = time.time() - hours * 3600
    for path in paths:
        os.utime(path, (old, old))


class PruneBuildDirsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        base = Path(self.temporary.name)
        self.root = base / 'repo'
        self.root.mkdir()
        git(self.root, 'init', '-q', '-b', 'main')
        (self.root / '.gitignore').write_text('/target\n/work\n')
        git(self.root, 'add', '.gitignore')
        git(self.root, 'commit', '-q', '-m', 'base')
        git(self.root, 'update-ref', 'refs/remotes/origin/main', 'HEAD')
        self.trees = {}
        for name in ('merged-idle', 'merged-recent', 'unmerged', 'dirty'):
            tree = self.trees[name] = base / name
            git(self.root, 'worktree', 'add', '-q', '-b', name, str(tree), 'main')
            (tree / 'target/debug').mkdir(parents=True)
        git(self.trees['unmerged'], 'commit', '-q', '--allow-empty', '-m', 'wip')
        (self.trees['dirty'] / 'notes.txt').write_text('uncommitted')
        (self.trees['merged-idle'] / 'work').mkdir()
        (self.trees['merged-idle'] / 'work/probe.py').write_text('scratch')
        for name in ('merged-idle', 'unmerged', 'dirty'):
            tree = self.trees[name]
            git_dir = Path(subprocess.run(['git', 'rev-parse', '--absolute-git-dir'], cwd=tree,
                                          capture_output=True, text=True).stdout.strip())
            age([tree / 'target', tree / 'target/debug', git_dir / 'index', git_dir / 'HEAD',
                 git_dir / 'logs/HEAD'])

    def tearDown(self):
        self.temporary.cleanup()

    def test_build_folders_only_for_merged_idle_worktrees(self):
        found = {path.parent.name for path in prune_build_dirs.candidates(self.root)}
        self.assertEqual(found, {'merged-idle', 'dirty'})

    def test_removes_only_clean_deployed_idle_worktrees_and_keeps_scratch(self):
        found = list(prune_build_dirs.removable_worktrees(self.root, 'origin/main'))
        self.assertEqual([(tree.name, branch) for tree, branch in found], [('merged-idle', 'merged-idle')])
        prune_build_dirs.remove_worktree(self.root, *found[0])
        self.assertFalse(self.trees['merged-idle'].exists())
        archived = self.root / 'private/reports/worktree-archive/merged-idle/work/probe.py'
        self.assertEqual(archived.read_text(), 'scratch')
        branches = subprocess.run(['git', 'branch', '--list', 'merged-idle'], cwd=self.root,
                                  capture_output=True, text=True).stdout
        self.assertEqual(branches.strip(), '')


if __name__ == '__main__':
    unittest.main()
