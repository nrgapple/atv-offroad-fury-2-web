#!/usr/bin/env python3
"""Build and publish the desktop archives automatically, from the owner's machine.

One pass per run, meant for the launchd StartInterval job in tools/desktop/ (see
research/DESKTOP_RELEASES.md). Each pass handles:

- every `desktop-v*` tag on origin it has not handled yet, and
- with `--auto-main`, the newest origin/main commit when it changed what goes into the
  apps (RELEASE_PATHS) since the newest desktop release and that release is at least
  `--min-hours` old. Its tag is `desktop-v<version>-<short commit>`.

Each build checks out the commit in a temporary worktree, links `private`, runs
tools/build_desktop.py --smoke, refuses a failed Linux launch, then runs
tools/publish_desktop_release.py --tag <tag> --yes, which still refuses commits that are
not on origin/main. Local only, never CI.

    python3 tools/desktop_release_watcher.py --auto-main --dry-run   # what it would build
"""
import argparse
import fcntl
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import tempfile
import plistlib
from host_build import lease

ROOT = Path(__file__).resolve().parents[1]
OUT_ROOT = Path.home() / 'LargeFiles/fury-desktop'
SERVICE = Path.home() / '.local/share/project-service-data/fury-desktop-release'
STATE = SERVICE / 'watcher-state.json'
WORKTREES = Path.home() / 'Projects/atv-worktrees'
PATTERN = 'refs/tags/desktop-v*'
# What the desktop archives are built from. Documentation, the web shell and tests do not
# start a release on their own.
RELEASE_PATHS = ('crates', 'third_party', 'Cargo.toml', 'Cargo.lock', 'tools/build_desktop.py', 'tools/desktop',
                 'tools/pack_content.py', 'tools/desktop_archive.py', 'tools/build_android.py',
                 'web/fury-app-icons.json')
KEEP_BUILDS = 3
AUTO_RELEASE_HOURS = 7 * 24


def git(repo, *args):
    return subprocess.run(['git', *args], cwd=repo, check=True, capture_output=True, text=True).stdout.strip()


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


def main_candidate(repo, tags, state, min_hours, now=None):
    """`(tag, commit)` for origin/main when it is due for an automatic release, else `(None, reason)`.

    Expects `origin/main` and the tagged commits to be fetched already.
    """
    now = time.time() if now is None else now
    attempt = state.get('last_attempt', {}).get('at', 0)
    attempt = max([attempt, *[e.get('at', 0) for e in state.get('failed', {}).values()]])
    if attempt and now - attempt < min_hours * 3600:
        return None, 'automatic attempt cooldown (includes failures and newer commits)'
    head = git(repo, 'rev-parse', 'origin/main')
    short = git(repo, 'rev-parse', '--short=7', head)
    version = next(line.split('"')[1] for line in git(repo, 'show', f'{head}:Cargo.toml').splitlines()
                   if line.startswith('version = '))
    tag = f'desktop-v{version}-{short}'
    if tag in tags or tag in state.get('done', {}) or tag in state.get('failed', {}):
        return None, f'{tag} is already handled'
    released = [sha for sha in tags.values()
                if subprocess.run(['git', 'merge-base', '--is-ancestor', sha, head], cwd=repo).returncode == 0]
    if released:
        last = max(released, key=lambda sha: int(git(repo, 'log', '-1', '--format=%ct', sha)))
        if not git(repo, 'diff', '--name-only', last, head, '--', *RELEASE_PATHS):
            return None, f'no app changes since {last[:7]}'
        stamps = [int(git(repo, 'log', '-1', '--format=%ct', last))]
        stamps += [entry.get('at', 0) for entry in state.get('done', {}).values()]
        waited = (now - max(stamps)) / 3600
        if waited < min_hours:
            return None, f'last release {waited:.1f} h ago, waiting for {min_hours} h'
    return tag, head


def load_state(path, tags):
    """The state file; a first pass (no file yet) adopts the tags already on origin as done,
    because their releases were published before the watcher was installed."""
    try:
        return json.loads(Path(path).read_text())
    except FileNotFoundError:
        return {'done': {tag: {'commit': sha, 'adopted': True} for tag, sha in tags.items()}, 'failed': {}}
    # Unreadable/corrupt state must stop the pass, never erase release history.



