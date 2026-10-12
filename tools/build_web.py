#!/usr/bin/env python3
"""Build a static, private-data-free workbench. No bundler or npm dependencies."""
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
subprocess.run([cargo, 'build', '--locked', '--release', '-p', 'atv-web', '--target', 'wasm32-unknown-unknown', '--target-dir', str(ROOT / 'target')], cwd=ROOT, check=True)
dist = ROOT / 'web/dist'
dist.mkdir(exist_ok=True)
for name in ['index.html', 'landing.css', 'landing.js', 'play.html', 'game.css', 'game.js', 'hud.js', 'frontend.js', 'touch.js', 'touch.css', 'content.js', 'audio.js', 'music.js', 'devlog.html', 'devlog.css', 'devlog.js', 'workbench.html', 'style.css', 'app.js', 'worker.js', 'input.js', 'controls.js', 'bindings.js']:
    shutil.copy2(ROOT / 'web' / name, dist / name)
shutil.copy2(ROOT / 'target/wasm32-unknown-unknown/release/atv_web.wasm', dist / 'atv_web.wasm')
print(f'Built {dist}; game assets are not included.')
