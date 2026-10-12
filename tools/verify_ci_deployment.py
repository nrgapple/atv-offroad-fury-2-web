#!/usr/bin/env python3
"""Verify public authored deployment using only the asset-free CI artifact."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import time
from urllib.request import Request, urlopen

from package_release import validate_release


def verify(base, release, commit):
    metadata = validate_release(release, commit)

    def get(path):
        request = Request(base.rstrip('/') + '/' + path.lstrip('/'),
                          headers={'Origin': base.rstrip('/'), 'User-Agent': 'ATV-CI-deployment-verifier/1.0',
                                   'Cache-Control': 'no-cache'})
        with urlopen(request, timeout=20) as response:
            return response.read(), response.headers.get_content_type()

    body, _ = get('deployment.json')
    if json.loads(body).get('commit') != commit:
        raise ValueError('public deployment is not the checked commit')
    landing, mime = get('/')
    if mime != 'text/html' or b'play.html' not in landing:
        raise ValueError('public landing route failed')

    def check(item):
        name, digest = item
        body, _ = get(name.removeprefix('static/'))
        if hashlib.sha256(body).hexdigest() != digest:
            raise ValueError(f'public authored asset differs: {name}')

    assets = [(name, digest) for name, digest in metadata['sha256'].items()
              if name.startswith('static/') and name != 'static/devlog.json']
    with ThreadPoolExecutor(max_workers=6) as executor:
        list(executor.map(check, assets))
    relay, _ = get('multiplayer/version')
    identity = json.loads(relay)
    if identity.get('commit') != commit or identity.get('wasmSha256') != metadata['sha256']['relay/index_bg.wasm']:
        raise ValueError('public Rust multiplayer relay differs from checked artifact')
    body, mime = get('devlog.json')
    if mime != 'application/json':
        raise ValueError('public dev log has incorrect MIME type')
    log = json.loads(body)
    for entry in log['entries']:
        entry.pop('image', None)
        entry.pop('caption', None)
    if log != json.loads((release / 'static/devlog.json').read_text()):
        raise ValueError('public dev-log text differs from checked artifact')
    return {'url': base, 'commit': commit, 'verified_static_files': len(assets),
            'log_entries': len(log['entries'])}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--attempts', type=int, default=6)
    args = parser.parse_args()
    if not args.url.startswith('https://') or args.attempts < 1:
        parser.error('requires an HTTPS URL and at least one attempt')
    for attempt in range(args.attempts):
        try:
            result = verify(args.url, args.release, args.commit)
            print(json.dumps(result))
            break
        except (OSError, ValueError) as error:
            if attempt + 1 == args.attempts:
                raise
            print(f'Public verification attempt {attempt + 1} failed: {error}; retrying', flush=True)
            time.sleep(10)
