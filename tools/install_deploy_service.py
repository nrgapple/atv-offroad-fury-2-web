#!/usr/bin/env python3
"""Install the local automatic deployment supervisor as a macOS LaunchAgent."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
LABEL = 'com.nrgapple.atv-offroad-fury-2.deploy'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--storage', type=Path, default=Path('/Volumes/External SSD/Developer/Builds/atv-offroad-fury-2-web/deploy'))
    parser.add_argument('--repository', default='nrgapple/atv-offroad-fury-2-web')
    parser.add_argument('--remote-url', default='https://nrgapples-mac-mini.tail96c48c.ts.net:8443/')
    parser.add_argument('--start', action='store_true', help='Start now; stop the foreground development server first')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('the persistent deployment service requires macOS launchd')
    volume = Path('/Volumes/External SSD')
    if not volume.is_mount():
        parser.error('the authorized External SSD is not mounted')
    if not args.storage.resolve().is_relative_to(volume):
        parser.error('deployment artifacts must use the authorized External SSD')
    gh = shutil.which('gh')
    if not gh:
        parser.error('install and authenticate GitHub CLI first')
    info = json.loads(subprocess.check_output([gh, 'api', f'repos/{args.repository}'], text=True))
    if not info['private']:
        parser.error('automatic deployment is restricted to a private repository')
    subprocess.run([gh, 'auth', 'status'], check=True)
    runtime = Path.home() / '.local/share/atv-offroad-fury-2'
    runtime.mkdir(parents=True, exist_ok=True)
    args.storage.mkdir(parents=True, exist_ok=True)
    installed = runtime / 'deploy_watch.py'
    shutil.copy2(ROOT / 'tools/deploy_watch.py', installed)
    config = runtime / 'deploy.json'
    config.write_text(json.dumps({
        'repository': args.repository, 'project': str(ROOT), 'storage': str(args.storage),
        'volume': str(volume), 'gh': gh, 'python': sys.executable,
        'port': 4173, 'remote_url': args.remote_url, 'poll_seconds': 60,
    }, indent=2) + '\n')
    config.chmod(0o600)
    agents = Path.home() / 'Library/LaunchAgents'
    agents.mkdir(exist_ok=True)
    plist = agents / f'{LABEL}.plist'
    plist.write_bytes(plistlib.dumps({
        'Label': LABEL, 'ProgramArguments': [sys.executable, '-u', str(installed), '--config', str(config)],
        'WorkingDirectory': str(runtime), 'RunAtLoad': True, 'KeepAlive': True,
        'ThrottleInterval': 60, 'ExitTimeOut': 20,
        'StandardOutPath': str(runtime / 'service.log'),
        'StandardErrorPath': str(runtime / 'service-error.log'),
        'EnvironmentVariables': {'PATH': os.environ.get('PATH', '/usr/bin:/bin')},
    }))
    if args.start:
        subprocess.run(['launchctl', 'bootout', f'gui/{os.getuid()}', str(plist)], capture_output=True)
        subprocess.run(['launchctl', 'bootstrap', f'gui/{os.getuid()}', str(plist)], check=True)
    print(f'Installed {plist}')
    print('Service starts automatically at user login and retains the last healthy release.')
    print(f'Private remote route: {args.remote_url}')
    if not args.start:
        print('Not started yet; rerun with --start after stopping the foreground server.')


if __name__ == '__main__':
    main()
