#!/usr/bin/env python3
"""Print the multiplayer relay's own accounting: why it carried poses on each
recent day and whether riders are linked right now.

Reads the public, counts-only endpoints (research/MULTIPLAYER.md "Relay
history"). Cloudflare bills 20 incoming messages as one Durable Object
request; the free plan allows 100,000 requests a day.
"""
import argparse
import datetime
import json
import urllib.request

FREE_REQUESTS = 100_000
MESSAGES_PER_REQUEST = 20


def read(origin, path):
    request = urllib.request.Request(origin + path, headers={'User-Agent': 'fury-relay-usage'})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def share(n, total):
    return f'{n:>9,} {100 * n / total:5.1f}%' if total else f'{n:>9,}'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--origin', default='https://furyremastered.com')
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    now, days = read(args.origin, '/multiplayer/freeride/riders'), read(args.origin, '/multiplayer/freeride/days')
    if args.json:
        print(json.dumps({'now': now, 'days': days['days']}, indent=1))
        return
    live = now.get('today', {}).get('now', {})
    print(f"Riders now: {now.get('riders', 0)}")
    if live:
        print('  ' + ', '.join(f'{k} {v}' for k, v in sorted(live.items())))
    for day in days['days']:
        date = datetime.date(1970, 1, 1) + datetime.timedelta(days=day['day'])
        messages, poses = day['messages'], day['poses']
        relayed = sum(poses.values())
        print(f"\n{date}: {messages:,} messages from shared rooms "
              f"({100 * messages / MESSAGES_PER_REQUEST / FREE_REQUESTS:.0f}% of the free daily limit), {day['bundles']:,} courier bundles, "
              f"{day.get('batches', 0):,} pose batches")
        for kind, n in sorted(poses.items(), key=lambda item: -item[1]):
            if n:
                print(f'  {kind:<14}{share(n, relayed)}')
        if day['busiest']:
            print('  busiest: ' + ', '.join(f'{k} {v}' for k, v in sorted(day['busiest'].items())))


if __name__ == '__main__':
    main()
