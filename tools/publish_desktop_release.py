#!/usr/bin/env python3
"""Publish the desktop archives built by tools/build_desktop.py as a GitHub Release.

    python3 tools/publish_desktop_release.py --dry-run     # every check, no publication
    python3 tools/publish_desktop_release.py               # asks you to type the tag to confirm

The archives embed the game's runtime resources, so this runs only on the owner's
machine (never in CI), only for a clean checkout whose commit is on origin/main, and
only after every archive passes the allowlist and content-pack checks. Releases are
marked prerelease while the port is incomplete. See research/DESKTOP_RELEASES.md.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import desktop_archive  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
OUT_ROOT = Path.home() / 'LargeFiles/fury-desktop'
EXPECTED_PLATFORMS = set(desktop_archive.LAYOUTS)


class PublishError(Exception):
    pass


def git(*args):
    return subprocess.run(['git', *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()


def refuse_in_ci(env=None):
    env = os.environ if env is None else env
    if env.get('GITHUB_ACTIONS') or env.get('CI'):
        raise PublishError('desktop releases embed game content and are published only from the owner\'s machine, never in CI')


def check_source():
    """Clean tree, and HEAD is already on origin/main. Returns the full commit."""
    if git('status', '--porcelain'):
        raise PublishError('the working tree is dirty')
    head = git('rev-parse', 'HEAD')
    try:
        git('fetch', '--quiet', 'origin', 'main')
        git('merge-base', '--is-ancestor', head, 'origin/main')
    except subprocess.CalledProcessError as e:
        raise PublishError('HEAD is not on origin/main (push and merge it first)') from e
    return head


def load_release(directory, head):
    directory = Path(directory)
    try:
        release = json.loads((directory / 'release.json').read_text())
    except (OSError, ValueError) as e:
        raise PublishError(f'{directory} has no readable release.json') from e
    if release.get('dirty') or release.get('commit') != head:
        raise PublishError(f'archives were built from {release.get("commit")} (dirty={release.get("dirty")}), not clean HEAD {head}')
    if set(release.get('archives', {})) != EXPECTED_PLATFORMS:
        raise PublishError(f'expected archives for {sorted(EXPECTED_PLATFORMS)}, found {sorted(release.get("archives", {}))}')
    return release


def check_archives(directory, release, scratch=None):
    directory = Path(directory)
    sums = {}
    for line in (directory / 'SHA256SUMS.txt').read_text().splitlines():
        digest, name = line.split('  ', 1)
        sums[name] = digest
    files = []
    for platform, record in sorted(release['archives'].items()):
        path = directory / record['file']
        if not path.is_file():
            raise PublishError(f'missing archive {path}')
        digest = desktop_archive.sha256(path)
        if digest != record.get('sha256') or sums.get(path.name) != digest:
            raise PublishError(f'checksum mismatch for {path.name}')
        try:
            found, _, _ = desktop_archive.check_contents(path, scratch=scratch)
        except desktop_archive.ArchiveError as e:
            raise PublishError(f'{path.name}: {e}') from e
        if found != platform:
            raise PublishError(f'{path.name} is a {found} archive, expected {platform}')
        files.append(path)
    files.append(directory / 'SHA256SUMS.txt')
    return files


def load_android(directory, head):
    """The APK built by tools/build_android.py for `head`, verified. Returns its path."""
    directory = Path(directory)
    try:
        record = json.loads((directory / 'release.json').read_text())
    except (OSError, ValueError) as e:
        raise PublishError(f'{directory} has no readable Android release.json') from e
    if record.get('commit') != head or str(record.get('short', '')).endswith('-dirty'):
        raise PublishError(f'the APK was built from {record.get("commit")} ({record.get("short")}), not clean HEAD {head}')
    apks = [a for a in record.get('artifacts', []) if a.get('name', '').endswith('.apk')]
    if len(apks) != 1:
        raise PublishError(f'expected one APK in {directory}/release.json')
    path = directory / apks[0]['name']
    if not path.is_file() or desktop_archive.sha256(path) != apks[0].get('sha256'):
        raise PublishError(f'missing APK or checksum mismatch for {path}')
    import build_android
    try:
        build_android.check_apk(path)  # member allowlist: no game content in the APK
    except SystemExit as e:
        raise PublishError(str(e)) from e
    return path, {'file': path.name, 'bytes': path.stat().st_size, 'sha256': apks[0]['sha256']}


def notes(release, verification='', android=None):
    archives = dict(release['archives'])
    if android:
        archives['android-arm64'] = android
    rows = '\n'.join(f'| {p} | `{r["file"]}` | {r["bytes"] / 1e6:.0f} MB | `{r["sha256"][:16]}...` |'
                     for p, r in sorted(archives.items()))
    phone = '''
**Android (arm64, Android 8.0 or newer).** The APK contains no game content: on first launch it downloads the
runtime resources (about 540 MB, once) from furyremastered.com, exactly as the browser does, and plays offline after
that. It is sideloaded: open the APK on the phone and allow installs from that app when asked. Later APKs install over
it (same signing key) and keep the downloaded content.
''' if android else ''
    tested = f'\n## What was tested\n\n{verification.strip()}\n' if verification.strip() else ''
    return f'''**Unofficial, incomplete fan port.** Desktop build of the Fury Remastered Rust port of ATV Offroad Fury 2,
commit `{release['short']}`. It is an early preview, not the finished game.

**Same engine as the browser game.** Since 2026-10-23 https://furyremastered.com/ runs this Rust engine too, so the
desktop apps have the same menus, venues, vehicles, races, HUD, settings and multiplayer, built natively for each
platform. Progress and known gaps: https://furyremastered.com/devlog.html.

Each desktop archive is self-contained: the original game's runtime resources the port needs are packed in, so
nothing else is required (no disc, emulator or install step). Read the `README.txt` inside for controls and options.
{phone}
**Unsigned builds.** They are not code signed or notarized: macOS asks you to Control-click the app and choose Open
(or run `xattr -dr com.apple.quarantine "Fury Remastered.app"`), and Windows SmartScreen asks you to choose
More info, Run anyway.

| Platform | Archive | Size | SHA-256 |
| --- | --- | --- | --- |
{rows}

Full desktop checksums are in `SHA256SUMS.txt`. Built locally from the commit above with `tools/build_desktop.py`
{'and `tools/build_android.py`' if android else ''}.
{tested}'''


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--dir', type=Path, help='build output folder (default: ~/LargeFiles/fury-desktop/<short HEAD>)')
    parser.add_argument('--dry-run', action='store_true', help='run every check and print the gh command without publishing')
    parser.add_argument('--yes', action='store_true', help='do not ask for confirmation')
    parser.add_argument('--stable', action='store_true', help='mark the release as a full release instead of a prerelease')
    parser.add_argument('--verification', default='', help='what was actually launched/tested, added to the release notes')
    parser.add_argument('--tag', help='override the tag (default desktop-v<version>-<short commit>)')
    parser.add_argument('--android', type=Path, metavar='DIR', help='also attach the APK that tools/build_android.py wrote to DIR')
    args = parser.parse_args(argv)
    try:
        refuse_in_ci()
        head = check_source()
        directory = args.dir or OUT_ROOT / git('rev-parse', '--short', 'HEAD')
        release = load_release(directory, head)
        files = check_archives(directory, release, scratch=directory)
        android = None
        if args.android:
            apk, android = load_android(args.android, head)
            files.append(apk)
        tag = args.tag or f'desktop-v{release["version"]}-{release["short"]}'
        if subprocess.run(['gh', 'release', 'view', tag], cwd=ROOT, capture_output=True).returncode == 0:
            raise PublishError(f'release {tag} already exists')
        command = ['gh', 'release', 'create', tag, *[str(f) for f in files], '--target', head,
                   '--title', f'Fury Remastered {"desktop and Android" if android else "desktop"} {release["version"]} ({release["short"]})',
                   '--notes', notes(release, args.verification, android)]
        if not args.stable:
            command.append('--prerelease')
        print(f'checks passed for {len(files) - 1} files from {release["short"]} ({head})')
        if args.dry_run:
            print('dry run, not publishing:\n  ' + ' '.join(c if '\n' not in c else '<notes>' for c in command))
            return 0
        if not args.yes and input(f'type {tag} to publish: ').strip() != tag:
            raise PublishError('not confirmed')
        subprocess.run(command, cwd=ROOT, check=True)
    except PublishError as e:
        print(f'publish_desktop_release: {e}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
