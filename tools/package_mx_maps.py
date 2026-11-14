#!/usr/bin/env python3
"""Build private MX venue archives; never uploads or edits the shared manifest."""
import argparse
import hashlib
import json
from pathlib import Path
from package_content import ROOT, Resource, pack_resource
from package_motorcycles import MX_BUILD

VENUES = tuple(f'{prefix}{i:02}' for prefix, count in [('FS',8),('MC',10),('Nat',17),('SM',3),('SX',18)] for i in range(1,count+1))


def package(output):
    output = output.resolve()
    if not output.is_relative_to((ROOT / 'private').resolve()):
        raise ValueError('output must remain in ignored private storage')
    if output == (ROOT / 'private/reports/hosted-content').resolve():
        raise ValueError('use a staging output until runtime validation')
    rows = json.loads((ROOT / 'private/reports/mx-unleashed/psr-directory.json').read_text())
    index = {r['name'].lower(): r for r in rows}
    output.mkdir(parents=True, exist_ok=True)
    report = []
    with (ROOT / 'private/source/MX vs. ATV Unleashed (USA).iso').open('rb') as disc:
        disc.seek(277*2048)
        if hashlib.sha256(disc.read(2965120)).hexdigest() != MX_BUILD:
            raise ValueError('unexpected source build')
        def read(name):
            row = index[name.lower()]
            if not 0 < row['bytes'] <= 128*1024*1024:
                raise ValueError('invalid source size')
            disc.seek((1400000+row['lba'])*2048)
            data = disc.read(row['bytes'])
            if len(data) != row['bytes']:
                raise ValueError('truncated source')
            return data
        for venue in VENUES:
            source = f'Env/{venue}/{venue}.res'
            resource = Resource(read(source))
            members = {name: resource.extract(i) for i,(name,_,_) in enumerate(resource.members)}
            if not any(n.lower().endswith('.ps2.trn') for n in members):
                raise ValueError(f'{venue}: missing terrain')
            packed = pack_resource(sorted(members.items()))
            digest = hashlib.sha256(packed).hexdigest()
            (output / f'{digest}.bin').write_bytes(packed)
            report.append(dict(id='mx_'+venue.lower(), source_stem=venue, archive=f'/MX/MAPS/{venue.upper()}.RES', sha256=digest, bytes=len(packed), member_count=len(members)))
    (output / 'map-packages.json').write_text(json.dumps(report,indent=2)+'\n')
    return report

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    args = parser.parse_args()
    result = package(args.output)
    print(json.dumps({'venues':len(result),'bytes':sum(r['bytes'] for r in result)}))
