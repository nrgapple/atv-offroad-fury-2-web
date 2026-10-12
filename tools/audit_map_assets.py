#!/usr/bin/env python3
"""Census every original track archive, including modes not yet playable.

No output contains asset payloads. Detailed names and authored configuration are
still source-derived research: the JSON destination must remain under private/.
Decoding coverage and original gameplay/render equivalence are separate gates.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import struct
from package_content import Resource, config, take, u32

ROOT = Path(__file__).resolve().parents[1]


def mesh_header(data):
    if take(data, 0, 4) != b'psg\0' or u32(data, 4) not in (3, 4):
        raise ValueError('unsupported mesh signature/version')
    nodes = u32(data, 8)
    if not 1 <= nodes <= 256:
        raise ValueError('invalid mesh nodes')
    at = 12 + nodes * 224
    count = u32(data, at)
    if not 1 <= count <= 4096:
        raise ValueError('invalid mesh material count')
    names = [take(data, at + 4 + i * 64, 64).split(b'\0')[0].decode('utf-8')
             for i in range(count)]
    return {'version': u32(data, 4), 'nodes': nodes, 'materials': names}


def audit(iso, entries):
    maps = defaultdict(lambda: {'archives': [], 'assets': {}, 'scenes': [],
                               'materials': [], 'animations': [], 'errors': []})
    cache = {}
    with iso.open('rb') as source:
        for entry in entries:
            path = entry['path']
            if entry['directory'] or not path.startswith('/TRACKS/') or not path.endswith('.RES'):
                continue
            family = path.split('/')[-2]
            report = maps[family]
            source.seek(entry['lba'] * 2048)
            resource = Resource(source.read(entry['bytes']))
            counts = Counter()
            for index, (name, _, size) in enumerate(resource.members):
                suffix = Path(name).suffix.lower()
                counts[suffix] += 1
                # Asset identities include the bytes, not just the often-reused
                # basename. Keep alternate L/S and mode-specific variants.
                data = resource.extract(index)
                digest = hashlib.sha256(data).hexdigest()
                key = name.lower() + ':' + digest
                asset = report['assets'].setdefault(key, {
                    'name': name, 'sha256': digest, 'bytes': size, 'archives': []})
                asset['archives'].append(path)
                if digest in cache:
                    parsed = cache[digest]
                else:
                    try:
                        parsed = config(data) if suffix in ('.scn', '.toy', '.psm') else (
                            mesh_header(data) if suffix == '.psg' else None)
                    except (ValueError, KeyError, UnicodeError, struct.error) as error:
                        parsed = {'error': str(error)}
                    cache[digest] = parsed
                if parsed and 'error' in parsed:
                    report['errors'].append({'archive': path, 'member': name, **parsed})
                if suffix in ('.scn', '.toy') and parsed and 'error' not in parsed:
                    record = {'archive': path, 'member': name, 'sha256': digest}
                    for section in ('environment', 'fog', 'lights', 'staticmodels', 'animations',
                                    'sounds', 'particleemitters', 'cameras'):
                        if section in parsed:
                            record[section] = parsed[section]
                    report['scenes'].append(record)
                    report['animations'].extend({'archive': path, 'member': name, 'section': k,
                                                 'config': v} for k, v in parsed.items()
                                                if k.startswith('animation') and k != 'animations')
                if suffix == '.psm' and parsed and 'error' not in parsed:
                    report['materials'].append({'archive': path, 'member': name, 'sha256': digest,
                                                'config': parsed})
                if suffix == '.psg' and parsed and 'error' not in parsed:
                    asset['mesh_header'] = parsed
            report['archives'].append({'path': path, 'members': len(resource.members),
                                       'extensions': dict(sorted(counts.items()))})
    output = []
    for name, report in sorted(maps.items()):
        report['assets'] = sorted(report['assets'].values(), key=lambda x: (x['name'].lower(), x['sha256']))
        unique_materials = {m['sha256']: m for m in report['materials']}
        shaders = Counter()
        animated_uv = []
        for m in unique_materials.values():
            for section, p in m['config'].items():
                if section.startswith('shaderpass_'):
                    shaders[p.get('microprogramname', '(default)')] += 1
                    if any(k.lower().startswith('uvs_animate') and v not in ('', '0') for k, v in p.items()):
                        animated_uv.append({'member': m['member'], 'pass': section,
                                            'u_velocity': p.get('u_velocity'), 'v_velocity': p.get('v_velocity')})
        report['summary'] = {'unique_assets': len(report['assets']),
                             'extensions': dict(sorted(Counter(Path(x['name']).suffix.lower()
                                                                 for x in report['assets']).items())),
                             'shaders': dict(sorted(shaders.items())), 'animated_uv': animated_uv}
        output.append({'map': name, **report})
    return {'scope': 'all disc TRACKS RES archives; header/config census, not full geometry or behavioral parity',
            'maps': output, 'archive_count': sum(len(m['archives']) for m in output),
            'unique_map_asset_variants': sum(len(m['assets']) for m in output)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'private/reports/map-fidelity/all-map-assets.json')
    args = parser.parse_args()
    if not args.output.resolve().is_relative_to((ROOT / 'private').resolve()):
        parser.error('source-derived census must stay under ignored private storage')
    source = json.loads((ROOT / 'private/reports/import.json').read_text())
    iso = ROOT / 'private/source' / Path(source['source_archive']).with_suffix('.iso').name
    entries = json.loads((ROOT / 'private/reports/disc-files.json').read_text())
    report = audit(iso, entries)
    report['source_iso_sha256'] = source['iso_sha256']
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: value for key, value in report.items() if key != 'maps'}))
    for m in report['maps']:
        print(json.dumps({'map': m['map'], 'archives': len(m['archives']), **m['summary'], 'errors': len(m['errors'])}))


if __name__ == '__main__':
    main()
