#!/usr/bin/env python3
"""Stage a checksum-verified asset-free CI release for Workers static assets."""
import argparse
from pathlib import Path
import shutil

from package_release import RELEASE_STATIC_FILES, ROOT, validate_release


def stage(release, commit):
    validate_release(release, commit)
    target = ROOT / 'work/cloudflare/static'
    if target.exists():
        shutil.rmtree(target)
    target.mkdir(parents=True)
    for name in sorted(RELEASE_STATIC_FILES):
        shutil.copy2(release / 'static' / name, target / name)
    print(f'Staged {len(RELEASE_STATIC_FILES)} asset-free files for {commit}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    stage(args.release, args.commit)
