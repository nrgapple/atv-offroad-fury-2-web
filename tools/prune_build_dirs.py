#!/usr/bin/env python3
"""Delete regenerable target/ build folders of merged, idle worktrees.

Each worktree builds into its own target/ (.cargo/config.toml), several GB each.
A folder is removed only when its worktree's HEAD is already in origin/main and
nothing in the folder changed recently. Worktrees, their files and branches are
never touched. Dry run by default; pass --apply to delete.
"""
import argparse
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent


def worktrees(root):
    listing = subprocess.run(['git', 'worktree', 'list', '--porcelain'], cwd=root, check=True,
                             capture_output=True, text=True).stdout
    for block in listing.strip().split('\n\n'):
        fields = dict(line.split(' ', 1) for line in block.splitlines() if ' ' in line)
        if 'worktree' in fields and 'HEAD' in fields:
            yield Path(fields['worktree']), fields['HEAD']


def merged(root, commit, base):
    return subprocess.run(['git', 'merge-base', '--is-ancestor', commit, base], cwd=root,
                          capture_output=True).returncode == 0


def recently_used(target, idle_seconds, now=None):
    cutoff = (now or time.time()) - idle_seconds
    stack = [(target, 0)]
    while stack:
        path, depth = stack.pop()
        try:
            if path.stat().st_mtime > cutoff:
                return True
            if depth < 3 and path.is_dir() and not path.is_symlink():
                stack.extend((child, depth + 1) for child in path.iterdir())
        except OSError:
            continue
    return False


def candidates(root, base='origin/main', idle_hours=12):
    for tree, head in worktrees(root):
        target = tree / 'target'
        if target.is_dir() and not target.is_symlink() and merged(root, head, base) \
                and not recently_used(target, idle_hours * 3600):
            yield target


def size(path):
    return sum(f.stat().st_size for f in path.rglob('*') if f.is_file() and not f.is_symlink())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='delete instead of listing')
    parser.add_argument('--idle-hours', type=float, default=12)
    args = parser.parse_args(argv)
    total = 0
    for target in candidates(ROOT, idle_hours=args.idle_hours):
        bytes_ = size(target)
        total += bytes_
        print(f'{"removed" if args.apply else "would remove"} {target} ({bytes_ / 2**30:.1f} GB)')
        if args.apply:
            shutil.rmtree(target)
    print(f'{"Freed" if args.apply else "Reclaimable"}: {total / 2**30:.1f} GB')


if __name__ == '__main__':
    main()
