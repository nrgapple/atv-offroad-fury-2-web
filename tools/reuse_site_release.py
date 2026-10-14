#!/usr/bin/env python3
"""Refresh committed dev-log text in a checked artifact without compiling the game."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from urllib.request import Request, urlopen

from ci_changes import ROOT, changed_paths, classify, output
from devlog import read_devlog
from fetch_site_release import eligible_run, gh_json
from package_release import validate_release

REPOSITORY = 'nrgapple/atv-offroad-fury-2-web'
PUBLIC_URL = 'https://furyremastered.com'


def public_commit(url=PUBLIC_URL):
    request = Request(url.rstrip('/') + '/deployment.json',
                      headers={'User-Agent': 'Fury-release/1.0', 'Cache-Control': 'no-cache'})
    with urlopen(request, timeout=20) as response:
        identity = json.load(response)
    commit = identity.get('commit') if isinstance(identity, dict) else None
    if not isinstance(commit, str) or len(commit) != 40 or any(c not in '0123456789abcdef' for c in commit):
        raise ValueError('invalid public release identity')
    return commit


def refresh(base, destination, commit, root=ROOT):
    metadata = validate_release(base, json.loads((base / 'release.json').read_text())['commit'])
    source = metadata['commit']
    subprocess.run(['git', 'merge-base', '--is-ancestor', source, commit], cwd=root, check=True)
    if classify(changed_paths(root, source, commit)) == 'runtime':
        raise ValueError('runtime changed since the checked artifact; compile normally')
    if (destination.resolve() in (base.resolve(), (root / 'work').resolve())
            or base.resolve().is_relative_to(destination.resolve())
            or not destination.resolve().is_relative_to((root / 'work').resolve())):
        raise ValueError('reuse destination must be a separate ignored work directory')
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(base, destination)
    application = metadata.get('application_commit', source)
    (destination / 'static/deployment.json').write_text(json.dumps({
        'commit': commit, 'application_commit': application}) + '\n')
    log = read_devlog(root, commit)
    for entry in log['entries']:
        entry.pop('image', None)
        entry.pop('caption', None)
    (destination / 'static/devlog.json').write_text(json.dumps(log, indent=2) + '\n')
    hashes = {name: hashlib.sha256((destination / name).read_bytes()).hexdigest()
              for name in metadata['sha256']}
    (destination / 'release.json').write_text(json.dumps({
        'commit': commit, 'application_commit': application, 'reused_from': source,
        'sha256': hashes}, indent=2) + '\n')
    validate_release(destination, commit)


def prepare(lane, event, commit, root=ROOT):
    if lane == 'runtime':
        return {'build': True, 'reused': False}
    if lane == 'docs' or event == 'pull_request':
        return {'build': False, 'reused': False}
    try:
        source = public_commit()
        # Fail closed before downloading an artifact if production is behind code.
        subprocess.run(['git', 'merge-base', '--is-ancestor', source, commit], cwd=root, check=True)
        if classify(changed_paths(root, source, commit)) == 'runtime':
            raise ValueError('production has different runtime inputs')
        from local_release import local_artifact
        base = local_artifact(source, root)
        if base is None:
            runs = gh_json('gh', f'repos/{REPOSITORY}/actions/workflows/ci.yml/runs?branch=main&status=success&per_page=100')
            run = eligible_run(runs['workflow_runs'], source)
            if not run:
                raise ValueError('no successful main artifact for the public release')
            base = root / 'work/reused-base'
            if base.exists():
                shutil.rmtree(base)
            subprocess.run(['gh', 'run', 'download', str(run['id']), '--repo', REPOSITORY,
                            '--name', 'web-release', '--dir', str(base)], check=True, timeout=120)
        refresh(base, root / 'work/release', commit, root)
        print(f'Reused checked application {source}; only dev-log text and release identity changed.')
        return {'build': False, 'reused': True}
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        # Missing/expired artifacts or concurrent runtime merges take the existing
        # full build path; they never lead to deploying a stale application.
        print(f'Checked artifact unavailable for reuse; taking full build path: {error}')
        return {'build': True, 'reused': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lane', required=True, choices=['runtime', 'metadata', 'docs'])
    parser.add_argument('--event', default=os.environ.get('GITHUB_EVENT_NAME', 'pull_request'))
    parser.add_argument('--commit', default=os.environ.get('GITHUB_SHA', 'HEAD'))
    args = parser.parse_args()
    output(prepare(args.lane, args.event, args.commit))
