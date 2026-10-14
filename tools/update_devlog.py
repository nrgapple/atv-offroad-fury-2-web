#!/usr/bin/env python3
"""Record a checkpoint; screenshots stay ignored until authorized public Workers/R2 publication."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shutil
import uuid
from devlog import ENTRIES, instant

ROOT = Path(__file__).resolve().parents[1]
def main(argv=None, root=ROOT):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--title', required=True)
    parser.add_argument('--summary', required=True)
    parser.add_argument('--kind', choices=['build', 'discovery', 'issue', 'verification'], default='build')
    parser.add_argument('--detail', action='append', default=[])
    parser.add_argument('--limitation', default='')
    parser.add_argument('--image', type=Path)
    parser.add_argument('--caption', default='')
    parser.add_argument('--timestamp', help='Actual checkpoint timestamp; defaults to screenshot capture time or now')
    parser.add_argument('--status')
    parser.add_argument('--working-on')
    args = parser.parse_args(argv)
    now = datetime.now(timezone.utc).isoformat()
    timestamp = args.timestamp or (datetime.fromtimestamp(args.image.stat().st_mtime, timezone.utc).isoformat() if args.image else now)
    instant(timestamp)
    slug = re.sub('[^a-z0-9]+', '-', args.title.lower()).strip('-')[:54]
    identifier = f'{slug or "checkpoint"}-{uuid.uuid4().hex}'
    entry = {'id': identifier, 'timestamp': timestamp, 'recorded_at': now, 'kind': args.kind,
             'title': args.title, 'summary': args.summary, 'details': args.detail, 'limitation': args.limitation}
    if args.image:
        if args.image.read_bytes()[:8] != b'\x89PNG\r\n\x1a\n':
            parser.error('Checkpoint screenshot must be a PNG')
        images = root / 'private/reports/devlog-images'
        images.mkdir(parents=True, exist_ok=True)
        name = identifier + '.png'
        shutil.copy2(args.image, images / name)
        entry.update(image='/devlog-images/' + name, caption=args.caption or args.title)
    if args.status is not None: entry['status'] = args.status
    if args.working_on is not None: entry['working_on'] = args.working_on
    destination = root / ENTRIES / (identifier + '.json')
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix('.json.tmp')
    try:
        temporary.write_text(json.dumps(entry, indent=2) + '\n')
        # Atomic publication without ever replacing an existing checkpoint.
        os.link(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)
    print('Recorded checkpoint:', destination)

if __name__ == '__main__':
    main()
