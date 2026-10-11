#!/usr/bin/env python3
"""Upload only validated runtime content and approved screenshots to dedicated R2.

Runs only on the private host, never CI. Reuses Wrangler OAuth in memory without
printing or copying credentials. Uploaded object keys are explicitly allowlisted.
"""
from concurrent.futures import ThreadPoolExecutor
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen

from package_content import ROOT, validated_content
from package_release import validate_release

ACCOUNT = '92b7e16fd24d5fb585f866f5832a032b'
BUCKET = 'atv-offroad-fury-2-content'


def upload(release, commit, force=False):
    validate_release(release, commit)
    if not re.fullmatch(r'[a-f0-9]{40}', commit):
        raise ValueError('expected complete commit SHA')
    current = subprocess.run(['gh', 'api', 'repos/nrgapple/atv-offroad-fury-2-web/commits/main', '--jq', '.sha'],
                             check=True, capture_output=True, text=True).stdout.strip()
    if current != commit:
        raise ValueError('only the validated current-main release may update R2')
    committed = subprocess.run(['git', 'show', f'{commit}:research/devlog.json'], cwd=ROOT,
                               check=True, capture_output=True, text=True)
    log = json.loads(committed.stdout)
    files = validated_content(ROOT / 'private/reports/hosted-content')
    manifest_bytes = (ROOT / 'private/reports/hosted-content/manifest.json').read_bytes()
    uploads = [(f'content/{path.name}', path.read_bytes()) for path in files if path.name != 'manifest.json']
    seen = set()
    for entry in log['entries']:
        image = entry.get('image')
        if not image or image in seen:
            continue
        if not re.fullmatch(r'/devlog-images/[a-z0-9-]+\.png', image):
            raise ValueError('unapproved image reference')
        path = ROOT / 'private/reports/devlog-images' / Path(image).name
        if path.is_symlink():
            raise ValueError('symlinked screenshot')
        data = path.read_bytes()
        if data[:8] != b'\x89PNG\r\n\x1a\n':
            raise ValueError('invalid screenshot')
        uploads.append((image.lstrip('/'), data))
        seen.add(image)
    auth_path = Path.home() / 'Library/Preferences/.wrangler/config/default.toml'
    match = re.search(r'^oauth_token\s*=\s*"([^"\n]+)"', auth_path.read_text(), re.MULTILINE)
    if not match:
        raise RuntimeError('Wrangler OAuth authentication is required')
    token = match.group(1)
    destination = ROOT / 'private/reports/cloudflare-upload.json'
    previous = json.loads(destination.read_text()) if destination.exists() and not force else {}
    known = {item['key']: item for item in previous.get('objects', [])} if previous.get('bucket') == BUCKET else {}

    def put(item):
        key, data = item
        record = {'key': key, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        if all(known.get(key, {}).get(field) == record[field] for field in ('bytes', 'sha256')):
            return dict(record, uploaded=False)
        mime = 'application/json' if key.endswith('.json') else 'image/png' if key.endswith('.png') else 'application/octet-stream'
        url = f'https://api.cloudflare.com/client/v4/accounts/{ACCOUNT}/r2/buckets/{BUCKET}/objects/{quote(key, safe="/")}'
        for attempt in range(4):
            request = Request(url, data=data, method='PUT', headers={
                'Authorization': f'Bearer {token}', 'Content-Type': mime,
                'Cache-Control': 'public, max-age=31536000, immutable' if key.endswith('.bin') else 'no-cache',
            })
            try:
                with urlopen(request, timeout=90) as response:
                    if response.status != 200:
                        raise RuntimeError('R2 upload failed')
                return dict(record, uploaded=True)
            except (HTTPError, OSError):
                if attempt == 3:
                    raise RuntimeError(f'R2 upload failed for {key}') from None
                time.sleep(2 ** attempt)

    report = []
    with ThreadPoolExecutor(max_workers=8) as executor:
        for result in executor.map(put, uploads):
            report.append(result)
            if len(report) % 25 == 0:
                print(f'Uploaded {len(report)}/{len(uploads)} blobs/screenshots', flush=True)
    # Publish pointers last; partial blob upload must never expose a new manifest.
    if (ROOT / 'private/reports/hosted-content/manifest.json').read_bytes() != manifest_bytes:
        raise ValueError('manifest changed during upload; pointers were not published')
    report.append(put(('content/manifest.json', manifest_bytes)))
    report.append(put(('devlog-gallery.json', json.dumps(log).encode())))
    destination.write_text(json.dumps({'bucket': BUCKET, 'commit': commit, 'objects': report}, indent=2) + '\n')
    print(f'Validated {len(report)} approved R2 objects; uploaded {sum(item["uploaded"] for item in report)} changes ({sum(item["bytes"] for item in report)} total bytes)')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--force-upload', action='store_true', help='Repair remote drift by re-uploading all validated objects')
    args = parser.parse_args()
    upload(args.release, args.commit, args.force_upload)
