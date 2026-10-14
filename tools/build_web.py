#!/usr/bin/env python3
"""Build a static, private-data-free workbench. No bundler or npm dependencies."""
from pathlib import Path
import os
import base64
import hashlib
import json
from devlog import read_devlog
import struct
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
subprocess.run([cargo, 'build', '--locked', '--release', '-p', 'atv-web', '--target', 'wasm32-unknown-unknown', '--target-dir', str(target)], cwd=ROOT, check=True)
dist = ROOT / 'web/dist'
dist.mkdir(exist_ok=True)
log = read_devlog(ROOT)
for entry in log['entries']:
    entry.pop('image', None)
    entry.pop('caption', None)
(dist / 'devlog.json').write_text(json.dumps(log, indent=2) + '\n')
for name in ['fury-remastered.svg', 'manifest.webmanifest', 'install.js', 'install.css', 'index.html', 'landing.css', 'landing.js', 'play.html', 'design-system.css', 'game.css', 'game.js', 'render-support.js', 'scenery-materials.js', 'race.js', 'multiplayer.js', 'water.js', 'fog.js', 'train.js', 'wheel-effects.js', 'weather.js', 'ambient-effects.js', 'ambient-streams.js', 'motorcycle-presentation.js', 'hud.js', 'minimap.js', 'diagnostics.js', 'frontend.js', 'launch.js', 'touch.js', 'touch.css', 'content.js', 'audio.js', 'music.js', 'devlog.html', 'devlog.css', 'devlog.js', 'workbench.html', 'style.css', 'app.js', 'worker.js', 'input.js', 'controls.js', 'bindings.js']:
    shutil.copy2(ROOT / 'web' / name, dist / name)
shutil.copy2(target / 'wasm32-unknown-unknown/release/atv_web.wasm', dist / 'atv_web.wasm')
icons = json.loads((ROOT / 'web/fury-app-icons.json').read_text())
if icons['source_sha256'] != hashlib.sha256((ROOT / 'web/fury-remastered.svg').read_bytes()).hexdigest():
    raise ValueError('Re-render Home Screen icons after changing the Fury SVG')
if [icon['size'] for icon in icons['icons']] != [180, 192, 512]:
    raise ValueError('unexpected authored icon sizes')
for icon in icons['icons']:
    size = icon['size']
    if icon['file'] != f'fury-remastered-home-{size}-v1.png':
        raise ValueError('unexpected authored icon path')
    data = base64.b64decode(icon['base64'], validate=True)
    if data[:8] != b'\x89PNG\r\n\x1a\n' or struct.unpack('>II', data[16:24]) != (size, size) or hashlib.sha256(data).hexdigest() != icon['sha256']:
        raise ValueError('authored icon bytes do not match metadata')
    (dist / icon['file']).write_bytes(data)
print(f'Built {dist}; game assets are not included.')
