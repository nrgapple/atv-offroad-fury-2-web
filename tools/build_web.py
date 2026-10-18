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
# Cargo may treat older source mtimes in another checkout as fresh when target
# directories are shared. Namespace artifacts by the canonical source checkout.
target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / 'web-checkouts' / hashlib.sha256(str(ROOT.resolve()).encode()).hexdigest()[:16]
subprocess.run([cargo, 'build', '--locked', '--release', '-p', 'atv-web', '--target', 'wasm32-unknown-unknown', '--target-dir', str(target)], cwd=ROOT, check=True)
dist = ROOT / 'web/dist'
dist.mkdir(exist_ok=True)
log = read_devlog(ROOT)
for entry in log['entries']:
    entry.pop('image', None)
    entry.pop('caption', None)
(dist / 'devlog.json').write_text(json.dumps(log, indent=2) + '\n')
for name in ['fury-remastered.svg', 'manifest.webmanifest', 'install.js', 'install.css', 'index.html', 'landing.css', 'landing.js', 'play.html', 'design-system.css', 'game.css', 'game.js', 'render-support.js', 'speed-rush.js', 'scenery-materials.js', 'race.js', 'multiplayer.js', 'water.js', 'fog.js', 'train.js', 'wheel-effects.js', 'weather.js', 'ambient-effects.js', 'ambient-streams.js', 'motorcycle-presentation.js', 'graphics.js', 'vehicle-shadows.js', 'tire-tracks.js', 'shadow-map.js', 'lingering-dust.js', 'alpha.js', 'hype.js', 'hype-gl.js', 'keyboard-help.js', 'analytics.js', 'menu-screens.js', 'jukebox.js', 'hud.js', 'minimap.js', 'diagnostics.js', 'frontend.js', 'launch.js', 'touch.js', 'touch.css', 'menu-screens.css', 'jukebox.css', 'hud.css', 'content.js', 'audio.js', 'quad-sounds.js', 'audio-enhance.js', 'music.js', 'wake-lock.js', 'devlog.html', 'devlog.css', 'devlog.js', 'workbench.html', 'style.css', 'app.js', 'worker.js', 'input.js', 'controls.js', 'bindings.js', 'gamepads.js', 'pad-glyphs.js', 'offline.js', 'sw.js']:
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
if [icon['size'] for icon in icons['maskable']] != [192, 512]:
    raise ValueError('unexpected maskable icon sizes')
for icon in icons['maskable']:
    size = icon['size']
    if icon['file'] != f'fury-remastered-maskable-{size}-v1.png':
        raise ValueError('unexpected maskable icon path')
    data = base64.b64decode(icon['base64'], validate=True)
    if data[:8] != b'\x89PNG\r\n\x1a\n' or struct.unpack('>II', data[16:24]) != (size, size) or hashlib.sha256(data).hexdigest() != icon['sha256']:
        raise ValueError('maskable icon bytes do not match metadata')
    (dist / icon['file']).write_bytes(data)
favicon = icons['favicon']
data = base64.b64decode(favicon['base64'], validate=True)
if favicon['file'] != 'favicon.ico' or data[:4] != b'\0\0\1\0' or hashlib.sha256(data).hexdigest() != favicon['sha256']:
    raise ValueError('favicon bytes do not match metadata')
(dist / 'favicon.ico').write_bytes(data)
subprocess.run([shutil.which('node') or 'node', str(ROOT / 'tools/smoke_web_wasm.mjs'), str(dist / 'atv_web.wasm')], cwd=ROOT, check=True)
print(f'Built {dist}; game assets are not included.')
