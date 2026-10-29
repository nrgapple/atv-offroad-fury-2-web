#!/usr/bin/env python3
"""Build the shared Rust engine (crates/atv-app) for the browser into web/dist.

The same crate builds the desktop apps; here it targets WebGL2 via wgpu. No game
content is included: the page reads the hosted package from /content/.
"""
from pathlib import Path
import hashlib
import os
import shutil
import subprocess
import sys
from host_build import ensure_guard

ensure_guard(__file__, sys.argv[1:])

ROOT = Path(__file__).resolve().parents[1]
cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
bindgen = shutil.which('wasm-bindgen') or str(Path.home() / '.cargo/bin/wasm-bindgen')
target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / 'engine-checkouts' / hashlib.sha256(str(ROOT.resolve()).encode()).hexdigest()[:16]
subprocess.run([cargo, 'rustc', '--locked', '--release', '-p', 'atv-app', '--lib', '--crate-type', 'cdylib', '--target', 'wasm32-unknown-unknown', '--target-dir', str(target)], cwd=ROOT, check=True)
dist = ROOT / 'web/dist'
dist.mkdir(exist_ok=True)
# Flat names keep the release allowlist and cache-busting rules unchanged.
# The name section (function names for stack traces) is most of the raw file;
# panics still report their messages through console_error_panic_hook.
subprocess.run([bindgen, '--target', 'web', '--no-typescript', '--remove-name-section', '--out-dir', str(dist), str(target / 'wasm32-unknown-unknown/release/atv_app.wasm')], cwd=ROOT, check=True)
for name in ['play.html', 'engine.css', 'engine.js', 'decode-worker.js']:
    shutil.copy2(ROOT / 'web' / name, dist / name)
# The pre-switch name of the same page: old links, bookmarks and tests.
shutil.copy2(ROOT / 'web/play.html', dist / 'engine.html')
wasm = dist / 'atv_app_bg.wasm'
if wasm.read_bytes()[:4] != b'\0asm':
    raise ValueError('engine build is not WebAssembly')
print(f'Built Rust engine ({wasm.stat().st_size / 1e6:.2f} MB wasm); game assets are not included.')
