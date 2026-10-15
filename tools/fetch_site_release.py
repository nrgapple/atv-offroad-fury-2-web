#!/usr/bin/env python3
"""Retrieve a successful main artifact, optionally pinned to the deployed release."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from urllib.request import Request, urlopen

from package_release import validate_release

ROOT = Path(__file__).resolve().parents[1]


def eligible_run(runs, commit):
    return next((run for run in runs if run['head_sha'] == commit
                 and run['head_branch'] == 'main'
                 and run['status'] == 'completed' and run['conclusion'] == 'success'
                 and run['event'] in ('push', 'workflow_dispatch')), None)


def gh_json(gh, endpoint):
    result = subprocess.run([gh, 'api', endpoint], check=True, capture_output=True,
                            text=True, timeout=40)
    return json.loads(result.stdout)


def fetch(repository, site_url=None, deployed=False):
    gh = shutil.which('gh')
    if not gh:
        raise RuntimeError('GitHub CLI is required')
    if deployed:
        from reuse_site_release import public_commit
        commit = public_commit(site_url or 'https://furyremastered.com')
    else:
        commit = gh_json(gh, f'repos/{repository}/commits/main')['sha']
    if not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise ValueError('unexpected main commit identifier')
    if site_url and not deployed:
        try:
            request = Request(site_url.rstrip('/') + '/deployment.json',
                              headers={'User-Agent': 'ATV-Offroad-Fury-2-release-check/1.0'})
            with urlopen(request, timeout=15) as response:
                published = json.load(response)
            if published.get('commit') == commit:
                return {'status': 'unchanged', 'commit': commit}
        except (OSError, ValueError):
            pass  # First publication may not contain deployment metadata yet.
    from local_release import local_artifact
    local = local_artifact(commit, ROOT)
    if local is not None:
        return {'status': 'ready', 'commit': commit, 'release': str(local), 'source': 'verified-local-fallback'}
    runs = gh_json(gh, f'repos/{repository}/actions/workflows/ci.yml/runs?branch=main&status=success&per_page=100')
    run = eligible_run(runs['workflow_runs'], commit)
    if not run:
        return {'status': 'waiting_for_ci', 'commit': commit}
    storage = ROOT / 'work/github-releases'
    storage.mkdir(parents=True, exist_ok=True)
    release = storage / commit
    if not release.exists():
        with tempfile.TemporaryDirectory(prefix='download-', dir=storage) as temporary:
            stage = Path(temporary)
            subprocess.run([gh, 'run', 'download', str(run['id']), '--repo', repository,
                            '--name', 'web-release', '--dir', str(stage)],
                           check=True, timeout=120, capture_output=True, text=True)
            validate_release(stage, commit)
            stage.rename(release)
    validate_release(release, commit)
    if not deployed and gh_json(gh, f'repos/{repository}/commits/main')['sha'] != commit:
        return {'status': 'main_changed_during_download', 'commit': commit}
    return {'status': 'ready', 'commit': commit, 'release': str(release), 'run_url': run['html_url']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repository', default='nrgapple/atv-offroad-fury-2-web')
    parser.add_argument('--deployed', action='store_true', help='Pin the validated live release, even after documentation advances main')
    parser.add_argument('--site-url', help='Literal verified public Site URL; used to skip unchanged releases')
    args = parser.parse_args()
    print(json.dumps(fetch(args.repository, args.site_url, args.deployed)))


if __name__ == '__main__':
    main()
