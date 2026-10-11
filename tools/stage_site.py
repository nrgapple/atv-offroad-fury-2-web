#!/usr/bin/env python3
"""Stage the public Site from authored build files and approved log screenshots.

Never recursively copy the project or private directory into a public checkout.
The Sites source helper owns source synchronization and publication afterward.
"""
import json
from pathlib import Path
import re
import shutil

from package_release import STATIC_FILES

ROOT = Path(__file__).resolve().parents[1]
CHECKOUT = ROOT / 'work/sites/atv-port'


def main():
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
    for name in sorted(STATIC_FILES):
        source = ROOT / 'web/dist' / name
        if source.is_symlink():
            raise ValueError('Refusing symlinked build input')
        shutil.copy2(source, dist / name)
    log = json.loads((ROOT / 'research/devlog.json').read_text())
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
