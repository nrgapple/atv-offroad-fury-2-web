#!/usr/bin/env python3
"""Package only authored browser files, WASM, and the allowlist HTTP server."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
STATIC_FILES = {'index.html', 'game.css', 'game.js', 'audio.js', 'devlog.html',
                'devlog.css', 'devlog.js', 'workbench.html', 'style.css', 'app.js',
                'worker.js', 'input.js', 'controls.js', 'atv_web.wasm', 'music.js'}


def package(commit, destination):
    if len(commit) != 40 or any(c not in '0123456789abcdef' for c in commit):
        raise ValueError('commit must be a complete lowercase Git SHA')
    if not destination.resolve().is_relative_to((ROOT / 'work').resolve()):
        raise ValueError('release staging must be inside the ignored work directory')
    if destination.resolve() == (ROOT / 'work').resolve():
        raise ValueError('release staging must be a child of work, not work itself')
    if destination.exists():
        shutil.rmtree(destination)
    (destination / 'static').mkdir(parents=True)
    for name in sorted(STATIC_FILES):
        shutil.copy2(ROOT / 'web/dist' / name, destination / 'static' / name)
    shutil.copy2(ROOT / 'tools/serve_web.py', destination / 'serve_web.py')
    (destination / 'static/deployment.json').write_text(json.dumps({'commit': commit}) + '\n')
    hashes = {}
    for path in sorted(destination.rglob('*')):
        if path.is_file():
            hashes[path.relative_to(destination).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    (destination / 'release.json').write_text(json.dumps({'commit': commit, 'sha256': hashes}, indent=2) + '\n')
    print(f'Packaged {len(hashes)} asset-free files for {commit}')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--commit', required=True)
    parser.add_argument('--destination', type=Path, default=ROOT / 'work/release')
    args = parser.parse_args()
    package(args.commit, args.destination)


if __name__ == '__main__':
    main()
