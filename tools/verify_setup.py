#!/usr/bin/env python3
"""Validate the resource handoff, not gameplay or recompilation correctness."""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
from import_disc import digest, elf_report, iso_entries

ROOT = Path(__file__).resolve().parents[1]
REFS = {
    'PS2Recomp': '2c5fbb9389e11dd95693385969490c9e8e6f57b4',
    'ATV-Quadzilla-Recomp': '21b989812bd0d1fbcee60b25ac381ce8f986e8c8',
}

def check(ok, message):
    if not ok:
        raise ValueError(message)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--full', action='store_true', help='Also hash the entire ISO')
    ap.add_argument('--inventory', action='store_true')
    args = ap.parse_args()
    summary = json.loads((ROOT / 'private/reports/import.json').read_text())
    entries = json.loads((ROOT / 'private/reports/disc-files.json').read_text())
    report = json.loads((ROOT / 'private/reports/elf.json').read_text())
    archive = ROOT / 'private/source' / summary['source_archive']
    iso = archive.with_suffix('.iso')
    executable = ROOT / 'private/extracted' / summary['elf']
    check(archive.stat().st_size == 1178996125, 'source ZIP size mismatch')
    check(iso.stat().st_size == summary['iso_bytes'], 'source ISO size mismatch')
    check(digest(executable) == summary['elf_sha256'], 'ELF hash mismatch')
    check(elf_report(executable.read_bytes()) == report, 'ELF report mismatch')
    check(sum(not e['directory'] for e in entries) == summary['file_count'], 'file count mismatch')
    with iso.open('rb') as f:
        volume, actual_entries = iso_entries(f)
    check(actual_entries == entries and volume == summary['volume'], 'ISO manifest mismatch')
    check((ROOT / 'private/extracted/SYSTEM.CNF').read_bytes().decode('ascii') == summary['system_cnf'], 'boot config mismatch')
    if args.full:
        check(digest(iso) == summary['iso_sha256'], 'ISO hash mismatch')
    for name, revision in REFS.items():
        path = ROOT / 'references' / name
        actual = subprocess.check_output(['git','-C',str(path),'rev-parse','HEAD'],text=True).strip()
        check(actual == revision, f'reference revision changed: {name}')
    for name in ('private', 'references', 'work'):
        subprocess.run(['git','-C',str(ROOT),'check-ignore',name+'/probe'],check=True,stdout=subprocess.DEVNULL)
    tracked_private = subprocess.check_output(['git','-C',str(ROOT),'ls-files','private','references','work'],text=True)
    check(not tracked_private.strip(), 'private/generated/reference files are tracked')
    print(f'PASS: {summary["elf"]}, {summary["file_count"]} disc files, matching ELF hash and layout, reference revisions, and Git exclusions')
    if args.full:
        print('PASS: full ISO SHA-256')
    if args.inventory:
        counts = Counter(Path(e['path']).suffix for e in entries if not e['directory'])
        print('Extensions:',dict(counts))
        for e in entries:
            if not e['directory']:
                print(f'{e["bytes"]:12d}  {e["path"]}')
    print('These checks validate project resources only. Use make check and make browser-check for workbench validation; playable gameplay remains unimplemented.')

if __name__ == '__main__':
    main()
