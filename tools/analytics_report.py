#!/usr/bin/env python3
"""Summarize anonymous gameplay usage from the fury_usage Analytics Engine dataset.

Needs a Cloudflare API token with Account > Account Analytics > Read in
CLOUDFLARE_ANALYTICS_TOKEN. The token is read from the environment only and
is never printed. See research/ANALYTICS.md for the event schema.
"""
import argparse
import json
import os
from pathlib import Path
import sys
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
DATASET = 'fury_usage'
# Column layout written by web/analytics.js dataPoint().
COLUMNS = {'event': 'blob1', 'country': 'blob2', 'release': 'blob3', 'device': 'blob4', 'display': 'blob5', 'entry': 'blob6',
           'mode': 'blob7', 'activity': 'blob8', 'map': 'blob9', 'vehicle': 'blob10', 'quad': 'double1', 'seconds': 'double2',
           'place': 'double3', 'field': 'double4', 'laps': 'double5', 'cpu': 'double6', 'players': 'double7'}
C = COLUMNS
COUNT = 'SUM(_sample_interval)'


def queries(days):
    since = f"timestamp > NOW() - INTERVAL '{days}' DAY"
    def where(event):
        return f"WHERE {since} AND {C['event']} = '{event}'"
    minutes = f"SUM(_sample_interval * {C['seconds']}) / 60"
    return {
        'Daily activity': f"SELECT toStartOfDay(timestamp) AS day, {C['event']} AS event, {COUNT} AS events, {minutes} AS minutes "
                          f"FROM {DATASET} WHERE {since} GROUP BY day, event ORDER BY day, event",
        'Sessions by device': f"SELECT {C['device']} AS device, {C['display']} AS display, {C['entry']} AS entry, {COUNT} AS sessions "
                              f"FROM {DATASET} {where('session_start')} GROUP BY device, display, entry ORDER BY sessions DESC",
        'Rides by mode': f"SELECT {C['mode']} AS mode, {C['activity']} AS activity, {COUNT} AS rides, "
                         f"SUM(_sample_interval * {C['seconds']}) / {COUNT} AS avg_load_seconds "
                         f"FROM {DATASET} {where('ride_start')} GROUP BY mode, activity ORDER BY rides DESC",
        'Riding minutes by venue': f"SELECT {C['map']} AS map, {C['activity']} AS activity, {minutes} AS minutes "
                                   f"FROM {DATASET} {where('ride_time')} GROUP BY map, activity ORDER BY minutes DESC LIMIT 25",
        'Top vehicles': f"SELECT {C['vehicle']} AS vehicle, {C['quad']} AS quad, {COUNT} AS rides "
                        f"FROM {DATASET} {where('ride_start')} GROUP BY vehicle, quad ORDER BY rides DESC LIMIT 15",
        'Race finishes': f"SELECT {C['map']} AS map, {COUNT} AS finishes, SUM(_sample_interval * {C['place']}) / {COUNT} AS avg_place, "
                         f"SUM(_sample_interval * {C['seconds']}) / {COUNT} AS avg_seconds "
                         f"FROM {DATASET} {where('race_finish')} GROUP BY map ORDER BY finishes DESC LIMIT 25",
        'Load failures': f"SELECT {C['mode']} AS mode, {C['map']} AS map, {C['device']} AS device, {COUNT} AS failures "
                         f"FROM {DATASET} {where('ride_failed')} GROUP BY mode, map, device ORDER BY failures DESC LIMIT 15",
        'Online lobby size': f"SELECT {C['players']} AS players, {COUNT} AS rides "
                             f"FROM {DATASET} WHERE {since} AND {C['event']} = 'ride_start' AND {C['mode']} = 'multiplayer' "
                             f"GROUP BY players ORDER BY players",
        'Countries': f"SELECT {C['country']} AS country, {COUNT} AS sessions "
                     f"FROM {DATASET} {where('session_start')} GROUP BY country ORDER BY sessions DESC LIMIT 15",
        'Releases': f"SELECT {C['release']} AS release, {COUNT} AS sessions, MIN(timestamp) AS first_seen "
                    f"FROM {DATASET} {where('session_start')} GROUP BY release ORDER BY first_seen DESC LIMIT 10",
    }


def account_id():
    config = json.loads((ROOT / 'wrangler.jsonc').read_text())
    return config['account_id']


def run(sql, account, token):
    request = urllib.request.Request(f'https://api.cloudflare.com/client/v4/accounts/{account}/analytics_engine/sql',
                                     data=f'{sql} FORMAT JSON'.encode(), method='POST',
                                     headers={'Authorization': f'Bearer {token}', 'Content-Type': 'text/plain'})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response).get('data', [])
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors='replace')[:300]
        raise SystemExit(f'Analytics Engine query failed (HTTP {error.code}): {detail}') from None


def table(rows):
    if not rows:
        return '  (no data yet)'
    keys = list(rows[0])
    def cell(value):
        if isinstance(value, float):
            return f'{value:.1f}'
        return str(value) if value not in ('', None) else '-'
    widths = [max(len(key), *(len(cell(row[key])) for row in rows)) for key in keys]
    lines = ['  ' + '  '.join(key.ljust(width) for key, width in zip(keys, widths))]
    lines += ['  ' + '  '.join(cell(row[key]).ljust(width) for key, width in zip(keys, widths)) for row in rows]
    return '\n'.join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--days', type=int, default=7, choices=range(1, 91), metavar='1-90', help='lookback window (default 7)')
    parser.add_argument('--json', action='store_true', help='print raw rows as JSON')
    parser.add_argument('--sql', action='store_true', help='print the queries without running them')
    args = parser.parse_args(argv)
    selected = queries(args.days)
    if args.sql:
        for title, sql in selected.items():
            print(f'-- {title}\n{sql};\n')
        return 0
    token = os.environ.get('CLOUDFLARE_ANALYTICS_TOKEN')
    if not token:
        print('Set CLOUDFLARE_ANALYTICS_TOKEN to a token with Account Analytics Read access.', file=sys.stderr)
        return 2
    account = account_id()
    results = {title: run(sql, account, token) for title, sql in selected.items()}
    if args.json:
        print(json.dumps(results, indent=2))
        return 0
    print(f'Fury Remastered usage — last {args.days} day(s)\n')
    for title, rows in results.items():
        print(f'{title}\n{table(rows)}\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
