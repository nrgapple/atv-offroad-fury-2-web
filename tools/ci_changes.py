#!/usr/bin/env python3
"""Choose a cheap metadata lane only for an explicit documentation allowlist."""
import argparse
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def metadata_path(name):
    return (name in {'README.md', 'HANDOFF.md', 'AGENTS.md', 'research/devlog.json'}
            or (name.startswith('research/') and name.endswith('.md'))
            or (name.startswith('research/devlog/entries/') and name.endswith('.json')))


def changed_paths(root, base, head):
    return subprocess.check_output(['git', 'diff', '--no-renames', '--name-only', '-z',
                                    base, head, '--'], cwd=root).decode().split('\0')[:-1]


def classify(paths):
    if any(not metadata_path(name) for name in paths):
        return 'runtime'
    if any(name == 'research/devlog.json' or name.startswith('research/devlog/entries/') for name in paths):
        return 'metadata'
    return 'docs'


def output(values):
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
            for key, value in values.items():
                stream.write(f'{key}={str(value).lower() if isinstance(value, bool) else value}\n')
    print(json.dumps(values))


def plan(event, event_name, root=ROOT, head='HEAD'):
    if event_name == 'workflow_dispatch':
        return {'lane': 'runtime', 'publish': True}
    base = event.get('pull_request', {}).get('base', {}).get('sha') if event_name == 'pull_request' else event.get('before')
    if not base or set(base) == {'0'}:
        return {'lane': 'runtime', 'publish': True}
    # For a PR, HEAD is GitHub's merge tree: include the result actually tested.
    lane = classify(changed_paths(root, base, head))
    return {'lane': lane, 'publish': lane != 'docs'}


def validate_metadata(root=ROOT):
    from devlog import read_devlog
    log = read_devlog(root)
    if not isinstance(log.get('entries'), list):
        raise ValueError('dev-log entries must be an array')
    for entry in log['entries']:
        for key in ('id', 'title', 'summary'):
            if not isinstance(entry.get(key), str) or not entry[key].strip():
                raise ValueError(f'invalid checkpoint {key}')
        if not isinstance(entry.get('details', []), list) or any(not isinstance(item, str) for item in entry.get('details', [])):
            raise ValueError('checkpoint details must be an array of strings')
    print(f'Validated {len(log["entries"])} committed checkpoint records')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--event', default=os.environ.get('GITHUB_EVENT_NAME', 'workflow_dispatch'))
    parser.add_argument('--event-path', type=Path, default=os.environ.get('GITHUB_EVENT_PATH'))
    parser.add_argument('--validate', action='store_true')
    args = parser.parse_args()
    if args.validate:
        validate_metadata()
    else:
        event = json.loads(args.event_path.read_text()) if args.event_path else {}
        output(plan(event, args.event))
