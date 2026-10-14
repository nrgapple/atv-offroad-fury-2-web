#!/usr/bin/env python3
"""Keep delayed Actions runs from replacing a newer local/main release."""
import argparse
import json
import os
from fetch_site_release import gh_json
from reuse_site_release import public_commit
from ci_changes import classify

REPOSITORY = 'nrgapple/atv-offroad-fury-2-web'


def decision(commit, main, live, relation, local_mode=False, main_compatible=False):
    if local_mode:
        return False, 'local release owner holds deployment'
    if commit != main and not main_compatible:
        return False, 'superseded main build'
    if live == commit:
        return False, 'checked commit already deployed'
    if relation == 'behind':
        return False, 'production already contains a newer release'
    if relation not in ('ahead', 'identical'):
        raise ValueError('candidate is not a descendant of production')
    return True, 'current main advances production'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    main_commit = gh_json('gh', f'repos/{REPOSITORY}/commits/main')['sha']
    local_mode = os.environ.get('FURY_DEPLOYMENT_MODE') == 'local'
    live = public_commit()
    relation = 'identical' if live == args.commit else gh_json('gh', f'repos/{REPOSITORY}/compare/{live}...{args.commit}')['status']
    later = gh_json('gh', f'repos/{REPOSITORY}/compare/{args.commit}...{main_commit}') if main_commit != args.commit else None
    compatible = bool(later and later['status'] == 'ahead' and len(later.get('files', [])) < 300
                      and classify([f['filename'] for f in later.get('files', [])]) != 'runtime')
    publish, reason = decision(args.commit, main_commit, live, relation, local_mode, compatible)
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
            stream.write(f'publish={str(publish).lower()}\n')
    print(json.dumps({'publish': publish, 'reason': reason, 'commit': args.commit}))


if __name__ == '__main__':
    main()
