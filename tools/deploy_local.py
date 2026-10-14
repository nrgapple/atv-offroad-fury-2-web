#!/usr/bin/env python3
"""Checked host deployment fallback. No daemon; run from the release owner's worktree."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time

from package_release import ROOT, package, validate_release
from release_checkpoint import publication_lock, require_owner
from stage_cloudflare import stage
from fetch_site_release import gh_json
from reuse_site_release import public_commit

REPOSITORY = 'nrgapple/atv-offroad-fury-2-web'
URLS = ['https://furyremastered.com/', 'https://atv-offroad-fury-2.champtoday.workers.dev/']


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def require_clean_main(commit):
    if git('status', '--porcelain'):
        raise ValueError('commit all task changes before local publication')
    if git('rev-parse', 'HEAD') != commit or git('rev-parse', 'origin/main') != commit:
        raise ValueError('local publication requires the exact merged origin/main checkout')


def main_runs():
    return gh_json('gh', f'repos/{REPOSITORY}/actions/workflows/ci.yml/runs?branch=main&per_page=100')['workflow_runs']


def cancel_main_runs():
    active = [r for r in main_runs() if r['status'] != 'completed']
    for item in active:
        result = subprocess.run(['gh', 'api', '--method', 'POST', f'repos/{REPOSITORY}/actions/runs/{item["id"]}/cancel'], cwd=ROOT, capture_output=True)
        if result.returncode:
            current = gh_json('gh', f'repos/{REPOSITORY}/actions/runs/{item["id"]}')
            if current['status'] != 'completed':
                raise ValueError(f'could not cancel competing main run {item["id"]}')
    deadline = time.monotonic() + 120
    while any(r['status'] != 'completed' for r in main_runs()):
        if time.monotonic() >= deadline:
            raise ValueError('Actions cancellation remains pending; local deployment lease retained; rerun when terminal')
        print('Waiting for cancelled main runs to become terminal before publishing', flush=True)
        time.sleep(10)


def prefer_actions(commit, stall_seconds=600):
    """Give CI a bounded chance, then take over on failure or no step progress."""
    started = changed = time.monotonic(); signature = None
    while time.monotonic() - started < 1200:
        runs = [r for r in main_runs() if r['head_sha'] == commit]
        if runs:
            item = runs[0]
            if item['status'] == 'completed':
                return item.get('conclusion') == 'success' and public_commit() == commit
            jobs = gh_json('gh', item['jobs_url'])['jobs']
            next_signature = [(j['name'], j['status'], [(s['name'], s['status']) for s in j.get('steps', [])]) for j in jobs]
        else:
            next_signature = []
        if next_signature != signature:
            signature, changed = next_signature, time.monotonic()
            print('Actions progress changed; awaiting checked deployment', flush=True)
        if time.monotonic() - changed >= stall_seconds:
            return False
        time.sleep(20)
    return False


def deploy(commit, checkpoint, owner, reason):
    run('git', 'fetch', 'origin')
    require_clean_main(commit)
    if not gh_json('gh', f'repos/{REPOSITORY}')['private']:
        raise ValueError('source repository must remain private')
    with publication_lock(ROOT):
        require_owner(ROOT, checkpoint, owner)
        variables = gh_json('gh', f'repos/{REPOSITORY}/actions/variables')['variables']
        previous = next((v['value'] for v in variables if v['name'] == 'FURY_DEPLOYMENT_MODE'), None)
        run('gh', 'variable', 'set', 'FURY_DEPLOYMENT_MODE', '--body', 'local')
        # Only release this guard after every competing run is confirmed terminal.
        cancel_main_runs()
        try:
            checks = [
                ['make', 'check'], ['make', 'verify'],
                ['python3', '-m', 'unittest', 'discover', '-s', 'tools', '-p', 'test_security.py'],
                ['python3', '-m', 'unittest', 'discover', '-s', 'tools', '-p', 'test_release*.py'],
                ['node', '--test', *[str(p.relative_to(ROOT)) for p in sorted((ROOT / 'cloudflare').glob('*.test.mjs'))]],
                ['python3', 'tools/check_secrets.py', '--history'],
            ]
            for command in checks:
                print('Local gate: ' + ' '.join(command), flush=True); run(*command)
            from worker_build_cache import VERSION
            worker_build = Path.home() / '.cargo/bin/worker-build'
            if subprocess.check_output([str(worker_build), '--version'], text=True).strip() != VERSION:
                raise ValueError('install the pinned worker-build version before local release')
            run('make', 'web'); run('make', 'relay')
            run('git', 'fetch', 'origin'); require_clean_main(commit)
            run('git', 'merge-base', '--is-ancestor', public_commit(), commit)
            release = ROOT / 'work/local-releases' / commit
            package(commit, release)
            validate_release(release, commit)
            stage(release, commit, preserve_custom_domains=True)
            run('npx', '--yes', 'wrangler@4.149.0', 'deploy', '--config', 'work/cloudflare/wrangler.jsonc')
            for url in URLS:
                run('python3', 'tools/verify_ci_deployment.py', '--url', url, '--release', str(release), '--commit', commit)
            folder = ROOT / 'private/reports/local-releases' / commit
            folder.mkdir(parents=True, exist_ok=True)
            artifact = folder / 'artifact'
            if artifact.exists():
                shutil.rmtree(artifact)
            shutil.copytree(release, artifact)
            receipt = {'commit': commit, 'checkpoint': checkpoint, 'owner': owner, 'reason': reason,
                       'public_verified': True, 'verified_at': datetime.now(timezone.utc).isoformat(),
                       'checks': checks, 'urls': URLS,
                       'manifest_sha256': hashlib.sha256((artifact / 'release.json').read_bytes()).hexdigest()}
            pending = folder / 'receipt.pending.json'
            pending.write_text(json.dumps(receipt, indent=2) + '\n')
            pending.replace(folder / 'receipt.json')
            print(json.dumps({'status': 'locally_deployed_and_verified', 'commit': commit, 'release': str(artifact)}))
        finally:
            if previous is None:
                run('gh', 'variable', 'delete', 'FURY_DEPLOYMENT_MODE')
            else:
                run('gh', 'variable', 'set', 'FURY_DEPLOYMENT_MODE', '--body', previous)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--checkpoint', required=True)
    parser.add_argument('--owner', required=True)
    parser.add_argument('--reason', required=True)
    parser.add_argument('--prefer-actions', action='store_true', help='Fall back after 10 minutes without step progress, 20 minutes total, or a failed run')
    args = parser.parse_args()
    if args.prefer_actions and prefer_actions(args.commit):
        print('Actions deployed the requested commit; proceed with public browser and gallery checks')
        return
    deploy(args.commit, args.checkpoint, args.owner, args.reason)


if __name__ == '__main__':
    main()
