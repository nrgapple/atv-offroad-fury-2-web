"""Validated local release receipts shared by publisher, fetcher and R2 guard."""
import hashlib
import json
import re
from pathlib import Path
from package_release import ROOT, validate_release


def local_artifact(commit, root=ROOT):
    if not re.fullmatch(r'[a-f0-9]{40}', commit):
        raise ValueError('expected full commit')
    folder = root / 'private/reports/local-releases' / commit
    receipt = folder / 'receipt.json'
    if not receipt.exists():
        return None
    record = json.loads(receipt.read_text())
    artifact = folder / 'artifact'
    if record.get('commit') != commit or record.get('public_verified') is not True:
        raise ValueError('local release has not passed public verification')
    validate_release(artifact, commit)
    if hashlib.sha256((artifact / 'release.json').read_bytes()).hexdigest() != record.get('manifest_sha256'):
        raise ValueError('local release receipt does not match artifact')
    if not record.get('checks') or not record.get('owner') or not record.get('checkpoint'):
        raise ValueError('local release receipt lacks verification evidence')
    return artifact
