#!/usr/bin/env python3
"""Download a successful PR's authored web build for host tests, without compiling."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from fetch_site_release import gh_json
from package_release import validate_release

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'nrgapple/atv-offroad-fury-2-web'


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def source_identity():
    if git('status', '--porcelain', '--untracked-files=all'):
        raise ValueError('Commit local changes before fetching a PR build')
    return git('rev-parse', 'HEAD'), git('rev-parse', 'HEAD^{tree}')


def require_run(run, head):
    if (run.get('event') != 'pull_request' or run.get('path') != '.github/workflows/ci.yml'
            or run.get('status') != 'completed' or run.get('conclusion') != 'success'
            or run.get('head_sha') != head
            or run.get('repository', {}).get('full_name') != REPOSITORY
            or run.get('head_repository', {}).get('full_name') != REPOSITORY):
        raise ValueError('Need a successful ci.yml PR run from this repository at local HEAD')


def require_tree(commit, head, tree):
    if (commit.get('tree', {}).get('sha') != tree
            or head not in [parent['sha'] for parent in commit.get('parents', [])]):
        raise ValueError('CI merge tree differs from local source; merge origin/main, push, and use the new PR run')


def require_source(built, commit, pr_head, head, tree):
    """Local source is the PR head CI merged, or CI's merge commit itself.

    Busy main often moves while a run builds, so the branch's own tree no longer
    matches the merge CI made. A detached checkout of that merge commit is the
    built source exactly, so it is accepted too."""
    parents = [parent['sha'] for parent in built.get('parents', [])]
    if head == commit and built.get('tree', {}).get('sha') == tree and pr_head in parents:
        return
    try:
        if head != pr_head:
            raise ValueError(f'Local HEAD is neither the PR head {pr_head} nor its CI merge commit')
        require_tree(built, head, tree)
    except ValueError as exc:
        raise ValueError(f'{exc}. Or test the exact build: git fetch origin {commit} && '
                         f'git worktree add --detach <dir> {commit}, then run this there') from exc


def fetch(run_id):
    gh = shutil.which('gh')
    if not gh:
        raise RuntimeError('GitHub CLI is required')
    identity = source_identity()
    run = gh_json(gh, f'repos/{REPOSITORY}/actions/runs/{run_id}')
    pr_head = run.get('head_sha') or ''
    if not re.fullmatch(r'[0-9a-f]{40}', pr_head):
        raise ValueError('Run has no PR head commit')
    require_run(run, pr_head)
    storage = ROOT / 'work/pr-builds'
    storage.mkdir(parents=True, exist_ok=True)
    release = storage / f'{run_id}-{run["run_attempt"]}'
    # Download anew: a failed/partial transfer never becomes a usable artifact.
    with tempfile.TemporaryDirectory(prefix='download-', dir=storage) as temporary:
        stage = Path(temporary)
        subprocess.run([gh, 'run', 'download', str(run_id), '--repo', REPOSITORY,
                        '--name', 'web-release', '--dir', str(stage)],
                       check=True, timeout=120, capture_output=True, text=True)
        commit = json.loads((stage / 'release.json').read_text())['commit']
        if not re.fullmatch(r'[0-9a-f]{40}', commit):
            raise ValueError('Invalid artifact commit')
        validate_release(stage, commit)
        built = gh_json(gh, f'repos/{REPOSITORY}/git/commits/{commit}')
        require_source(built, commit, pr_head, *identity)
        # Detect a push/rerun or local edit while downloading.
        current = gh_json(gh, f'repos/{REPOSITORY}/actions/runs/{run_id}')
        require_run(current, pr_head)
        if current['run_attempt'] != run['run_attempt'] or source_identity() != identity:
            raise ValueError('Run or local source changed during download; retry')
        if release.exists():
            validate_release(release, commit)
        else:
            stage.rename(release)
    return {'status': 'ready', 'head': pr_head, 'commit': commit,
            'root': str(release / 'static'), 'run_url': run['html_url'],
            'access': 'host test files only; no deployment'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=int, required=True, help='Successful Check and build PR run ID')
    args = parser.parse_args()
    try:
        print(json.dumps(fetch(args.run)))
    except (ValueError, RuntimeError, OSError, subprocess.SubprocessError) as exc:
        parser.exit(1, f'{exc}\n')


if __name__ == '__main__':
    main()
