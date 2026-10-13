#!/usr/bin/env python3
"""Append only connected MX motorcycle runtime resources to the private R2 package."""
import argparse
import hashlib
import json
from pathlib import Path
import re
from package_content import ROOT, Resource, config, pack_resource

MX_BUILD = '4b74c0c76ffe0838e00c1b9f75b31e1bb91adae06ec9da1416355b363572102a'
DEFINITIONS = {
 50: ['MXF_S_50_Fast50_01','MXF_S_50_Sano_01','MXF_S_50_RedB_01'],
 125: ['MX_S_125_RM1251','MX_S_125_CR125RTHQ1','MX_S_125_YZ1251','MX_S_125_KX1251','MX_S_125_125SX1'],
 250: ['MX_S_250_RM2501','MX_S_250_CR250RTHQ1','MX_S_250_YZ2501','MX_S_250_KX2501','MX_S_250_250SX1'],
 500: ['MX_S_500_RM500XGX1','MX_S_500_SX500XGX1','MX_S_500_YZF500XGX1'],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'private/reports/hosted-content')
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to((ROOT / 'private').resolve()):
        parser.error('runtime content must stay under ignored private/')
    rows = json.loads((ROOT / 'private/reports/mx-unleashed/psr-directory.json').read_text())
    by_name = {r['name'].lower(): r for r in rows}
    manifest_path = output / 'manifest.json'
    manifest = json.loads(manifest_path.read_text())
    with (ROOT / 'private/source/MX vs. ATV Unleashed (USA).iso').open('rb') as disc:
        disc.seek(277 * 2048)
        if hashlib.sha256(disc.read(2965120)).hexdigest() != MX_BUILD:
            raise ValueError('unexpected motorcycle source build')

        def source(name):
            row = by_name[name.replace('\\', '/').lower()]
            if not 0 < row['bytes'] <= 25 * 1024 * 1024:
                raise ValueError('motorcycle member exceeds bound')
            disc.seek((1400000 + row['lba']) * 2048)
            data = disc.read(row['bytes'])
            if len(data) != row['bytes']:
                raise ValueError('truncated motorcycle source')
            return data

        def materials(members, remaps):
            names = set()
            for name, data in list(members.items()):
                if not name.lower().endswith('.psg'):
                    continue
                # Fixed-width material table follows the audited 224-byte nodes.
                import struct
                at = 12 + struct.unpack_from('<I', data, 8)[0] * 224
                count = struct.unpack_from('<I', data, at)[0]
                for i in range(count):
                    mat = data[at+4+i*64:at+4+(i+1)*64].split(b'\0')[0].decode()
                    names.add(remaps.get(mat.lower(), mat))
            for mat in names:
                psm = next((r['name'] for r in rows if r['name'].lower().endswith('/' + mat.lower() + '.psm')), None)
                binding = mat
                if psm:
                    members[psm] = source(psm)
                    text = members[psm].decode()
                    match = re.search(r'^\s*TextureMap_1\s*=\s*([^\r\n;]+)', text, re.I | re.M)
                    if match:
                        binding = match[1].strip().strip('"')
                    for mud in re.findall(r'^\s*TextureMap_1\s*=\s*([^\r\n;]+)', text, re.I | re.M)[1:]:
                        stem = mud.strip().strip('"')
                        extra = next((r['name'] for r in rows if r['name'].lower().endswith('/' + stem.lower() + '.pst')), None)
                        if extra: members[extra] = source(extra)
                pst = next((r['name'] for r in rows if r['name'].lower().endswith('/' + binding.lower() + '.pst')), None)
                if pst:
                    members[pst] = source(pst)

        def assemble(definition, base):
            sub = f'Veh/Anim/{definition}.sub'
            members = {sub: source(sub), f'Veh/Anim/{base}.psg': source(f'Veh/Anim/{base}.psg')}
            data = config(members[sub])
            for i in range(1, int(data['objects']['numberofsubobjects']) + 1):
                stem = data[f'subobject{i}']['slt']
                name = f'Veh/Anim/{stem}.psg'
                members[name] = source(name)
            materials(members, data.get('material changes', {}))
            return members

        rider_sub = 'Rider/Anim/THQ1.sub'
        rider = {rider_sub: source(rider_sub), 'Rider/Anim/mrider.psg': source('Rider/Anim/mrider.psg')}
        rider_config = config(rider[rider_sub])
        for i in range(1, int(rider_config['objects'].get('numberofsubobjects', '0')) + 1):
            name = f"Rider/Anim/{rider_config[f'subobject{i}']['slt']}.psg"
            rider[name] = source(name)
        materials(rider, rider_config.get('material changes', {}))
        rider_base = dict(rider)
        def motion_bank(size):
            bank = 'MXF' if size == 50 else 'MX'
            result = dict(rider_base)
            stunt_ini = f'Rider/Anim/{bank}_Rider_Stunts.ini'
            stunt_data = config(source(stunt_ini))
            connected = {'mx_' + fields['name'].lower() for section, fields in stunt_data.items()
                         if section.startswith('stunt') and fields.get('defaultkeymapping', '')}
            connected.update(('mx_backflip','mx_threesixtyl','mx_threesixtyr'))
            for suffix in ('RiderMotions.mcf','RiderStunts.mcf'):
                name = f'Rider/Anim/{bank}_{suffix}'
                result[name] = source(name)
                for section, fields in config(result[name]).items():
                    if not section.startswith('motion ') or 'vue' not in fields:
                        continue
                    alias = fields.get('name','').lower().replace('mxf_', 'mx_').replace('mx_rider_', 'mx_')
                    if suffix == 'RiderStunts.mcf' and alias not in connected:
                        continue
                    name = 'Rider/Anim/' + fields['vue']
                    result[name] = source(name)
            result[stunt_ini] = source(stunt_ini)
            return result
        records = [r for r in manifest['files'] if not r['path'].startswith('/MX/')]
        sector = max((r['lba'] * 2048 + r['bytes'] + 2047) // 2048 for r in records) + 1
        report = []
        for size, definitions in DEFINITIONS.items():
            members = {}
            for definition in definitions:
                members.update(assemble(definition, 'bike50' if size == 50 else 'bike'))
            members.update(motion_bank(size))
            for stem in ('tiredust.emt','tirechunks.emt','tiredirtspray.emt','exhaust.emt','wreckdust.emt','FS01particles.prt'):
                name=f'Part/{stem}'
                members[name]=source(name)
            texture=next(r['name'] for r in rows if r['name'].lower().endswith('/fs01particles.pst'))
            members[texture]=source(texture)
            for mode in ('Race', 'Pro'):
                name = f'Veh/mx_{mode}{size}P.phy'
                members[name] = source(name)
            name = f'Aud/vehicle_{size}A.aud'
            members[name] = source(name)
            audio = config(members[name])
            for section, fields in audio.items():
                for i in range(1, int(fields.get('numsounds', '0')) + 1):
                    sound = fields[f'sound{i}'].replace('\\', '/')
                    sound = 'Aud/' + sound + ('' if sound.lower().endswith('.vag') else '.vag')
                    members[sound] = source(sound)
            packed = pack_resource(sorted(members.items()))
            digest = hashlib.sha256(packed).hexdigest()
            (output / f'{digest}.bin').write_bytes(packed)
            records.append({'path': f'/MX/BIKE{size}.RES', 'lba': sector, 'bytes': len(packed),
                            'directory': False, 'lazy': True, 'repacked': True,
                            'source_build_sha256': MX_BUILD, 'member_count': len(members),
                            'sha256': digest, 'url': f'/content/{digest}.bin'})
            sector += (len(packed) + 2047) // 2048 + 1
            report.append({'class': size, 'members': len(members), 'bytes': len(packed)})
        manifest['files'] = records
        manifest['motorcycle_source_build_sha256'] = MX_BUILD
        manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
        print(json.dumps(report))


if __name__ == '__main__':
    main()
