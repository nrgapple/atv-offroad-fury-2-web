"""Shared fixtures for the tools test suites: Git repositories and content manifests."""
import hashlib
import json
import subprocess

from package_content import SOURCE_SHA


def content(root, records=None):
    """Write a content-addressed runtime manifest fixture and return the manifest."""
    root.mkdir(parents=True, exist_ok=True)
    records = records or {'/STARTAPP.RES': b'first'}
    files = []
    for name, data in records.items():
        digest = hashlib.sha256(data).hexdigest()
        (root / f'{digest}.bin').write_bytes(data)
        files.append({'path': name, 'sha256': digest, 'bytes': len(data), 'url': f'/content/{digest}.bin'})
    manifest = {'version': 1, 'source_build_sha256': SOURCE_SHA, 'files': files}
    (root / 'manifest.json').write_text(json.dumps(manifest))
    return manifest


def git(root, *args):
    return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
