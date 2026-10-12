#!/usr/bin/env python3
"""Record a checkpoint; screenshots stay ignored until authorized public Workers/R2 publication."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import shutil
import uuid

ROOT = Path(__file__).resolve().parents[1]
LOG = ROOT / 'research/devlog.json'
IMAGES = ROOT / 'private/reports/devlog-images'

def main():
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
    args = parser.parse_args()
    now = datetime.now(timezone.utc).isoformat()
    log = json.loads(LOG.read_text()) if LOG.exists() else {'status': 'The faithful port is in progress. This is not the complete game.', 'working_on': '', 'entries': []}
    timestamp = args.timestamp or (datetime.fromtimestamp(args.image.stat().st_mtime, timezone.utc).isoformat() if args.image else now)
    datetime.fromisoformat(timestamp.replace('Z', '+00:00'))
    slug = re.sub('[^a-z0-9]+', '-', args.title.lower()).strip('-')[:54]
    identifier = f'{slug}-{uuid.uuid4().hex[:8]}'
    entry = {'id': identifier, 'timestamp': timestamp, 'recorded_at': now, 'kind': args.kind,
             'title': args.title, 'summary': args.summary, 'details': args.detail, 'limitation': args.limitation}
    if args.image:
        if args.image.read_bytes()[:8] != b'\x89PNG\r\n\x1a\n':
            parser.error('Checkpoint screenshot must be a PNG')
        IMAGES.mkdir(parents=True, exist_ok=True)
        name = identifier + '.png'
        shutil.copy2(args.image, IMAGES / name)
        entry.update(image='/devlog-images/' + name, caption=args.caption or args.title)
    log['entries'].append(entry)
    log['updated_at'] = now
    if args.status: log['status'] = args.status
    if args.working_on: log['working_on'] = args.working_on
    LOG.parent.mkdir(parents=True, exist_ok=True)
    temporary = LOG.with_suffix('.json.tmp')
    temporary.write_text(json.dumps(log, indent=2) + '\n')
    temporary.replace(LOG)
    print('Recorded checkpoint:', args.title)

if __name__ == '__main__':
    main()
