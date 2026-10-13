#!/usr/bin/env python3
"""Redacted credential checks for tracked content and reachable Git history."""
import argparse
from pathlib import Path
import re
import subprocess

PATTERNS = {
    'private key': rb'-----BEGIN (?:RSA |OPENSSH |EC |DSA |ENCRYPTED )?PRIVATE KEY-----',
    'GitHub token': rb'gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}',
    'AWS credential': rb'(?:AKIA|ASIA)[A-Z0-9]{16}',
    'Slack token': rb'xox[baprs]-[A-Za-z0-9-]{20,}',
    'Google API key': rb'AIza[0-9A-Za-z_-]{35}',
    'live payment key': rb'(?:sk|rk)_live_[0-9A-Za-z]{20,}',
    'signed bearer token': rb'eyJ[A-Za-z0-9_-]{12,}\.[A-Za-z0-9_-]{12,}\.[A-Za-z0-9_-]{12,}',
    'URL credential': rb'https?://[^\s\x22\x27/@:]{2,}:[^\s\x22\x27/@]{8,}@',
    'bearer literal': rb'(?i)Bearer[ \t]+[A-Za-z0-9_.-]{30,}',
    'credential literal': rb'(?i)(?:api[_-]?key|api[_-]?token|access[_-]?token|oauth[_-]?token|password|secret[_-]?key|client[_-]?secret)[\x22\x27]?\s*[=:]\s*[\x22\x27][A-Za-z0-9_+/-]{24,}[\x22\x27]',
}
COMPILED = [(name, re.compile(pattern)) for name, pattern in PATTERNS.items()]


def candidates(data):
    return [(name, data[:match.start()].count(b'\n') + 1)
            for name, pattern in COMPILED for match in pattern.finditer(data)]


def scan(history=False):
    if history:
        records = subprocess.check_output(['git', 'rev-list', '--objects', '--all'], text=True).splitlines()
        objects = {line.split(' ', 1)[0]: line.partition(' ')[2] for line in records}
    else:
        paths = subprocess.check_output(['git', 'ls-files', '-z']).decode().split('\0')
        failures = [f'{kind}: {name}:{line} (value redacted)' for name in paths if name
                    for kind, line in candidates(Path(name).read_bytes())]
        if failures:
            raise ValueError('\n'.join(failures))
        print(f'PASS: redacted credential scan of {len([p for p in paths if p])} tracked files')
        return
    failures = []
    with subprocess.Popen(['git', 'cat-file', '--batch'], stdin=subprocess.PIPE, stdout=subprocess.PIPE) as process:
        for oid, path in objects.items():
            process.stdin.write((oid + '\n').encode()); process.stdin.flush()
            header = process.stdout.readline().split()
            data = process.stdout.read(int(header[2])); process.stdout.read(1)
            if header[1] in (b'blob', b'commit', b'tag'):
                failures.extend(f'{kind}: {path or header[1].decode()}:{line} (object {oid[:12]}, value redacted)'
                                for kind, line in candidates(data))
        process.stdin.close(); process.wait()
    if failures:
        raise ValueError('\n'.join(failures))
    print(f'PASS: redacted credential scan of {len(objects)} tracked/history objects')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--history', action='store_true')
    scan(parser.parse_args().history)
