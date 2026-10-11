#!/usr/bin/env python3
"""Validate tracked-file hygiene without requiring the private disc or references."""
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_ROOTS = {'private', 'references', 'work', 'target'}
ASSET_SUFFIXES = {'.iso', '.zip', '.7z', '.irx', '.vag', '.res', '.lsi', '.pss',
                  '.pst', '.elf', '.bin', '.exe', '.wasm', '.png', '.jpg',
                  '.jpeg', '.webp', '.wav', '.ogg', '.mp3'}


def main():
    records = subprocess.check_output(['git', 'ls-files', '--stage', '-z'], cwd=ROOT).decode().split('\0')
    for record in filter(None, records):
        metadata, name = record.split('\t', 1)
        mode, object_id, stage = metadata.split()
        if stage != '0':
            raise ValueError(f'unresolved merge entry: {name}')
        path = Path(name)
        parts = Path(name).parts
        if parts[0] in FORBIDDEN_ROOTS or parts[:2] == ('web', 'dist'):
            raise ValueError(f'private/generated/reference path is tracked: {name}')
        if mode not in ('100644', '100755') or path.suffix.lower() in ASSET_SUFFIXES:
            raise ValueError(f'asset or symlink is tracked: {name}')
        data = subprocess.check_output(['git', 'cat-file', 'blob', object_id], cwd=ROOT)
        if len(data) > 2_000_000 or b'\0' in data:
            raise ValueError(f'large or binary file is tracked: {name}')
        if re.search(rb'-----BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY-----|gh[pousr]_[A-Za-z0-9]{30,}', data):
            raise ValueError(f'possible credential is tracked: {name}')
    print('PASS: tracked files contain no private paths, assets, binaries, symlinks, or recognized credentials')


if __name__ == '__main__':
    main()
