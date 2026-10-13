#!/usr/bin/env python3
"""Private snapshots, conflict-aware integration and resource assembly for Rust queue.

Only referenced allowlisted runtime blobs and reviewed PNGs are copied. No disc,
executable, analysis output, arbitrary directory tree or credential is staged.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
from package_content import validated_content


def git(*args, cwd=None, check=True):
    return subprocess.run(['git', *args], cwd=cwd, check=check, capture_output=True, text=True)


def load(path):
    return json.loads(Path(path).read_text())


def copy_content(source, destination):
    destination.mkdir(parents=True, exist_ok=True)
    for path in validated_content(source):
        shutil.copyfile(path, destination / path.name)
    validated_content(destination)  # Verify the copied bytes, not only source bytes.


def image_name(image):
    if not re.fullmatch(r'/devlog-images/[a-z0-9-]+\.png', image):
        raise ValueError('unapproved screenshot path')
    return Path(image).name


def image_bytes(path):
    if path.is_symlink() or not 8 < path.stat().st_size <= 20_000_000:
        raise ValueError('invalid or symlinked screenshot')
    data = path.read_bytes()
    if data[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('invalid PNG signature')
    return data


def snapshot(source, commit, job, queue, content, images, reviewed=False):
    log = json.loads(git('show', f'{commit}:research/devlog.json', cwd=source).stdout)
    published = queue / 'published'
    target = job / 'resources'
    copy_content(content, target / 'content')
    shutil.copyfile(published / 'content/manifest.json', target / 'base-manifest.json')
    (target / 'images').mkdir()
    for entry in log['entries']:
        if not entry.get('image'):
            continue
        name = image_name(entry['image'])
        previous = published / 'images' / name
        source_image = images / name
        if not source_image.exists():
            source_image = previous
        data = image_bytes(source_image)
        known = previous.exists() and image_bytes(previous) == data
        if not known and not reviewed:
            raise ValueError(f'{name}: inspect new/changed pixels and submit with --reviewed-images')
        # Prevent changing a previously published image URL in-place.
        if previous.exists() and not known:
            raise ValueError('published screenshot bytes are immutable; use a new image name')
        (target / 'images' / name).write_bytes(data)
    (target / 'devlog.json').write_text(json.dumps(log, indent=2) + '\n')
    hashes = {path.relative_to(target).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
              for path in target.rglob('*') if path.is_file()}
    (job / 'resource-hashes.json').write_text(json.dumps(hashes, indent=2) + '\n')
    print(f'Snapshotted {len(hashes)} approved dependency files for {commit}')


def verify_snapshot(job):
    root = job / 'resources'
    expected = load(job / 'resource-hashes.json')
    paths = list(root.rglob('*'))
    if any(p.is_symlink() for p in paths):
        raise ValueError('snapshot contains a symlink')
    actual = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in paths if p.is_file()}
    if actual != expected:
        raise ValueError('queued resource snapshot changed after submission')
    validated_content(root / 'content')


def three_way(base, current, proposed, label):
    if proposed == base:
        return current
    if current == base or current == proposed:
        return proposed
    raise ValueError(f'conflicting queued changes: {label}')


def merge_records(base, current, proposed, key):
    maps = [{record[key]: record for record in records} for records in (base, current, proposed)]
    if any(len(mapping) != len(records) for mapping, records in zip(maps, (base, current, proposed))):
        raise ValueError(f'duplicate {key}')
    keys = dict.fromkeys(record[key] for records in (current, proposed) for record in records)
    result = []
    for name in keys:
        value = three_way(*(mapping.get(name) for mapping in maps), name)
        if value is not None:
            result.append(value)
    return result


def merge_manifest(base, current, proposed):
    result = {}
    for key in set(base) | set(current) | set(proposed):
        result[key] = (merge_records(base[key], current[key], proposed[key], 'path') if key == 'files'
                       else three_way(base.get(key), current.get(key), proposed.get(key), key))
    return result


def merge_devlog(base, current, proposed):
    # Independent entries combine; edits to the same entry must agree.
    result = dict(current)
    result['entries'] = merge_records(base['entries'], current['entries'], proposed['entries'], 'id')
    for key in set(base) | set(current) | set(proposed):
        if key == 'entries':
            continue
        if key == 'updated_at':
            result[key] = max(current.get(key, ''), proposed.get(key, ''))
        else:
            result[key] = three_way(base.get(key), current.get(key), proposed.get(key), key)
    return result


def integrate(commit):
    if git('merge-base', '--is-ancestor', commit, 'HEAD', check=False).returncode == 0:
        return
    ours = git('rev-parse', 'HEAD').stdout.strip()
    base = git('merge-base', ours, commit).stdout.strip()
    result = git('merge', '--no-ff', '--no-commit', commit, check=False)
    if result.returncode:
        conflicts = git('diff', '--name-only', '--diff-filter=U').stdout.splitlines()
        if conflicts != ['research/devlog.json']:
            git('merge', '--abort', check=False)
            raise ValueError(f'code merge conflict; resolve in submitting chat: {conflicts}')
        try:
            logs = [json.loads(git('show', f'{rev}:research/devlog.json').stdout) for rev in (base, ours, commit)]
            Path('research/devlog.json').write_text(json.dumps(merge_devlog(*logs), indent=2) + '\n')
            git('add', 'research/devlog.json')
        except Exception:
            git('merge', '--abort', check=False)
            raise
    git('commit', '-m', f'Integrate queued checkpoint {commit[:12]}')


def prepare(job, queue):
    verify_snapshot(job)
    resources = job / 'resources'
    published = queue / 'published'
    manifest = merge_manifest(load(resources / 'base-manifest.json'),
                              load(published / 'content/manifest.json'),
                              load(resources / 'content/manifest.json'))
    root = Path.cwd() / 'private/reports'
    content = root / 'hosted-content'
    content.mkdir(parents=True, exist_ok=True)
    for source in (published / 'content', resources / 'content'):
        for path in validated_content(source):
            if path.name != 'manifest.json':
                shutil.copyfile(path, content / path.name)
    (content / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    validated_content(content)
    destination = root / 'devlog-images'
    destination.mkdir(exist_ok=True)
    log = load('research/devlog.json')
    if (published / 'upload.json').exists():
        shutil.copyfile(published / 'upload.json', root / 'cloudflare-upload.json')
    for entry in log['entries']:
        if entry.get('image'):
            name = image_name(entry['image'])
            choices = [source / 'images' / name for source in (resources, published)]
            available = [path for path in choices if path.exists()]
            if not available:
                raise ValueError(f'merged checkpoint lacks approved screenshot {name}')
            data = image_bytes(available[0])
            if any(image_bytes(path) != data for path in available):
                raise ValueError(f'screenshot conflict: {name}')
            (destination / name).write_bytes(data)
    print('Merged dependencies validated for integrated checkpoint')


def promote(queue, content=None):
    versions = queue / 'resource-versions'
    versions.mkdir(exist_ok=True)
    version = versions / str(time.time_ns())
    copy_content(content or Path('private/reports/hosted-content'), version / 'content')
    (version / 'images').mkdir()
    log = load('research/devlog.json')
    for entry in log['entries']:
        if entry.get('image'):
            name = image_name(entry['image'])
            (version / 'images' / name).write_bytes(image_bytes(Path('private/reports/devlog-images') / name))
    (version / 'devlog.json').write_text(json.dumps(log, indent=2) + '\n')
    receipt = Path('private/reports/cloudflare-upload.json')
    if receipt.exists():
        shutil.copyfile(receipt, version / 'upload.json')
    pointer = queue / 'published.next'
    pointer.unlink(missing_ok=True)
    pointer.symlink_to(version, target_is_directory=True)
    os.replace(pointer, queue / 'published')


def initialize(queue, content=None):
    # Initial historical gallery must match the existing validated upload receipt.
    report = load('private/reports/cloudflare-upload.json')
    known = {record['key']: record for record in report['objects']}
    for path in validated_content(content or Path('private/reports/hosted-content')):
        key = 'content/' + path.name
        if path.name == 'manifest.json' and key not in known:
            key = f'releases/{report["commit"]}/manifest.json'
        record = known.get(key, {})
        data = path.read_bytes()
        if record.get('sha256') != hashlib.sha256(data).hexdigest() or record.get('bytes') != len(data):
            raise ValueError(f'initial content requires prior verified upload receipt: {path.name}')
    for entry in load('research/devlog.json')['entries']:
        if entry.get('image'):
            name = image_name(entry['image'])
            data = image_bytes(Path('private/reports/devlog-images') / name)
            record = known.get(entry['image'].lstrip('/'), {})
            if record.get('sha256') != hashlib.sha256(data).hexdigest() or record.get('bytes') != len(data):
                raise ValueError(f'initial gallery requires prior approved upload receipt: {name}')
    promote(queue, content)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['snapshot', 'integrate', 'prepare', 'promote', 'initialize'])
    for name in ('source', 'job', 'queue', 'content', 'images'):
        parser.add_argument('--' + name, type=Path)
    parser.add_argument('--commit')
    parser.add_argument('--reviewed-images', action='store_true')
    args = parser.parse_args()
    if args.action == 'snapshot':
        snapshot(args.source, args.commit, args.job, args.queue, args.content, args.images, args.reviewed_images)
    elif args.action == 'integrate':
        integrate(args.commit)
    elif args.action == 'prepare':
        prepare(args.job, args.queue)
    elif args.action == 'promote':
        promote(args.queue)
    else:
        initialize(args.queue, args.content)


if __name__ == '__main__':
    main()
