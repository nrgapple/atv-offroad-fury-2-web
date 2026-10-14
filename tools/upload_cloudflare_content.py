#!/usr/bin/env python3
"""Upload only validated runtime content and approved screenshots to dedicated R2.

Runs only on the private host, never CI. Reuses Wrangler OAuth in memory without
printing or copying credentials. Uploaded object keys are explicitly allowlisted.
"""
from concurrent.futures import ThreadPoolExecutor
import argparse
from devlog import read_devlog
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
from release_checkpoint import publication_lock, require_owner
from reuse_site_release import public_commit
from app_icons import validated_icons

ACCOUNT = '92b7e16fd24d5fb585f866f5832a032b'
BUCKET = 'atv-offroad-fury-2-content'


def upload(release, commit, force=False, content=None, release_scoped=False, checkpoint=None, owner=None):
    with publication_lock(ROOT):
        if checkpoint is not None:
            require_owner(ROOT, checkpoint, owner)
        return _upload(release, commit, force, content, release_scoped)


def require_deployed_release(commit):
    # A later docs-only merge must not invalidate a release already live. Pin to
    # the deployed identity instead of asking every chat to chase current main.
    from fetch_site_release import eligible_run, gh_json
    repository = 'nrgapple/atv-offroad-fury-2-web'
    runs = gh_json('gh', f'repos/{repository}/actions/workflows/ci.yml/runs?branch=main&status=success&per_page=100')
    if not eligible_run(runs['workflow_runs'], commit):
        raise ValueError('publication requires a successful main release')
    if public_commit() != commit:
        raise ValueError('release is no longer deployed; the current release owner must publish its package')


def _upload(release, commit, force=False, content=None, release_scoped=False):
    content = (content or ROOT / 'private/reports/hosted-content').resolve()
    if not content.is_relative_to((ROOT / 'private').resolve()):
        raise ValueError('runtime package must remain under private storage')
    validate_release(release, commit)
    if not re.fullmatch(r'[a-f0-9]{40}', commit):
        raise ValueError('expected complete commit SHA')
    require_deployed_release(commit)
    log = read_devlog(ROOT, commit)
    icon_records = json.loads(subprocess.run(['git', 'show', f'{commit}:web/app-icon-assets.json'], cwd=ROOT,
                              check=True, capture_output=True, text=True).stdout)
    files = validated_content(content)
    manifest_bytes = (content / 'manifest.json').read_bytes()
    uploads = [(f'content/{path.name}', path.read_bytes()) for path in files if path.name != 'manifest.json']
    icon_directory = (ROOT / 'private/reports/app-icons').resolve()
    if not icon_directory.is_relative_to((ROOT / 'private').resolve()):
        raise ValueError('app icons must remain in private storage')
    uploads.extend(validated_icons(icon_records, icon_directory))
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
                print(f'Checked {len(report)}/{len(uploads)} blobs/screenshots', flush=True)
    # Publish pointers last; partial blob upload must never expose a new manifest.
    if (content / 'manifest.json').read_bytes() != manifest_bytes:
        raise ValueError('manifest changed during upload; pointers were not published')
    require_deployed_release(commit)
    manifest_key = f'releases/{commit}/manifest.json' if release_scoped else 'content/manifest.json'
    gallery_key = f'releases/{commit}/gallery.json' if release_scoped else 'devlog-gallery.json'
    report.append(put((manifest_key, manifest_bytes)))
    report.append(put((gallery_key, json.dumps(log).encode())))
    if release_scoped:
        # Storage is independent of Worker versions. Verify upload bytes before
        # selecting them with a Worker deployment; local receipts are only hints.
        def verify_object(item):
            key = item['key']
            url = f'https://api.cloudflare.com/client/v4/accounts/{ACCOUNT}/r2/buckets/{BUCKET}/objects/{quote(key, safe="/")}'
            request = Request(url, headers={'Authorization': f'Bearer {token}'})
            with urlopen(request, timeout=90) as response:
                data = response.read()
            if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
                raise ValueError(f'R2 staged object differs: {key}; retry with --force-upload to repair')
        with ThreadPoolExecutor(max_workers=8) as executor:
            list(executor.map(verify_object, report))
    destination.write_text(json.dumps({'bucket': BUCKET, 'commit': commit, 'objects': report}, indent=2) + '\n')
    print(f'Validated {len(report)} approved R2 objects; uploaded {sum(item["uploaded"] for item in report)} changes ({sum(item["bytes"] for item in report)} total bytes)')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--force-upload', action='store_true', help='Repair remote drift by re-uploading all validated objects')
    parser.add_argument('--release-scoped', action='store_true', help='Stage immutable release manifests before Worker deployment')
    parser.add_argument('--content', type=Path, help='Validated private runtime package')
    parser.add_argument('--checkpoint', required=True, help='Combined checkpoint claimed by its single release owner')
    parser.add_argument('--owner', required=True)
    args = parser.parse_args(argv)
    upload(args.release, args.commit, args.force_upload, content=args.content, release_scoped=args.release_scoped,
           checkpoint=args.checkpoint, owner=args.owner)


if __name__ == '__main__':
    main()
