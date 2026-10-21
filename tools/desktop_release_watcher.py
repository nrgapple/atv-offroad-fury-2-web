#!/usr/bin/env python3
"""Build and publish the desktop archives when a `desktop-v*` tag appears on origin.

One pass per run, meant for a launchd StartInterval job (see research/DESKTOP_RELEASES.md;
the job is not installed by default). For each new tag it checks out the tagged commit in
a temporary worktree, runs tools/build_desktop.py there, then tools/publish_desktop_release.py
--tag <tag> --yes, which still refuses commits that are not on origin/main. Local only.

    python3 tools/desktop_release_watcher.py --dry-run     # list the tags it would build
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
STATE = Path.home() / 'LargeFiles/fury-desktop/watcher-state.json'
WORKTREES = Path.home() / 'Projects/atv-worktrees'
PATTERN = 'refs/tags/desktop-v*'


def remote_tags(repo):
    """`{tag: commit}` for desktop-v* tags on origin (annotated tags peeled to their commit)."""
    out = subprocess.run(['git', 'ls-remote', '--tags', 'origin', PATTERN], cwd=repo, check=True,
                         capture_output=True, text=True).stdout
    tags = {}
    for line in out.splitlines():
        sha, ref = line.split('\t')
        name = ref[len('refs/tags/'):]
        if name.endswith('^{}'):
            tags[name[:-3]] = sha
        else:
            tags.setdefault(name, sha)
    return tags


def pending(tags, state):
    handled = set(state.get('done', {})) | set(state.get('failed', {}))
    return {tag: sha for tag, sha in sorted(tags.items()) if tag not in handled}


def load_state(path):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return {'done': {}, 'failed': {}}


def notify(message):
    subprocess.run(['osascript', '-e', f'display notification "{message}" with title "Fury desktop release"'],
                   capture_output=True)


def process(repo, tag, sha, log):
    worktree = WORKTREES / f'desktop-release-{tag}'
    subprocess.run(['git', 'fetch', '--quiet', 'origin', 'main', f'refs/tags/{tag}:refs/tags/{tag}'], cwd=repo, check=True)
    subprocess.run(['git', 'worktree', 'add', '--detach', str(worktree), sha], cwd=repo, check=True)
    try:
        os.symlink(Path(repo) / 'private', worktree / 'private')  # the private hosted content package
        for script in (['tools/build_desktop.py'], ['tools/publish_desktop_release.py', '--tag', tag, '--yes']):
            subprocess.run([sys.executable, *script], cwd=worktree, check=True, stdout=log, stderr=subprocess.STDOUT)
    finally:
        subprocess.run(['git', 'worktree', 'remove', '--force', str(worktree)], cwd=repo)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--repo', type=Path, default=ROOT, help='checkout used to talk to origin (never modified)')
    parser.add_argument('--state', type=Path, default=STATE)
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args(argv)
    if os.environ.get('GITHUB_ACTIONS') or os.environ.get('CI'):
        sys.exit('desktop_release_watcher: local machine only, never CI')
    state = load_state(args.state)
    todo = pending(remote_tags(args.repo), state)
    if not todo:
        print('no new desktop-v* tags')
        return 0
    for tag, sha in todo.items():
        print(f'{"would build" if args.dry_run else "building"} {tag} ({sha})', flush=True)
        if args.dry_run:
            continue
        log_path = args.state.parent / f'watcher-{tag}.log'
        log_path.parent.mkdir(parents=True, exist_ok=True)
        try:
            with open(log_path, 'w') as log:
                process(args.repo, tag, sha, log)
            state.setdefault('done', {})[tag] = {'commit': sha, 'at': int(time.time())}
            notify(f'{tag} published')
        except (subprocess.CalledProcessError, OSError) as e:
            # A failed tag is not retried automatically; fix it and delete its entry from the state file.
            state.setdefault('failed', {})[tag] = {'commit': sha, 'at': int(time.time()), 'error': str(e), 'log': str(log_path)}
            notify(f'{tag} failed, see {log_path.name}')
        args.state.parent.mkdir(parents=True, exist_ok=True)
        args.state.write_text(json.dumps(state, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
