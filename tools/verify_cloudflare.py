#!/usr/bin/env python3
"""Verify public deployment identity and every approved R2 object over HTTPS."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from urllib.request import Request, urlopen

from package_content import ROOT


def verify(base, commit):
    def get(path):
        request = Request(base.rstrip('/') + '/' + path.lstrip('/'), headers={'User-Agent': 'ATV-release-verifier/1.0'})
        with urlopen(request, timeout=90) as response:
            return response.read(), response.headers.get_content_type()

    data, _ = get('/deployment.json')
    if json.loads(data)['commit'] != commit:
        raise ValueError('public deployment is not the expected commit')
    landing, mime = get('/')
    if mime != 'text/html' or b'play.html' not in landing:
        raise ValueError('public landing page failed')
    report = json.loads((ROOT / 'private/reports/cloudflare-upload.json').read_text())
    expected = [item for item in report['objects'] if item['key'] != 'devlog-gallery.json']

    def check(item):
        body, mime = get(item['key'])
        if len(body) != item['bytes'] or hashlib.sha256(body).hexdigest() != item['sha256']:
            raise ValueError(f'public object differs: {item["key"]}')
        wanted = 'image/png' if item['key'].endswith('.png') else 'application/json' if item['key'].endswith('.json') else 'application/octet-stream'
        if mime != wanted:
            raise ValueError(f'public object MIME mismatch: {item["key"]}')
        return item['key']

    with ThreadPoolExecutor(max_workers=8) as executor:
        checked = list(executor.map(check, expected))
    data, _ = get('/devlog.json')
    log = json.loads(data)
    for entry in log['entries']:
        if entry.get('image') and entry['image'].lstrip('/') not in checked:
            raise ValueError('public gallery references an unverified screenshot')
    result = {'url': base, 'commit': commit, 'verified_objects': len(checked), 'log_entries': len(log['entries']),
              'screenshots': sum(bool(entry.get('image')) for entry in log['entries'])}
    (ROOT / 'private/reports/cloudflare-verification.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    if not args.url.startswith('https://'):
        parser.error('public verification requires HTTPS')
    verify(args.url, args.commit)
