#!/usr/bin/env python3
"""Extract one manifest-listed disc file to private/extracted, preserving paths."""
import argparse
import json
from pathlib import Path, PurePosixPath
from import_disc import read_at, SECTOR

ROOT = Path(__file__).resolve().parents[1]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('disc_path', help='Exact ISO path from private/reports/disc-files.json')
    args = ap.parse_args()
    entries = json.loads((ROOT / 'private/reports/disc-files.json').read_text())
    entry = next((e for e in entries if e['path'].upper() == args.disc_path.upper() and not e['directory']), None)
    if entry is None:
        ap.error('file not found in manifest; run make inventory')
    rel = PurePosixPath(entry['path'].lstrip('/'))
    if '..' in rel.parts or rel.is_absolute():
        raise ValueError('unsafe disc path')
    summary = json.loads((ROOT / 'private/reports/import.json').read_text())
    iso = ROOT / 'private/source' / Path(summary['source_archive']).with_suffix('.iso').name
    out = ROOT / 'private/extracted' / rel
    out.parent.mkdir(parents=True, exist_ok=True)
    with iso.open('rb') as src:
        data = read_at(src, entry['lba'] * SECTOR, entry['bytes'])
    if out.exists():
        if out.read_bytes() != data:
            raise ValueError(f'refusing to overwrite a different existing file: {out}')
    else:
        out.write_bytes(data)
    print(f'{entry["path"]} -> {out} ({len(data)} bytes)')

if __name__ == '__main__':
    main()

