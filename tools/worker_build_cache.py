#!/usr/bin/env python3
"""Cache only the pinned deployment executable and its compatibility record."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import time

VERSION = '0.8.7'
SCHEMA = 1


def command(*args):
    return subprocess.check_output(args, text=True, timeout=30).strip()


def identity():
    runner_os = os.environ.get('RUNNER_OS', platform.system())
    runner_arch = os.environ.get('RUNNER_ARCH', platform.machine())
    for value in (runner_os, runner_arch):
        if not re.fullmatch(r'[A-Za-z0-9_.-]+', value):
            raise ValueError('invalid runner identity')
    return {'schema': SCHEMA, 'worker_build': VERSION, 'os': runner_os,
            'arch': runner_arch, 'rustc': command('rustc', '-vV'),
            'cargo': command('cargo', '--version')}


def cache_key(stamp):
    digest = hashlib.sha256(json.dumps(stamp, sort_keys=True).encode()).hexdigest()
    return f'fury-worker-build-v{SCHEMA}-{stamp["os"]}-{stamp["arch"]}-{VERSION}-{digest}'


def safe_root(root):
    if root.is_symlink() or (root / 'bin').is_symlink():
        raise ValueError('tool cache directory must not be symlinked')


def valid(root, stamp):
    safe_root(root)
    binary, record = root / 'bin/worker-build', root / 'identity.json'
    if binary.is_symlink() or record.is_symlink() or not binary.is_file() or not record.is_file():
        return False
    try:
        metadata = json.loads(record.read_text())
        if not isinstance(metadata, dict) or metadata.get('identity') != stamp or metadata.get('sha256') != hashlib.sha256(binary.read_bytes()).hexdigest():
            return False
        # Version is checked only after provenance fields and bytes match.
        return os.access(binary, os.X_OK) and command(str(binary), '--version') == VERSION
    except (OSError, ValueError, subprocess.SubprocessError):
        return False


def prepare(root, stamp, exact_hit=False):
    started = time.monotonic()
    safe_root(root)
    source = 'cache' if exact_hit and valid(root, stamp) else 'cold'
    binary, record = root / 'bin/worker-build', root / 'identity.json'
    if source == 'cold':
        root.mkdir(parents=True, exist_ok=True)
        # Remove only the two owned cache files, including invalid file symlinks.
        for path in (binary, record):
            path.unlink(missing_ok=True)
        env = dict(os.environ, CARGO_TARGET_DIR=str(root.parent / 'fury-worker-build-target'))
        subprocess.run(['cargo', 'install', 'worker-build', '--version', VERSION,
                        '--locked', '--force', '--root', str(root)], env=env, check=True)
        if binary.is_symlink() or not binary.is_file() or command(str(binary), '--version') != VERSION:
            raise ValueError('cold-installed worker-build has an unexpected version')
        record.write_text(json.dumps({'identity': stamp, 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}, indent=2) + '\n')
        if not valid(root, stamp):
            raise ValueError('cold-installed worker-build did not validate')
    return {'source': source, 'seconds': round(time.monotonic() - started, 3),
            'key': cache_key(stamp), 'version': VERSION, 'sha256': json.loads(record.read_text())['sha256']}


def output(values):
    path = os.environ.get('GITHUB_OUTPUT')
    if path:
        with open(path, 'a') as stream:
            for key, value in values.items():
                stream.write(f'{key}={value}\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['key', 'prepare'])
    parser.add_argument('--root', type=Path)
    parser.add_argument('--cache-hit', default='', choices=['', 'true', 'false'])
    args = parser.parse_args()
    stamp = identity()
    if args.mode == 'key':
        output({'key': cache_key(stamp)})
        print(cache_key(stamp))
        return
    if args.root is None:
        parser.error('prepare requires --root')
    result = prepare(args.root.absolute(), stamp, args.cache_hit == 'true')
    output(result)
    print(json.dumps(result))
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        with open(summary, 'a') as stream:
            stream.write(f'\nWorker build tool: **{result["source"]}**, {result["seconds"]}s; version {VERSION}.\n\nCache key: `{result["key"]}`. Binary SHA-256: `{result["sha256"]}`.\n')


if __name__ == '__main__':
    main()
