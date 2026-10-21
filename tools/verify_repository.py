#!/usr/bin/env python3
"""Validate tracked-file hygiene without requiring the private disc or references."""
from pathlib import Path
import hashlib
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_ROOTS = {'private', 'references', 'work', 'target'}
# Binary/media containers plus the original game's resource formats. Text-based
# game formats are listed too, because the binary check alone would miss them.
ASSET_SUFFIXES = {
    # Disc images, executables, archives and media.
    '.iso', '.zip', '.7z', '.irx', '.vag', '.res', '.lsi', '.pss', '.pst', '.elf',
    '.bin', '.exe', '.wasm', '.png', '.jpg', '.jpeg', '.webp', '.wav', '.ogg', '.mp3',
    # Original resource formats found in the game archives.
    '.pb', '.ini', '.phy', '.psm', '.toy', '.mcf', '.sub', '.aud', '.tdf', '.scn',
    '.txt', '.mot', '.cel', '.emt', '.atm', '.prt', '.cub', '.tr2', '.esb', '.vut',
    '.vub', '.vue', '.wtt', '.tv', '.juk', '.bn', '.dat', '.col', '.psg',
}
# Authored files that legitimately use a blocked suffix. Keep this list explicit.
ASSET_SUFFIX_EXCEPTIONS = frozenset({'crates/atv-ui/fonts/OFL.txt'})
# Unmodified SIL OFL fonts bundled for desktop UI text (research/RUST_UI.md),
# pinned by SHA-256 so no other binary can be added under these names.
BUNDLED_FONTS = {
    'crates/atv-ui/fonts/FiraSansCondensed-Regular.ttf': 'c67609f14fc9d328442bfffcc31988b0f76e5575cf460fcc1f3dd8e4912fe257',
    'crates/atv-ui/fonts/FiraSansCondensed-SemiBold.ttf': '5a9586bc549d557c415a6a3dc88658b1453d1df661edf811954ebd55776c01cd',
    'crates/atv-ui/fonts/FiraSansCondensed-Bold.ttf': 'b9371052e58ec5b709153dd0fffc60b5205b4d3435cf957c2522705b31b0dbcc',
    'crates/atv-ui/fonts/FiraSansCondensed-ExtraBold.ttf': 'd1e5181a65e04970ef17b83fa518277658217081a032c7f60b7ac0f101347cf2',
    'crates/atv-ui/fonts/FiraSansCondensed-ExtraBoldItalic.ttf': '071e6c37e90a3ba2fb53ede9f88fd77d024770503665697187013397002b0c66',
}
# Authored files allowed to embed a large base64 payload.
BASE64_ALLOWLIST = frozenset({'web/fury-app-icons.json'})
BASE64_LIMIT = 64 * 1024
BASE64_BLOB = re.compile(rb'[A-Za-z0-9+/]{%d,}={0,2}' % BASE64_LIMIT)
CREDENTIAL = re.compile(rb'-----BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY-----|gh[pousr]_[A-Za-z0-9]{30,}')
# Ghidra-style decompiler output: generated labels, sized placeholder types and
# synthesized local/parameter names. Shareable notes use plain addresses instead.
DECOMPILER_OUTPUT = re.compile(
    rb'\b(?:FUN|DAT|LAB|PTR|SUB)_[0-9a-fA-F]{6,}\b'
    rb'|\bundefined[1248]\b'
    rb'|\b[a-z]{1,3}Var\d+\b'
    rb'|\bparam_\d+\b'
    rb'|\b(?:local|auStack|uStack|iStack|fStack)_[0-9a-fA-F]+\b')
# The checker and its tests necessarily describe these patterns.
DECOMPILER_SCAN_EXEMPT = frozenset({'tools/verify_repository.py', 'tools/test_security.py'})


def check_path(name, mode):
    path = Path(name)
    parts = path.parts
    if parts[0] in FORBIDDEN_ROOTS or parts[:2] == ('web', 'dist'):
        raise ValueError(f'private/generated/reference path is tracked: {name}')
    if mode not in ('100644', '100755'):
        raise ValueError(f'asset or symlink is tracked: {name}')
    if path.suffix.lower() in ASSET_SUFFIXES and name not in ASSET_SUFFIX_EXCEPTIONS:
        raise ValueError(f'asset or game resource is tracked: {name}')


def check_content(name, data):
    if name in BUNDLED_FONTS:
        if hashlib.sha256(data).hexdigest() != BUNDLED_FONTS[name]:
            raise ValueError(f'bundled font does not match its pinned hash: {name}')
        return
    if len(data) > 2_000_000 or b'\0' in data:
        raise ValueError(f'large or binary file is tracked: {name}')
    if CREDENTIAL.search(data):
        raise ValueError(f'possible credential is tracked: {name}')
    if name not in DECOMPILER_SCAN_EXEMPT:
        match = DECOMPILER_OUTPUT.search(data)
        if match:
            line = data.count(b'\n', 0, match.start()) + 1
            raise ValueError(f'possible decompiler output is tracked: {name}:{line}: {match.group().decode()}')
    if name not in BASE64_ALLOWLIST and BASE64_BLOB.search(data):
        raise ValueError(f'large embedded base64 blob is tracked: {name}')


def main():
    records = subprocess.check_output(['git', 'ls-files', '--stage', '-z'], cwd=ROOT).decode().split('\0')
    for record in filter(None, records):
        metadata, name = record.split('\t', 1)
        mode, object_id, stage = metadata.split()
        if stage != '0':
            raise ValueError(f'unresolved merge entry: {name}')
        check_path(name, mode)
        check_content(name, subprocess.check_output(['git', 'cat-file', 'blob', object_id], cwd=ROOT))
    print('PASS: tracked files contain no private paths, assets, game resources, binaries, symlinks, '
          'decompiler output, large base64 blobs, or recognized credentials')


if __name__ == '__main__':
    main()
