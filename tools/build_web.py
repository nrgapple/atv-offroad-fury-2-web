#!/usr/bin/env python3
"""Copy the page shell (landing, app install, offline, analytics, dev log and
icons) into web/dist. No bundler or npm dependencies. The game itself is the
Rust engine (tools/build_engine.py, which also writes play.html)."""
from pathlib import Path
import base64
import hashlib
import json
from devlog import read_devlog
import struct
import shutil

ROOT = Path(__file__).resolve().parents[1]
# Page-shell files copied as authored (play.html comes from build_engine.py).
SHELL_FILES = ['fury-remastered.svg', 'manifest.webmanifest', 'install.js', 'install.css', 'index.html', 'landing.css', 'landing.js', 'design-system.css', 'analytics.js', 'devlog.html', 'devlog.css', 'devlog.js', 'offline.js', 'sw.js']
dist = ROOT / 'web/dist'
dist.mkdir(exist_ok=True)
log = read_devlog(ROOT)
for entry in log['entries']:
    entry.pop('image', None)
    entry.pop('caption', None)
(dist / 'devlog.json').write_text(json.dumps(log, indent=2) + '\n')
for name in SHELL_FILES:
    shutil.copy2(ROOT / 'web' / name, dist / name)
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
print(f'Built {dist}; game assets are not included.')
