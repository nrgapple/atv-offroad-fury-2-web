#!/usr/bin/env python3
"""Stage a checksum-verified authored release, Rust relay and pinned R2 bindings."""
import argparse
import json
import shutil
from pathlib import Path
from package_release import RELEASE_STATIC_FILES, RELAY_FILES, ROOT, validate_release


def stage(release, commit, release_scoped=False, preserve_custom_domains=False):
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
    if preserve_custom_domains:
        # Existing Cloudflare bindings survive code deployments when omitted.
        # The scoped CI credential must not reprovision account-level DNS/TLS.
        routes = [route for route in config.get('routes', [])
                  if not isinstance(route, dict) or not route.get('custom_domain')]
        if routes:
            config['routes'] = routes
        else:
            config.pop('routes', None)
    config['main'] = str(ROOT / 'cloudflare/entry.mjs')
    config['assets']['directory'] = str(target)
    config.setdefault('vars', {}).update(BUILD_COMMIT=commit, BUILD_WASM_SHA=metadata['sha256']['relay/index_bg.wasm'])
    manifest = json.loads((release / 'static/manifest.webmanifest').read_text())
    icon = next((item['src'].lstrip('/') for item in manifest.get('icons', []) if item.get('sizes') == '192x192' and item['src'].startswith('/app-icons/')), None)
    if icon:
        # Safari also probes its conventional root URL. The linked 180px icon
        # is authoritative; the 192px manifest icon is a valid root fallback.
        config['vars']['APPLE_TOUCH_ICON_KEY'] = icon
    if release_scoped:
        config['vars']['RELEASE_COMMIT'] = commit
    (ROOT / 'work/cloudflare/wrangler.jsonc').write_text(json.dumps(config, indent=2) + '\n')
    print(f'Staged {len(RELEASE_STATIC_FILES)} authored files and Rust relay for {commit}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--release-scoped', action='store_true')
    parser.add_argument('--preserve-custom-domains', action='store_true',
                        help='Keep existing Cloudflare domain bindings without reprovisioning them')
    args = parser.parse_args()
    stage(args.release, args.commit, args.release_scoped, args.preserve_custom_domains)
