#!/usr/bin/env python3
"""Stage the public Site from authored build files and approved log screenshots.

Never recursively copy the project or private directory into a public checkout.
The Sites source helper owns source synchronization and publication afterward.
"""
import argparse
import json
from pathlib import Path
import re
import shutil

from package_release import STATIC_FILES, validate_release

ROOT = Path(__file__).resolve().parents[1]
CHECKOUT = ROOT / 'work/sites/atv-port'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--release', type=Path, help='Validated current-main GitHub CI artifact directory')
    parser.add_argument('--commit', help='Expected complete current-main commit from fetch_site_release.py')
    args = parser.parse_args()
    if args.release and not args.commit:
        parser.error('--release requires --commit')
    if args.release:
        validate_release(args.release, args.commit)
    manifest = CHECKOUT / '.openai/hosting.json'
    hosting = json.loads(manifest.read_text())
    if not hosting.get('project_id'):
        raise ValueError('Register the Site once before staging')
    hosting['static'] = {'directory': 'dist'}
    temporary = manifest.with_suffix('.tmp')
    temporary.write_text(json.dumps(hosting, indent=2) + '\n')
    temporary.replace(manifest)
    dist = CHECKOUT / 'dist'
    if dist.exists():
        shutil.rmtree(dist)
    dist.mkdir()
    build = args.release / 'static' if args.release else ROOT / 'web/dist'
    for name in sorted(STATIC_FILES):
        source = build / name
        if source.is_symlink():
            raise ValueError('Refusing symlinked build input')
        shutil.copy2(source, dist / name)
    local_log = json.loads((ROOT / 'research/devlog.json').read_text())
    if args.release:
        log = json.loads((build / 'devlog.json').read_text())
        approved = {entry['id']: entry for entry in local_log['entries']}
        for entry in log['entries']:
            local = approved.get(entry['id'], {})
            authored = {key: value for key, value in local.items() if key not in ('image', 'caption')}
            if entry == authored and local.get('image'):
                entry['image'] = local['image']
                if local.get('caption'):
                    entry['caption'] = local['caption']
        shutil.copy2(build / 'deployment.json', dist / 'deployment.json')
    else:
        log = local_log
    (dist / 'devlog.json').write_text(json.dumps(log, indent=2) + '\n')
    images = dist / 'devlog-images'
    images.mkdir()
    count = 0
    for entry in log['entries']:
        image = entry.get('image')
        if not image:
            continue
        if not re.fullmatch(r'/devlog-images/[a-z0-9-]+\.png', image):
            raise ValueError('Unexpected dev log image reference')
        source = ROOT / 'private/reports/devlog-images' / Path(image).name
        if source.is_symlink() or source.read_bytes()[:8] != b'\x89PNG\r\n\x1a\n':
            raise ValueError('Only inspected PNG checkpoint images are supported')
        shutil.copy2(source, images / source.name)
        count += 1
    print(f'Staged {len(STATIC_FILES)} authored build files, {len(log["entries"])} checkpoints, {count} screenshots')


if __name__ == '__main__':
    main()
