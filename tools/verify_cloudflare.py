#!/usr/bin/env python3
"""Verify public deployment identity and every approved R2 object over HTTPS."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
from urllib.request import Request, urlopen

from package_content import ROOT
from package_release import validate_release


def verify(base, commit, release=None):
    release = release or ROOT / 'work/github-releases' / commit
    metadata = validate_release(release, commit)
    def get(path):
        request = Request(base.rstrip('/') + '/' + path.lstrip('/'), headers={'Origin': base.rstrip('/'), 'User-Agent': 'ATV-release-verifier/1.0'})
        with urlopen(request, timeout=90) as response:
            return response.read(), response.headers.get_content_type()

    data, _ = get('/deployment.json')
    if json.loads(data)['commit'] != commit:
        raise ValueError('public deployment is not the expected commit')
    landing, mime = get('/')
    if mime != 'text/html' or b'play.html' not in landing:
        raise ValueError('public landing page failed')
    static_files = 0
    for name, digest in metadata['sha256'].items():
        if not name.startswith('static/') or name == 'static/devlog.json':
            continue  # The authorized R2 overlay adds approved image references.
        body, _ = get(name.removeprefix('static/'))
        if hashlib.sha256(body).hexdigest() != digest:
            raise ValueError(f'public authored asset differs: {name}')
        static_files += 1
    body, _ = get('/multiplayer/version')
    identity = json.loads(body)
    if identity.get('commit') != commit or identity.get('wasmSha256') != metadata['sha256']['relay/index_bg.wasm']:
        raise ValueError('public Rust multiplayer relay differs from checked artifact')
    report = json.loads((ROOT / 'private/reports/cloudflare-upload.json').read_text())
    if report['commit'] != commit:
        raise ValueError('upload receipt belongs to a different commit')
    if any(item['key'] == f'releases/{commit}/manifest.json' for item in report['objects']):
        canonical, _ = get('content/manifest.json')
        pinned, _ = get(f'content/manifests/{commit}.json')
        if canonical != pinned:
            raise ValueError('Worker manifest pin differs from release manifest')
    expected = [item for item in report['objects'] if item['key'] not in ('devlog-gallery.json', f'releases/{commit}/gallery.json')]

    def check(item):
        served_key = 'content/manifest.json' if item['key'] == f'releases/{commit}/manifest.json' else item['key']
        body, mime = get(served_key)
        if len(body) != item['bytes'] or hashlib.sha256(body).hexdigest() != item['sha256']:
            raise ValueError(f'public object differs: {item["key"]}')
        wanted = 'image/png' if item['key'].endswith('.png') else 'application/json' if item['key'].endswith('.json') else 'audio/mp4' if item['key'].endswith('.m4a') else 'application/octet-stream'
        if mime != wanted:
            raise ValueError(f'public object MIME mismatch: {item["key"]}')
        return item['key']

    with ThreadPoolExecutor(max_workers=8) as executor:
        checked = list(executor.map(check, expected))
    data, _ = get('/devlog.json')
    log = json.loads(data)
    authored = json.loads(json.dumps(log))
    for entry in authored['entries']:
        entry.pop('image', None)
        entry.pop('caption', None)
    if authored != json.loads((release / 'static/devlog.json').read_text()):
        raise ValueError('public authored devlog differs from validated CI revision')
    for entry in log['entries']:
        if entry.get('image') and entry['image'].lstrip('/') not in checked:
            raise ValueError('public gallery references an unverified screenshot')
    result = {'url': base, 'commit': commit, 'verified_objects': len(checked), 'log_entries': len(log['entries']),
              'screenshots': sum(bool(entry.get('image')) for entry in log['entries']), 'verified_static_files': static_files}
    (ROOT / 'private/reports/cloudflare-verification.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--release', type=Path)
    args = parser.parse_args()
    if not args.url.startswith('https://'):
        parser.error('public verification requires HTTPS')
    verify(args.url, args.commit, args.release)
