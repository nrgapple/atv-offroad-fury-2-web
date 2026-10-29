#!/usr/bin/env python3
"""Install local guard and hand off the desktop watcher only after its old pass exits.

Small runtime/state files stay internal; builds and source remain on the SSD.
The Cargo proxy replacement is opt-in and refuses non-rustup/custom executables.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
SERVICE = Path.home() / '.local/share/project-service-data/fury-desktop-release'
BUDGET = Path.home() / '.local/share/project-service-data/fury-build-budget'
LABEL = 'com.furyremastered.desktop-release'


def cargo_install(tools, home=Path.home()):
    cargo = home / '.cargo/bin/cargo'
    backup = cargo.with_name('cargo.fury-original')
    marker = '# fury host build budget'
    if not (cargo.is_symlink() and os.readlink(cargo) == 'rustup'):
        if not cargo.is_file() or marker not in cargo.read_text():
            raise ValueError('Refusing to replace a Cargo entry point that is not the rustup proxy or our shim')
    runtime = home / '.local/share/project-service-data/fury-build-budget'
    runtime.mkdir(parents=True, exist_ok=True)
    shutil.copy2(tools / 'host_build.py', runtime / 'host_build.py')
    import shlex
    content = '#!/bin/sh\n' + marker + '\nexec /usr/bin/python3 ' + shlex.quote(str(runtime / 'host_build.py')) + ' --cargo-proxy -- "$@"\n'
    if not backup.exists() and not backup.is_symlink():
        if not cargo.is_symlink():
            raise ValueError('Original rustup proxy backup is missing')
        backup.symlink_to(os.readlink(cargo))
    temporary = cargo.with_name('cargo.fury-new')
    temporary.write_text(content)
    temporary.chmod(0o755)
    os.replace(temporary, cargo)


def running_watcher():
    result = subprocess.run(['launchctl', 'print', f'gui/{os.getuid()}/{LABEL}'], capture_output=True, text=True)
    for line in result.stdout.splitlines():
        if line.strip().startswith('pid = '):
            return int(line.strip().split()[-1])
    return None


def finish_handoff(repo):
    """The old service is disabled for new starts, but its active process is untouched."""
    SERVICE.mkdir(parents=True, exist_ok=True)
    with open(SERVICE / 'install.lock', 'a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        while running_watcher():
            time.sleep(10)
        # Also refuse a manually launched old watcher: state must have one writer.
        legacy_lock = Path.home() / 'LargeFiles/fury-desktop/watcher-state.lock'
        with open(legacy_lock, 'a') as old_lock:
            fcntl.flock(old_lock, fcntl.LOCK_EX)  # never kill its holder
            legacy = legacy_lock.with_suffix('.json')
            target = SERVICE / 'watcher-state.json'
            if not target.exists() and legacy.exists():
                data = json.loads(legacy.read_text())
                from desktop_release_watcher import save_state
                save_state(target, data)
            template = (SERVICE / 'runtime/com.furyremastered.desktop-release.plist.in').read_text()
            template = template.replace('@REPO@', str(repo)).replace('@HOME@', str(Path.home()))
            config = plistlib.loads(template.encode())
            destination = Path.home() / 'Library/LaunchAgents' / (LABEL + '.plist')
            temporary = destination.with_suffix('.new')
            temporary.write_bytes(plistlib.dumps(config))
            os.replace(temporary, destination)
            domain = f'gui/{os.getuid()}'
            subprocess.run(['launchctl', 'bootout', domain + '/' + LABEL], capture_output=True)
            subprocess.run(['launchctl', 'enable', domain + '/' + LABEL], check=True)
            subprocess.run(['launchctl', 'bootstrap', domain, str(destination)], check=True)
            (SERVICE / 'handoff.json').write_text(json.dumps({'status': 'installed', 'at': time.time(), 'repo': str(repo)}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--finish-handoff', action='store_true')
    args = parser.parse_args()
    if args.finish_handoff:
        try:
            finish_handoff(args.repo)
        except (OSError, ValueError, subprocess.SubprocessError) as exc:
            (SERVICE / 'handoff.json').write_text(json.dumps({'status': 'blocked', 'error': str(exc), 'at': time.time()}, indent=2))
            raise
        return
    if not args.apply:
        print('Would install a reversible Cargo guard and stage an idle-only watcher handoff. Pass --apply.')
        return
    # Backups only: no live-service restart, no target/cache deletion.
    SERVICE.mkdir(parents=True, exist_ok=True)
    runtime = SERVICE / 'runtime'
    runtime.mkdir(exist_ok=True)
    for name in ['host_build.py', 'desktop_release_watcher.py', 'install_host_build_budget.py']:
        shutil.copy2(ROOT / 'tools' / name, runtime / name)
    shutil.copy2(ROOT / 'tools/desktop/com.furyremastered.desktop-release.plist.in', runtime)
    plist = Path.home() / 'Library/LaunchAgents' / (LABEL + '.plist')
    if plist.exists() and not (SERVICE / 'previous-launchagent.plist').exists():
        shutil.copy2(plist, SERVICE / 'previous-launchagent.plist')
    active = running_watcher()
    if active:
        BUDGET.mkdir(parents=True, exist_ok=True)
        identity = subprocess.check_output(['ps', '-p', str(active), '-o', 'lstart='], text=True).strip()
        (BUDGET / 'grandfather.json').write_text(json.dumps([{'pid': active, 'started': identity}]))
    cargo_install(ROOT / 'tools')
    # disable affects future starts, not the running pass. Reload happens after exit.
    subprocess.run(['launchctl', 'disable', f'gui/{os.getuid()}/{LABEL}'], check=True)
    (SERVICE / 'handoff.json').write_text(json.dumps({'status': 'waiting_for_active_watcher', 'pid': running_watcher()}, indent=2))
    # One-shot launchd-owned installer survives this chat; it is not a recurring monitor.
    handoff = LABEL + '.handoff'
    existing = subprocess.run(['launchctl', 'print', f'gui/{os.getuid()}/{handoff}'], capture_output=True, text=True)
    if not any(line.strip().startswith('pid = ') for line in existing.stdout.splitlines()):
        if existing.returncode == 0:
            subprocess.run(['launchctl', 'remove', handoff], check=True)
        job = {
            'Label': handoff, 'RunAtLoad': True, 'ProcessType': 'Background',
            'ProgramArguments': ['/usr/bin/python3', str(runtime / 'install_host_build_budget.py'),
                                 '--finish-handoff', '--repo', str(args.repo)],
            'StandardOutPath': str(SERVICE / 'handoff.log'),
            'StandardErrorPath': str(SERVICE / 'handoff.log'),
        }
        path = SERVICE / 'handoff.plist'
        path.write_bytes(plistlib.dumps(job))
        try:
            subprocess.run(['launchctl', 'bootstrap', f'gui/{os.getuid()}', str(path)], check=True)
        except subprocess.CalledProcessError:
            subprocess.run(['launchctl', 'enable', f'gui/{os.getuid()}/{LABEL}'], check=True)
            raise
    print('Cargo guard installed. Watcher handoff staged; active pass and live services are untouched.')


if __name__ == '__main__':
    main()
