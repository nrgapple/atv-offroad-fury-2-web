#!/usr/bin/env python3
"""Build private MX venue archives; never uploads or edits the shared manifest."""
import argparse
import hashlib
import json
import re
import os
import struct
import fcntl
from package_content import mesh_material_names
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
        track_resource = Resource(read('TrackList.res'))
        track_members = {n.lower(): track_resource.extract(i) for i,(n,_,_) in enumerate(track_resource.members)}
        catalog = []
        for venue in VENUES:
            source = f'Env/{venue}/{venue}.res'
            resource = Resource(read(source))
            members = {name: resource.extract(i) for i,(name,_,_) in enumerate(resource.members)}
            members = {n:d for n,d in members.items() if not n.lower().endswith('.vuo')}
            aty_path = f'env/{venue.lower()}/{venue.lower()}.atylist'
            members[aty_path] = track_members[aty_path]
            title = re.search(rb'DisplayName\s*=\s*"([^"\r\n]+)', track_members[aty_path], re.I).group(1).decode()
            groups = {'fs':'Freestyle', 'mc':'Machine Challenge', 'nat':'National', 'sm':'Supermoto', 'sx':'Supercross'}
            catalog.append(dict(id='mx_'+venue.lower(), source_stem=venue.lower(), display_name=title, group=groups[re.sub(r'\d','',venue.lower())], archive=f'/MX/MAPS/{venue.upper()}.RES'))
            pending = list(members)
            seen = set()
            missing = set()
            excluded = set()
            def resolve(ref, origin):
                ref = ref.replace('\\','/').strip('"').lower()
                candidates = [ref, str(Path(origin).parent / ref).lower()]
                matches = [index[c]['name'] for c in candidates if c in index]
                if not matches:
                    matches = [r['name'] for r in rows if r['name'].lower().endswith('/'+ref)]
                existing = next((n for n in members if n.lower() in candidates or n.lower().endswith('/'+ref)), None)
                if existing:
                    return
                if ref.endswith('.cub') and any(n.lower().endswith('/'+ref[:-4]+'.ps2.cub') for n in members):
                    return
                if len(matches) != 1:
                    if not origin.lower().endswith('.trn'):
                        missing.add(origin+': '+ref)
                    return
                name = matches[0]
                if name.lower().endswith('.vuo'):
                    excluded.add(name)
                    return
                if name.lower().endswith(('.res','.irx','.elf','.vag')):
                    missing.add(origin+': unsupported '+name)
                    return
                members[name] = read(name)
                pending.append(name)
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                data = members[name]
                if name.lower().endswith(('.atylist','.psm','.scn','.toy','.pco')):
                    data = re.sub(rb';[^\r\n]*', b'', data)
                for ref in re.findall(rb'[A-Za-z0-9_][A-Za-z0-9_./\\-]*\.(?:psg|psm|pst|col|toy|cub|pco|trn|vuo|emt|prt|tdf|aty|cs)\b', data, re.I):
                    resolve(ref.decode(),name)
                if name.lower().endswith('.psg'):
                    at = 12 + struct.unpack_from('<I', data, 8)[0] * 224
                    count = struct.unpack_from('<I', data, at)[0]
                    if count > 4096 or at+4+count*64 > len(data):
                        raise ValueError('invalid MX material table')
                    for mat in [data[at+4+i*64:at+4+(i+1)*64].split(b'\0')[0].decode() for i in range(count)]:
                        resolve(mat+'.psm',name)
                if name.lower().endswith('.psm'):
                    for texture in re.findall(rb'TextureMap_\d+\s*=\s*([^;\r\n]+)',data,re.I):
                        stem=texture.decode().strip().strip('"')
                        resolve(stem if stem.lower().endswith('.pst') else stem+'.pst',name)
            if not any(n.lower().endswith('.ps2.trn') for n in members):
                raise ValueError(f'{venue}: missing terrain')
            packed = pack_resource(sorted(members.items()))
            digest = hashlib.sha256(packed).hexdigest()
            (output / f'{digest}.bin').write_bytes(packed)
            report.append(dict(id='mx_'+venue.lower(), source_stem=venue, archive=f'/MX/MAPS/{venue.upper()}.RES', sha256=digest, bytes=len(packed), member_count=len(members), missing=sorted(missing), excluded=sorted(excluded)))
            print(venue, len(packed), 'missing', len(missing), flush=True)
    (ROOT / 'research/mx-map-catalog.json').write_text(json.dumps(catalog,indent=2)+'\n')
    (output / 'map-packages.json').write_text(json.dumps(report,indent=2)+'\n')
    return report

def integrate_manifest(output, report):
    """Serialize additive map updates, assigning nonoverlapping virtual sectors."""
    from release_checkpoint import publication_lock
    with publication_lock(ROOT):
        path = output / 'manifest.json'
        manifest = json.loads(path.read_text())
        keys = {r['archive'] for r in report}
        records = [r for r in manifest['files'] if r['path'] not in keys]
        sector = max(((r['lba']*2048+r['bytes']+2047)//2048 for r in records), default=0)+1
        for row in report:
            blob = output / (row['sha256']+'.bin')
            if hashlib.sha256(blob.read_bytes()).hexdigest() != row['sha256']:
                raise ValueError('package hash mismatch')
            records.append(dict(path=row['archive'], lba=sector, bytes=row['bytes'], directory=False, lazy=True, repacked=True, source_build_sha256=MX_BUILD, member_count=row['member_count'], sha256=row['sha256'], url='/content/'+row['sha256']+'.bin'))
            sector += (row['bytes']+2047)//2048+1
        manifest['files'] = records
        temporary = path.with_suffix('.json.tmp')
        with temporary.open('w') as stream:
            stream.write(json.dumps(manifest,indent=2)+'\n')
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(path)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--integrate-manifest', action='store_true')
    args = parser.parse_args()
    result = package(args.output)
    if args.integrate_manifest:
        integrate_manifest(args.output.resolve(), result)
    print(json.dumps({'venues':len(result),'bytes':sum(r['bytes'] for r in result)}))
