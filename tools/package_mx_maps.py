#!/usr/bin/env python3
"""Build private MX venue archives; never uploads or edits the shared manifest."""
import argparse
import hashlib
import json
import re
import os
import struct
from pathlib import Path
from package_content import ROOT, Resource, pack_resource
from package_motorcycles import MX_BUILD

VENUES = tuple(f'{prefix}{i:02}' for prefix, count in [('FS',8),('MC',10),('Nat',17),('SM',3),('SX',18)] for i in range(1,count+1))


def related_source_candidates(ref, rows):
    """Suggest source naming variants without asserting runtime equivalence."""
    stem = Path(ref).stem.lower().replace('_', '').replace(' ', '').replace('.dxm', '')
    return sorted(r['name'] for r in rows
                  if stem in Path(r['name']).stem.lower().replace('_', '').replace(' ', '')
                  and Path(r['name']).suffix.lower() == Path(ref).suffix.lower())


def source_sections(data):
    from package_content import config
    return config(data.decode('latin1').encode('utf-8'))


def apply_binding_semantics(members, audit, resolve):
    """Apply source count gates and independently named rigid-body colliders."""
    scenes = [(n, source_sections(d)) for n,d in members.items() if n.lower().endswith('.scn')]
    models = set()
    for _, sections in scenes:
        for fields in sections.values():
            if fields.get('slt'):
                models.add(Path(fields['slt'].replace('\\','/')).stem.lower())
    collections = []
    active_models = set()
    for name, data in list(members.items()):
        if not name.lower().endswith('.toy'):
            continue
        sections = source_sections(data)
        count = int(sections.get('physicscollections', {}).get('numberofphysicscollections', '0'))
        for section, fields in sections.items():
            match = re.fullmatch(r'physicscollection(\d+)', section)
            if not match:
                continue
            pattern = fields.get('name','').lower()
            active = 1 <= int(match[1]) <= count
            matched = sorted(model for model in models if (model.startswith(pattern[:-1]) if pattern.endswith('*') else model == pattern)) if active else []
            record = dict(origin=name, section=section, pattern=pattern, active_count=count, status='active' if active else 'inactive_count_gate', models=matched, bindings=[])
            for model in matched:
                active_models.add(model)
                binding = dict(model=model, roles={})
                for suffix, role in [('m','actor_whiskers'), ('f','collision_target')]:
                    ref = model+suffix+'.col'
                    resolve(ref,name,role)
                    paths = [n for n in members if n.lower().endswith('/'+ref)]
                    binding['roles'][role] = dict(reference=ref, path=paths[0] if len(paths)==1 else None, status='resolved' if len(paths)==1 else 'source_missing')
                record['bindings'].append(binding)
            collections.append(record)
    # HasCol=False is an object-level semantic exclusion, regardless of filename.
    nonphysical = set()
    for name, sections in scenes:
        for section, fields in sections.items():
            if fields.get('hascol','').lower() in ('false','f','0'):
                for value in fields.values():
                    if value.lower().endswith('.col'):
                        nonphysical.add((name,value.lower()))
    repairs = {
        'nat05_liverystable_detail.dxm.pst': 'Tex/Nat05_liverystable_detail.pst',
        'nat12 _genericsandstone.pst': 'Tex/Nat12_genericsandstone.pst',
        'nat12 _pillara.pst': 'Tex/Nat12_pillarA.pst',
    }
    for edge in list(audit):
        ref = edge['reference']
        if edge['status'] not in ('actual_missing','ambiguous'):
            continue
        stem = Path(ref).stem.lower()
        if (edge['origin'],ref) in nonphysical:
            edge.update(status='nonphysical_marker', reason='source SCN HasCol=False; no physical collider required')
        elif ref.endswith('.col') and stem in active_models:
            edge.update(status='physics_collection_roles', reason='002171b0 count-gated collection; 004922a0 derives independent m.col/f.col', model=stem)
        elif ref.endswith('.col') and '/nat10/' in edge['origin'].lower() and stem in ('nat10_mailbox_closed','nat10_mailboxopen'):
            edge.update(status='inactive_count_gate', reason='TOY NumberOfPhysicsCollections=0; mailbox sections must not activate')
        elif ref in repairs and edge['kind'] == 'texture':
            target = repairs[ref]
            resolve(target,edge['origin'],'authored_texture_repair_target')
            paths = [n for n in members if n.lower() == target.lower()]
            if len(paths) != 1:
                raise ValueError('authored repair target is not unique: '+target)
            edge.update(status='authored_repair', repair_target=paths[0], source_equivalent=False, reason='explicit spelling repair; original PSM and PST paths/bytes preserved')
        elif ref.endswith('.col'):
            edge.update(status='source_missing', reason='unsuffixed source collider absent; segment or similarly named colliders are not aliases; 00319b40 tolerates object load failure')
    return collections


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
        # Index nested venue members too; do not assume the PSR lists every payload.
        nested = {}
        for stem in VENUES:
            archive = f'Env/{stem}/{stem}.res'
            container = Resource(read(archive))
            for position, (name, _, _) in enumerate(container.members):
                nested.setdefault(name.lower(), []).append((archive, position, name))
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
            audit = []
            missing = set()
            excluded = set()
            requested = set()
            def resolve(ref, origin, kind='file'):
                ref = ref.replace('\\','/').strip('"').lower()
                identity = (origin, ref, kind)
                if identity in requested:
                    return
                requested.add(identity)
                entry = dict(origin=origin, reference=ref, kind=kind)
                def record(status, **extra):
                    audit.append(dict(entry, status=status, **extra))
                if ref == '?.slt.psg' and re.search(rb'NumberOfStaticModelParams\s*=\s*0\b', members[origin], re.I):
                    record('false_positive_inactive_template', reason='TOY NumberOfStaticModelParams=0; unused Model1 example')
                    return
                variants = [ref]
                if ref.endswith(('.cub','.trn')) and '.ps2.' not in ref:
                    variants.append(ref.rsplit('.',1)[0]+'.ps2.'+ref.rsplit('.',1)[1])
                if kind == 'texture':
                    variants.append(ref.rsplit('.',1)[0]+'.atm')
                for variant in variants:
                    exact = [variant, str(Path(origin).parent / variant).lower()]
                    matches = [n for n in members if n.lower() in exact]
                    if not matches:
                        matches = [n for n in members if n.lower().endswith('/'+variant)]
                    if len(matches) == 1:
                        record('resolved' if variant == ref else 'platform_alias', path=matches[0])
                        return
                    candidates = [r['name'] for r in rows if r['name'].lower() in exact]
                    if not candidates:
                        candidates = [r['name'] for r in rows if r['name'].lower().endswith('/'+variant)]
                    if len(candidates) == 1:
                        name = candidates[0]
                        if name.lower().endswith('.vuo'):
                            excluded.add(name)
                            record('excluded_shader', path=name)
                            return
                        members[name] = read(name)
                        pending.append(name)
                        record('resolved' if variant == ref else 'platform_alias', path=name)
                        return
                    nested_matches = [key for key in nested if key in exact]
                    if not nested_matches:
                        nested_matches = [key for key in nested if key.endswith('/'+variant)]
                    if not candidates and len(nested_matches) == 1:
                        archive, position, name = sorted(nested[nested_matches[0]])[0]
                        if not name.lower().endswith('.vuo'):
                            members[name] = Resource(read(archive)).extract(position)
                            pending.append(name)
                            record('resolved' if variant == ref else 'platform_alias', path=name, source_archive=archive)
                            return
                    if len(candidates) > 1 or len(matches) > 1 or len(nested_matches) > 1:
                        record('ambiguous', candidates=sorted(set(candidates+matches)))
                        return
                if kind == 'texture' and ref == 'rendertarget.pst':
                    record('dynamic_generated_target', reason='Screen.psm RenderTarget binding')
                else:
                    related = related_source_candidates(ref, rows)
                    missing.add(origin+': '+ref)
                    record('ambiguous' if related else 'actual_missing', candidates=related, reason='no exact, relative, basename or supported platform/animation alias; similarly named files are evidence, not an automatic substitution')
            while pending:
                name = pending.pop()
                if name in seen:
                    continue
                seen.add(name)
                data = members[name]
                # Only textual configuration fields and audited PSG tables create edges.
                if name.lower().endswith(('.scn','.toy','.pco','.psm','.atylist','.aty')):
                    text = re.sub(rb';[^\r\n]*', b'', data)
                    for key, value in re.findall(rb'^\s*([A-Za-z0-9_]+)\s*=\s*([^\r\n]+)', text, re.M):
                        key = key.decode().lower()
                        value = value.decode('latin1').strip().strip('"')
                        if key == 'slt':
                            resolve(value+'.psg',name,'model')
                        elif key.startswith('texturemap_'):
                            resolve(value if value.lower().endswith('.pst') else value+'.pst',name,'texture')
                        elif key.startswith('dependentmaterial_'):
                            resolve(value+'.psm',name,'material')
                        elif re.fullmatch(r'[^\s]+\.(?:col|cub|trn|toy|pco|psg|psm|vuo)',value,re.I):
                            resolve(value,name,'shader' if value.lower().endswith('.vuo') else 'configuration')
                if name.lower().endswith('.psg'):
                    at = 12 + struct.unpack_from('<I', data, 8)[0] * 224
                    count = struct.unpack_from('<I', data, at)[0]
                    if count > 4096 or at+4+count*64 > len(data):
                        raise ValueError('invalid MX material table')
                    for mat in [data[at+4+i*64:at+4+(i+1)*64].split(b'\0')[0].decode() for i in range(count)]:
                        resolve(mat+'.psm',name,'material')
            physics_collections = apply_binding_semantics(members, audit, resolve)
            missing = {e['origin']+': '+e['reference'] for e in audit if e['status'] in ('actual_missing','ambiguous','source_missing')}
            if not any(n.lower().endswith('.ps2.trn') for n in members):
                raise ValueError(f'{venue}: missing terrain')
            packed = pack_resource(sorted(members.items()))
            digest = hashlib.sha256(packed).hexdigest()
            (output / f'{digest}.bin').write_bytes(packed)
            report.append(dict(id='mx_'+venue.lower(), source_stem=venue.lower(), archive=f'/MX/MAPS/{venue.upper()}.RES', sha256=digest, bytes=len(packed), member_count=len(members), missing=sorted(missing), excluded=sorted(excluded), physics_collections=physics_collections, dependency_audit=sorted({json.dumps(e,sort_keys=True):e for e in audit}.values(), key=lambda e:(e['origin'],e['reference'],e['kind']))))
            print(venue, len(packed), 'missing', len(missing), flush=True)
    (ROOT / 'research/mx-map-catalog.json').write_text(json.dumps(catalog,indent=2)+'\n')
    (output / 'map-packages.json').write_text(json.dumps(report,indent=2)+'\n')
    return report

def classify_previous_warnings(previous, current):
    """Account for warnings from the retired opaque-byte lexical scan."""
    by_id = {row['id']: row for row in current}
    result = []
    for row in previous:
        edges = by_id[row['id']]['dependency_audit']
        for warning in row.get('missing', []):
            origin, ref = warning.split(': ', 1)
            matches = [edge for edge in edges if edge['origin'] == origin and edge['reference'] == ref]
            if matches:
                classification = matches[0]['status']
                evidence = matches
            else:
                classification = 'false_positive_binary_token' if origin.lower().endswith(('.trn','.psg','.pst','.atm','.col')) else 'false_positive_untyped_or_commented_field'
                evidence = []
            result.append(dict(id=row['id'], origin=origin, reference=ref, classification=classification, typed_evidence=evidence))
    return result


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
