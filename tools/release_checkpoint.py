#!/usr/bin/env python3
"""One publication owner and independent milestone receipts; no daemon or queue."""
import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import json
import os
from pathlib import Path
import re
import subprocess

from devlog import instant

ROOT = Path(__file__).resolve().parents[1]
STAGES = ('implementation-ready', 'merged', 'live', 'gallery-complete')


def directory(root, checkpoint):
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,79}', checkpoint):
        raise ValueError('checkpoint must use lowercase letters, digits and hyphens (80 maximum)')
    path = root / 'private/reports/release-checkpoints' / checkpoint
    path.mkdir(parents=True, exist_ok=True)
    return path


@contextmanager
def publication_lock(root=ROOT):
    """All worktrees share private storage; never overlap R2 pointer writes."""
    path = root / 'private/reports/release-publication.lock'
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('a') as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('another release owner is publishing; leave publication to that owner') from None
        try:
            yield
        finally:
            fcntl.flock(stream, fcntl.LOCK_UN)


def write_once(path, record):
    data = (json.dumps(record, indent=2) + '\n').encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'wb') as stream:
        stream.write(data)


def require_owner(root, checkpoint, owner):
    path = directory(root, checkpoint) / 'owner.json'
    record = json.loads(path.read_text())
    if record['owner'] != owner:
        raise ValueError(f'checkpoint publication belongs to {record["owner"]}; do not duplicate it')
    return record


def claim(root, checkpoint, owner):
    if not owner.strip() or len(owner) > 160:
        raise ValueError('provide a short task/thread owner name')
    with publication_lock(root):
        path = directory(root, checkpoint) / 'owner.json'
        if not path.exists():
            write_once(path, {'checkpoint': checkpoint, 'owner': owner,
                              'claimed_at': datetime.now(timezone.utc).isoformat()})
        return require_owner(root, checkpoint, owner)


def record(root, checkpoint, owner, stage, commit, evidence, at=None):
    if stage not in STAGES or not re.fullmatch(r'[a-f0-9]{40}', commit) or not evidence.strip():
        raise ValueError('provide a known stage, full commit and verification evidence')
    with publication_lock(root):
        require_owner(root, checkpoint, owner)
        path = directory(root, checkpoint) / (stage + '.json')
        if path.exists():
            previous = json.loads(path.read_text())
            if previous['commit'] != commit:
                raise ValueError('milestone already recorded; use a new checkpoint for a later release')
            return previous
        timestamp = at or datetime.now(timezone.utc).isoformat()
        moment = instant(timestamp)
        for earlier in STAGES[:STAGES.index(stage)]:
            previous = directory(root, checkpoint) / (earlier + '.json')
            if not previous.exists():
                raise ValueError(f'record {earlier} before {stage}')
            if instant(json.loads(previous.read_text())['timestamp']) > moment:
                raise ValueError('milestone timestamps must be chronological')
        receipt = {'stage': stage, 'timestamp': timestamp, 'commit': commit, 'evidence': evidence}
        write_once(path, receipt)
        return receipt


def status(root, checkpoint):
    folder = directory(root, checkpoint)
    owner = json.loads((folder / 'owner.json').read_text())
    result = dict(owner, milestones={}, elapsed_seconds={})
    previous = None
    for stage in STAGES:
        path = folder / (stage + '.json')
        entry = json.loads(path.read_text()) if path.exists() else None
        result['milestones'][stage] = entry
        if entry:
            if previous:
                result['elapsed_seconds'][stage] = (instant(entry['timestamp']) - instant(previous['timestamp'])).total_seconds()
            previous = entry
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['claim', 'record', 'status'])
    parser.add_argument('--checkpoint', required=True)
    parser.add_argument('--owner')
    parser.add_argument('--stage', choices=STAGES)
    parser.add_argument('--commit')
    parser.add_argument('--evidence')
    parser.add_argument('--pr', type=int, help='For merged, derive SHA and actual mergedAt from GitHub')
    args = parser.parse_args()
    if args.action == 'status':
        result = status(ROOT, args.checkpoint)
    elif not args.owner:
        parser.error('--owner is required')
    elif args.action == 'claim':
        result = claim(ROOT, args.checkpoint, args.owner)
    else:
        at, commit = None, args.commit
        if args.pr:
            if args.stage != 'merged':
                parser.error('--pr only applies to the merged stage')
            pr = json.loads(subprocess.check_output(['gh', 'pr', 'view', str(args.pr),
                                                     '--json', 'mergedAt,mergeCommit'], text=True))
            if not pr.get('mergedAt'):
                parser.error('PR has not merged')
            at, commit = pr['mergedAt'], pr['mergeCommit']['oid']
        if not args.stage or not commit or not args.evidence:
            parser.error('record requires --stage, --commit (or --pr), and --evidence')
        result = record(ROOT, args.checkpoint, args.owner, args.stage, commit, args.evidence, at)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
