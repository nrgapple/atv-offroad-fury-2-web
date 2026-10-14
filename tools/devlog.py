"""Assemble historical metadata and independent checkpoints without shared writes."""
from datetime import datetime, timezone
import io
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BASE = 'research/devlog.json'
ENTRIES = 'research/devlog/entries'
OVERRIDES = ('status', 'working_on')


def instant(value):
    timestamp = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if timestamp.tzinfo is None:
        raise ValueError('checkpoint timestamps must include a timezone')
    return timestamp.astimezone(timezone.utc)


def committed_files(root, commit):
    """Read all metadata from one Git tree in two calls, never from dirty files."""
    tree = subprocess.run(['git', 'ls-tree', '-r', '-z', commit, '--', BASE, ENTRIES],
                          cwd=root, check=True, capture_output=True).stdout
    records = []
    for record in filter(None, tree.split(b'\0')):
        metadata, name = record.decode().split('\t', 1)
        if name != BASE and not name.endswith('.json'):
            continue
        mode, kind, oid = metadata.split()
        if kind != 'blob' or mode not in ('100644', '100755'):
            raise ValueError(f'dev-log metadata must be a regular file: {name}')
        records.append((name, oid))
    blobs = subprocess.run(['git', 'cat-file', '--batch'], cwd=root, check=True,
                           input=''.join(oid + '\n' for _, oid in records).encode(),
                           capture_output=True).stdout
    stream = io.BytesIO(blobs)
    files = {}
    for name, oid in records:
        header = stream.readline().decode().split()
        if len(header) != 3 or header[:2] != [oid, 'blob']:
            raise ValueError(f'unreadable committed dev-log file: {name}')
        files[name] = json.loads(stream.read(int(header[2])))
        if stream.read(1) != b'\n':
            raise ValueError('invalid Git blob response')
    return files


def read_devlog(root=ROOT, commit=None):
    root = Path(root)
    if commit is not None:
        files = committed_files(root, commit)
    else:
        paths = [root / BASE, *sorted((root / ENTRIES).rglob('*.json'))]
        files = {}
        for path in paths:
            if path.is_symlink():
                raise ValueError(f'symlinked dev-log metadata: {path}')
            files[path.relative_to(root).as_posix()] = json.loads(path.read_text())
    log = files.pop(BASE)
    identifiers = [entry['id'] for entry in log['entries']]
    if len(set(identifiers)) != len(identifiers):
        raise ValueError('duplicate historical checkpoint id')
    seen = set(identifiers)
    documents = []
    for name, entry in files.items():
        if not re.fullmatch(r'[a-z0-9-]+', entry['id']) or name != f"{ENTRIES}/{entry['id']}.json":
            raise ValueError(f'checkpoint filename must match its id: {name}')
        if entry['id'] in seen:
            raise ValueError(f"duplicate checkpoint id: {entry['id']}")
        seen.add(entry['id'])
        instant(entry['timestamp'])
        documents.append((instant(entry['recorded_at']), entry['id'], entry))
    if not documents:
        return log
    baseline_time = instant(log['updated_at']) if log.get('updated_at') else datetime.min.replace(tzinfo=timezone.utc)
    for recorded, _, entry in sorted(documents):
        log['entries'].append({key: value for key, value in entry.items() if key not in OVERRIDES})
        if recorded >= baseline_time:
            log['updated_at'] = entry['recorded_at']
            for key in OVERRIDES:
                if key in entry:
                    log[key] = entry[key]
    return log
