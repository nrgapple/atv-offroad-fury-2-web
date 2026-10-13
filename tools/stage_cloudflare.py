#!/usr/bin/env python3
"""Stage a checksum-verified asset-free CI release for Workers static assets."""
import argparse
import json
from pathlib import Path
import shutil

from package_release import RELEASE_STATIC_FILES, RELAY_FILES, ROOT, validate_release


def stage(release, commit):
    metadata = validate_release(release, commit)
    target = ROOT / 'work/cloudflare/static'
    if target.exists():
        shutil.rmtree(target)
    target.mkdir(parents=True)
    for name in sorted(RELEASE_STATIC_FILES):
        shutil.copy2(release / 'static' / name, target / name)
    relay = ROOT / 'work/cloudflare/relay'
    if relay.exists():
        shutil.rmtree(relay)
    relay.mkdir(parents=True)
    for name in sorted(RELAY_FILES):
        shutil.copy2(release / 'relay' / name, relay / name)
    config = json.loads((ROOT / 'wrangler.jsonc').read_text())
    config['main'] = str(ROOT / 'cloudflare/entry.mjs')
    config['assets']['directory'] = str(target)
    config.setdefault('vars', {}).update(BUILD_COMMIT=commit, BUILD_WASM_SHA=metadata['sha256']['relay/index_bg.wasm'])
    (ROOT / 'work/cloudflare/wrangler.jsonc').write_text(json.dumps(config, indent=2) + '\n')
    print(f'Staged {len(RELEASE_STATIC_FILES)} asset-free files for {commit}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    stage(args.release, args.commit)
