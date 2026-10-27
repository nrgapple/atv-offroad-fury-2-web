#!/usr/bin/env python3
"""Reclaim disk from finished worktrees.

Each worktree builds into its own target/ (.cargo/config.toml), several GB each.
By default this deletes target/ in worktrees whose HEAD is already in origin/main
and whose build folder has been idle. With --worktrees it also removes whole
task worktrees once their work is merged and deployed (HEAD is in the commit
production serves), nothing is uncommitted or untracked, and git has been idle;
their local branch is deleted and any ignored work/ scratch is moved to
private/reports/worktree-archive/ first. The shared checkout and Claude Code's
own .claude/worktrees are never removed. Dry run unless --apply.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import time
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent
DEPLOYMENT = 'https://furyremastered.com/deployment.json'


def worktrees(root):
    listing = subprocess.run(['git', 'worktree', 'list', '--porcelain'], cwd=root, check=True,
                             capture_output=True, text=True).stdout
    for block in listing.strip().split('\n\n'):
        fields = dict(line.split(' ', 1) for line in block.splitlines() if ' ' in line)
        if 'worktree' in fields and 'HEAD' in fields:
            branch = fields.get('branch', '').removeprefix('refs/heads/') or None
            yield Path(fields['worktree']), fields['HEAD'], branch


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
    for tree, head, _ in worktrees(root):
        target = tree / 'target'
        if target.is_dir() and not target.is_symlink() and merged(root, head, base) \
                and not recently_used(target, idle_hours * 3600):
            yield target


def git_idle(tree, idle_seconds, now=None):
    git_dir = Path(subprocess.run(['git', 'rev-parse', '--absolute-git-dir'], cwd=tree, check=True,
                                  capture_output=True, text=True).stdout.strip())
    cutoff = (now or time.time()) - idle_seconds
    return all(not path.exists() or path.stat().st_mtime <= cutoff
               for path in (git_dir / 'index', git_dir / 'HEAD', git_dir / 'logs/HEAD'))


def removable_worktrees(root, deployed, idle_hours=12):
    shared = root.resolve()
    for tree, head, branch in worktrees(root):
        resolved = tree.resolve()
        if resolved == shared or '.claude' in resolved.parts or not branch:
            continue
        # Check idleness before git status, which may refresh the index.
        if not git_idle(tree, idle_hours * 3600) or not merged(root, head, deployed):
            continue
        status = subprocess.run(['git', 'status', '--porcelain'], cwd=tree, check=True,
                                capture_output=True, text=True).stdout
        if not status.strip():
            yield tree, branch


def shared_checkout(root):
    common = subprocess.run(['git', 'rev-parse', '--path-format=absolute', '--git-common-dir'], cwd=root,
                            check=True, capture_output=True, text=True).stdout.strip()
    return Path(common).parent


def remove_worktree(root, tree, branch):
    work = tree / 'work'
    if work.is_dir() and not work.is_symlink() and any(work.iterdir()):
        # Archive into the shared checkout's private/, which outlives every worktree.
        archive = shared_checkout(root) / 'private/reports/worktree-archive' / tree.name
        archive.mkdir(parents=True, exist_ok=True)
        shutil.move(str(work), str(archive / 'work'))
    target = tree / 'target'
    if target.is_dir() and not target.is_symlink():
        shutil.rmtree(target)
    subprocess.run(['git', 'worktree', 'remove', str(tree)], cwd=root, check=True, capture_output=True)
    subprocess.run(['git', 'branch', '-D', branch], cwd=root, check=True, capture_output=True)


def deployed_commit():
    request = Request(DEPLOYMENT, headers={'User-Agent': 'Fury-prune/1.0', 'Cache-Control': 'no-cache'})
    with urlopen(request, timeout=30) as response:
        return json.load(response)['commit']


def size(path):
    return sum(f.stat().st_size for f in path.rglob('*') if f.is_file() and not f.is_symlink())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='delete instead of listing')
    parser.add_argument('--worktrees', action='store_true', help='also remove merged, deployed, clean worktrees')
    parser.add_argument('--idle-hours', type=float, default=12)
    args = parser.parse_args(argv)
    verb = 'removed' if args.apply else 'would remove'
    total = 0
    if args.worktrees:
        subprocess.run(['git', 'fetch', '-q', 'origin'], cwd=ROOT, check=True)
        for tree, branch in list(removable_worktrees(ROOT, deployed_commit(), args.idle_hours)):
            print(f'{verb} worktree {tree} ({branch})')
            if args.apply:
                remove_worktree(ROOT, tree, branch)
    for target in candidates(ROOT, idle_hours=args.idle_hours):
        bytes_ = size(target)
        total += bytes_
        print(f'{verb} {target} ({bytes_ / 2**30:.1f} GB)')
        if args.apply:
            shutil.rmtree(target)
    print(f'{"Freed" if args.apply else "Reclaimable"} from build folders: {total / 2**30:.1f} GB')


if __name__ == '__main__':
    main()