def save_state(path, state):
    """Atomic replacement; a failed write leaves the prior history intact."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix='.watcher-', dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            json.dump(state, stream, indent=2)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def preflight(repo):
    """Check source/SSD/output access in the actual service process before building."""
    if sys.platform == 'darwin':
        info = plistlib.loads(subprocess.check_output(['diskutil', 'info', '-plist', '/Volumes/External SSD']))
        if info.get('MountPoint') != '/Volumes/External SSD' or info.get('VolumeUUID') != '4CC9A66C-3ECB-4EC5-A1AF-221A7A925267':
            raise OSError('Required external SSD is unavailable or has the wrong identity')
    for source in (Path(repo) / 'Cargo.toml', Path(repo) / 'private/reports/hosted-content/manifest.json'):
        with open(source, 'rb') as stream:
            stream.read(1)
    for folder in (OUT_ROOT, WORKTREES):
        folder.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryFile(dir=folder) as stream:
            stream.write(b'preflight')
            stream.flush()
            os.fsync(stream.fileno())


def notify(message):
    subprocess.run(['osascript', '-e', f'display notification "{message}" with title "Fury desktop release"'],
                   capture_output=True)


def verification(release):
    """Release-note text from what the build actually launched."""
    lines = ['Automated build (tools/desktop_release_watcher.py). Every archive passed the member allowlist and an end '
             'to end check of the packed content.']
    for platform, record in sorted(release['archives'].items()):
        smoke = record.get('smoke')
        if smoke:
            where = 'under qemu emulation' if smoke.get('emulated') else 'natively'
            lines.append(f'- {platform}: launched {where} in Docker (Xvfb, Mesa software Vulkan), content verified '
                         f'and the first ride frame captured.')
        else:
            lines.append(f'- {platform}: built and structurally verified, not launched by the automated build.')
    return '\n'.join(lines)


def failed_smokes(release):
    return sorted(p for p, r in release['archives'].items() if r.get('smoke') and not r['smoke'].get('ok'))


def process(repo, tag, sha, log, fetch_tag=True):
    """Build and publish one release. Returns a note when the optional Android APK failed."""
    worktree = WORKTREES / f'desktop-release-{tag}'
    refs = ['main'] + ([f'refs/tags/{tag}:refs/tags/{tag}'] if fetch_tag else [])
    subprocess.run(['git', 'fetch', '--quiet', 'origin', *refs], cwd=repo, check=True)
    subprocess.run(['git', 'worktree', 'add', '--detach', str(worktree), sha], cwd=repo, check=True)
    try:
        os.symlink(Path(repo) / 'private', worktree / 'private')  # the private hosted content package
        run = lambda *script: subprocess.run([sys.executable, *script], cwd=worktree, check=True,  # noqa: E731
                                             stdout=log, stderr=subprocess.STDOUT)
        out = OUT_ROOT / sha[:7]
        run('tools/build_desktop.py', '--smoke', '--output', str(out))
        release = json.loads((out / 'release.json').read_text())
        if failed_smokes(release):
            raise OSError(f'Linux launch failed for {", ".join(failed_smokes(release))}; not publishing')
        # The APK carries no content (it downloads it from R2 on first launch). It is optional:
        # a failed Android build still publishes the desktop archives and is reported.
        android, android_error = out / 'android', None
        try:
            run('tools/build_android.py', '--output', str(android))
        except (subprocess.CalledProcessError, OSError) as e:
            android_error = f'Android build failed: {e}'
            print(android_error, file=log, flush=True)
        extra = [] if android_error else ['--android', str(android)]
        tested = verification(release) + ('' if android_error else
                                         '\n- android-arm64: built, signed and checked against the APK member allowlist '
                                         '(no game content), not launched by the automated build.')
        run('tools/publish_desktop_release.py', '--dir', str(out), '--tag', tag, '--yes', *extra,
            '--verification', tested)
        return android_error
    finally:
        # Preserve unexpected modifications or untracked files; never force removal.
        subprocess.run(['git', 'worktree', 'remove', str(worktree)], cwd=repo)


def prune_builds(state, out_root=OUT_ROOT, keep=KEEP_BUILDS):
    """Delete local archive folders of published releases beyond the newest `keep`.

    Their archives stay on GitHub Releases and can be rebuilt from the commit. Folders of
    unpublished, failed or manual builds are never touched.
    """
    published = {entry['commit'][:7] for entry in state.get('done', {}).values() if entry.get('commit')}
    folders = [p for p in Path(out_root).iterdir()
               if p.is_dir() and re.fullmatch(r'[0-9a-f]{7}', p.name) and p.name in published]
    folders.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    for folder in folders[keep:]:
        shutil.rmtree(folder)
        print(f'removed old local build {folder}')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--repo', type=Path, default=ROOT, help='checkout used to talk to origin (never modified)')
    parser.add_argument('--state', type=Path, default=STATE)
    parser.add_argument('--auto-main', action='store_true', help='also release origin/main when the apps changed')
    parser.add_argument('--min-hours', type=float, default=AUTO_RELEASE_HOURS,
                        help='least time between automatic releases/attempts (default 168: weekly)')
    parser.add_argument('--log', type=Path, help='append output here (launchd cannot open files on the SSD itself)')
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--preflight-only', action='store_true', help='check actual service storage access without building')
    args = parser.parse_args(argv)
    if not math.isfinite(args.min_hours) or args.min_hours <= 0:
        parser.error('--min-hours must be a finite positive number')
    if os.environ.get('GITHUB_ACTIONS') or os.environ.get('CI'):
        sys.exit('desktop_release_watcher: local machine only, never CI')
    if args.log:
        args.log.parent.mkdir(parents=True, exist_ok=True)
        sys.stdout = sys.stderr = open(args.log, 'a', buffering=1)
        print(f'--- {time.strftime("%Y-%m-%d %H:%M:%S")}')
    if args.preflight_only:
        preflight(args.repo)
        print('Storage preflight passed; no build started')
        return 0
    args.state.parent.mkdir(parents=True, exist_ok=True)
    with open(args.state.with_suffix('.lock'), 'a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print('another watcher pass is running')
            return 0
        # Migration is safe only after the previous watcher has exited (installer enforces this).
        legacy = OUT_ROOT / 'watcher-state.json'
        if args.state == STATE and not STATE.exists() and legacy.exists():
            save_state(STATE, load_state(legacy, {}))
        tags = remote_tags(args.repo)
        state = load_state(args.state, tags)
        if state.get('retry_after', 0) > time.time():
            print('release failure backoff is active')
            return 0
        todo = {tag: (sha, True) for tag, sha in pending(tags, state).items()}
        if args.auto_main and not todo:
            subprocess.run(['git', 'fetch', '--quiet', 'origin', 'main', *(f'refs/tags/{t}:refs/tags/{t}' for t in tags)],
                           cwd=args.repo, check=True)
            tag, detail = main_candidate(args.repo, tags, state, args.min_hours)
            if tag:
                todo[tag] = (detail, False)
            else:
                print(f'origin/main: {detail}')
        if not todo:
            print('nothing to build')
            return 0
        # One release per pass: don't drain a backlog or append an automatic release
        # after explicit tags. Remaining tags stay pending for a later admission check.
        for tag, (sha, from_remote) in list(todo.items())[:1]:
            print(f'{"would build" if args.dry_run else "building"} {tag} ({sha})', flush=True)
            if args.dry_run:
                continue
            log_path = args.state.parent / f'watcher-{tag}.log'
            try:
                with lease(wait=0):
                    state['last_attempt'] = {'commit': sha, 'tag': tag, 'at': int(time.time())}
                    save_state(args.state, state)  # durable before any expensive work
                    try:
                        preflight(args.repo)
                        with open(log_path, 'w') as log:
                            android_error = process(args.repo, tag, sha, log, fetch_tag=from_remote)
                        state.setdefault('done', {})[tag] = {'commit': sha, 'at': int(time.time())}
                        state.pop('retry_after', None)
                        if android_error:
                            state['done'][tag]['android_error'] = android_error
                        notify(f'{tag} published' + (' without Android, see ' + log_path.name if android_error else ''))
                    except (subprocess.CalledProcessError, OSError, ValueError) as e:
                        state.setdefault('failed', {})[tag] = {'commit': sha, 'at': int(time.time()),
                                                              'error': str(e), 'log': str(log_path)}
                        state['retry_after'] = int(time.time()) + 3600
                        save_state(args.state, state)
                        notify(f'{tag} failed, see {log_path.name}')
                        return 1
                    save_state(args.state, state)
            except BlockingIOError as exc:
                print(f'release deferred without interrupting active work: {exc}')
                return 0
        if not args.dry_run:
            prune_builds(state)
        return 0

if __name__ == '__main__':
    sys.exit(main())
