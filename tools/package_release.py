#!/usr/bin/env python3
"""Package and verify authored browser files and WASM for public Sites hosting."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
STATIC_FILES = {'fury-remastered-home-180-v1.png', 'fury-remastered-home-192-v1.png', 'fury-remastered-home-512-v1.png', 'fury-remastered.svg', 'manifest.webmanifest', 'install.js', 'install.css', 'index.html', 'landing.css', 'landing.js', 'play.html', 'design-system.css', 'game.css', 'game.js', 'race.js', 'multiplayer.js', 'water.js', 'fog.js', 'train.js', 'wheel-effects.js', 'motorcycle-presentation.js', 'hud.js', 'minimap.js', 'diagnostics.js', 'frontend.js', 'touch.js', 'touch.css', 'content.js', 'audio.js', 'devlog.html',
                'devlog.css', 'devlog.js', 'workbench.html', 'style.css', 'app.js',
                'worker.js', 'input.js', 'controls.js', 'bindings.js', 'atv_web.wasm', 'music.js'}
RELEASE_STATIC_FILES = STATIC_FILES | {'deployment.json', 'devlog.json'}
RELAY_FILES = {'index.js', 'index_bg.wasm'}
EXPECTED_FILES = {'release.json'} | {'static/' + name for name in RELEASE_STATIC_FILES} | {'relay/' + name for name in RELAY_FILES}


def validate_release(root, expected_commit):
    paths = list(root.rglob('*'))
    if any(path.is_symlink() for path in paths):
        raise ValueError('release contains a symlink')
    actual = {path.relative_to(root).as_posix() for path in paths if path.is_file()}
    if actual != EXPECTED_FILES:
        raise ValueError(f'release file allowlist mismatch: {sorted(actual ^ EXPECTED_FILES)}')
    manifest = json.loads((root / 'release.json').read_text())
    if manifest['commit'] != expected_commit:
        raise ValueError('release commit does not match successful main build')
    if set(manifest['sha256']) != EXPECTED_FILES - {'release.json'}:
        raise ValueError('release checksum list mismatch')
    for name, digest in manifest['sha256'].items():
        path = root / name
        if path.stat().st_size > 20_000_000:
            raise ValueError(f'release file too large: {name}')
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f'release checksum mismatch: {name}')
    if (root / 'relay/index_bg.wasm').read_bytes()[:4] != b'\0asm':
        raise ValueError('invalid relay WebAssembly header')
    if (root / 'static/atv_web.wasm').read_bytes()[:4] != b'\0asm':
        raise ValueError('invalid WebAssembly header')
    if json.loads((root / 'static/deployment.json').read_text())['commit'] != expected_commit:
        raise ValueError('deployment metadata commit mismatch')
    return manifest


def scoped_content_source(source, commit):
    marker = "fetch('/content/manifest.json')"
    if source.count(marker) != 1:
        raise ValueError('expected exactly one hosted manifest fetch to pin')
    return source.replace(marker, f"fetch('/content/manifests/{commit}.json')")


def versioned_source(source, commit):
    """Give the whole CSS/module/Worker/WASM graph a fresh browser cache key.

    Stable filenames are kept in the audited artifact; query versions apply to
    references, including transitive module imports and Worker startup.
    """
    def replace(match):
        quote, relative, name = match.groups()
        if name not in STATIC_FILES:
            return match.group(0)
        return f'{quote}{relative or ""}{name}?v={commit}{quote}'
    return re.sub(r'''(["'])(\./)?([a-z0-9_-]+\.(?:js|css|wasm))\1''', replace, source)


def package(commit, destination, release_scoped=False):
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
        if name.endswith(('.html', '.js', '.css')):
            path = destination / 'static' / name
            path.write_text(versioned_source(path.read_text(), commit))
    if release_scoped:
        source = destination / 'static/content.js'
        source.write_text(scoped_content_source(source.read_text(), commit))
    (destination / 'relay').mkdir()
    for name in sorted(RELAY_FILES):
        shutil.copy2(ROOT / 'crates/atv-relay/build' / name, destination / 'relay' / name)
    (destination / 'static/deployment.json').write_text(json.dumps({'commit': commit}) + '\n')
    # The CI artifact excludes checkpoint screenshots. Public Sites can retain
    # an explicitly authorized gallery in their own separate hosting checkout.
    log = json.loads((ROOT / 'research/devlog.json').read_text())
    for entry in log['entries']:
        entry.pop('image', None)
        entry.pop('caption', None)
    (destination / 'static/devlog.json').write_text(json.dumps(log, indent=2) + '\n')
    hashes = {}
    for path in sorted(destination.rglob('*')):
        if path.is_file():
            hashes[path.relative_to(destination).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    (destination / 'release.json').write_text(json.dumps({'commit': commit, 'sha256': hashes}, indent=2) + '\n')
    validate_release(destination, commit)
    print(f'Packaged {len(hashes)} authored application files for {commit}')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--commit', required=True)
    parser.add_argument('--destination', type=Path, default=ROOT / 'work/release')
    parser.add_argument('--release-scoped', action='store_true')
    args = parser.parse_args()
    package(args.commit, args.destination, args.release_scoped)


if __name__ == '__main__':
    main()
