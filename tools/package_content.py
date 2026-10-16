#!/usr/bin/env python3
"""Package explicitly authorized runtime content for the connected browser maps.

Generated files belong only in ignored private storage and the authorized content
hosting destination. Never add these outputs, the source disc or executable to Git.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1]
BLOCK = 0x6000
CHUNK = 1024 * 1024
MAX_FILE = 25 * 1024 * 1024
SOURCE_SHA = '11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49'
STARTUP = (
    '/TRACKS/TRACKSA/ENV01/E01_NAT.RES',
    '/TRACKS/TRACKSA/ENV01/E01_FR.RES', '/TRACKS/TRACKSA/ENV01/E01_ENDL.RES',
    '/QUAD/BIKESA/ATV01.RES', '/QUAD/BIKESA/ATV01_MT.RES',
    '/RIDER/MRIDER.RES', '/RIDER/FRIDER.RES', '/RIDER/RIDER_MT.RES', '/ANIMSRDR.RES',
    '/AUDIOTBL.RES', '/STARTAPP.RES', '/STARTEVT.RES', '/OVERLAYS.RES',
    '/TRACKS/TRACKSA/ENV02/E02_FR.RES', '/TRACKS/TRACKSA/ENV02/E02_ENDL.RES',
    '/TRACKS/TRACKSA/ENV03/E03_FR.RES', '/TRACKS/TRACKSA/ENV03/E03_ENDL.RES',
    '/TRACKS/TRACKSA/ENV04/E04_FR.RES', '/TRACKS/TRACKSA/ENV04/E04_ENDL.RES',
    '/TRACKS/TRACKSA/ENV05/E05_FR.RES', '/TRACKS/TRACKSA/ENV05/E05_ENDL.RES',
    '/TRACKS/TRACKSA/ENV06/E06_FR.RES', '/TRACKS/TRACKSA/ENV06/E06_ENDL.RES',
    '/TRACKS/TRACKSA/ENV07/E07_FR.RES', '/TRACKS/TRACKSA/ENV07/E07_ENDL.RES',
    '/TRACKS/TRACKSA/ENV08/E08_FR.RES', '/TRACKS/TRACKSA/ENV08/E08_ENDL.RES',
    '/TRACKS/TRACKSA/FREE05/F05_FR.RES',
    '/UI/RESOURCE/QUADINFO.RES', '/UI/RESOURCE/PAGEA/PAGE07.RES',
    '/UI/RESOURCE/TP/TP_ANAT.RES',
    '/UI/RESOURCE/TP/TP_PSTNT.RES',
    '/UI/RESOURCE/TITLE.RES', '/UI/RESOURCE/PAGEA/PAGE01.RES',
    '/UI/RESOURCE/PAGEA/PAGE02.RES', '/UI/RESOURCE/PAGEA/PAGE06.RES',
)
# Original Freeride versions of all remaining venues, including locked courses.
STARTUP += tuple(f'/TRACKS/TRACKSA/FREE{i:02}/F{i:02}_FR.RES' for i in (1,2,3,4,6))
STARTUP += tuple(f'/TRACKS/TRACKSA/NAT{i:02}/N{i:02}_FR.RES' for i in range(1,9))
STARTUP += tuple(f'/TRACKS/TRACKSB/SX{i:02}/S{i:02}_FR.RES' for i in range(1,17))
STARTUP += ('/TRACKS/TRACKSB/TRAINING/T00_TR.RES', '/TRACKS/TRACKSA/GAME01/G01_SO.RES')
# Original race archives (TDF course, AIRiders, race TOY) for every other
# Environment, National and Supercross venue; ENV01 is listed above.
STARTUP += tuple(f'/TRACKS/TRACKSA/ENV{i:02}/E{i:02}_NAT.RES' for i in range(2,9))
STARTUP += tuple(f'/TRACKS/TRACKSA/NAT{i:02}/N{i:02}_NAT.RES' for i in range(1,9))
STARTUP += tuple(f'/TRACKS/TRACKSB/SX{i:02}/S{i:02}_NAT.RES' for i in range(1,17))
# Original Profile Store gear catalog (uiRider.txt only) for rider customization.
STARTUP += ('/UI/RESOURCE/RIDEINFO.RES',)
STARTUP += tuple(f'/UI/RESOURCE/TP/TP_{group}.RES' for group in ('PNAT','ASX','PSX','HOCK'))

# Original loading-screen action shots for every Freestyle venue except Training,
# which ships only progress-bar art. Each course's file is fetched lazily.
LOADICON = tuple([f'/UI/LOADICON/ENV{"AAABBBCC"[i - 1]}/E{i:02}_OB.LSI' for i in range(1, 9)]
    + [f'/UI/LOADICON/FREE/F{i:02}_OB.LSI' for i in range(1, 7)]
    + [f'/UI/LOADICON/NAT/N{i:02}_NAT.LSI' for i in range(1, 9)]
    + [f'/UI/LOADICON/SX/S{i:02}_NAT.LSI' for i in range(1, 17)]
    + ['/UI/LOADICON/GAME/G01_SO.LSI'])
STARTUP += LOADICON

STARTUP += tuple(path for i in range(2, 27) for path in (
    f'/QUAD/BIKES{"A" if i <= 13 else "B"}/ATV{i:02}.RES',
    f'/QUAD/BIKES{"A" if i <= 13 else "B"}/ATV{i:02}_MT.RES'))


def take(data, at, count):
    if at < 0 or count < 0 or at + count > len(data):
        raise ValueError('truncated resource')
    return data[at:at + count]


def u32(data, at):
    return struct.unpack('<I', take(data, at, 4))[0]


class Resource:
    """Bounded reader for the already-audited version-3 block container."""
    def __init__(self, data):
        if len(data) > 128 * 1024 * 1024 or u32(data, 0) != 3 or u32(data, 12) != 0:
            raise ValueError('unsupported resource header')
        flags = u32(data, 8)
        if flags not in (0x340, 0x100):
            raise ValueError('unsupported resource flags')
        directory = u32(data, 4) + 8
        if directory < 20:
            raise ValueError('invalid directory offset')
        count, size = u32(data, directory), u32(data, directory + 4)
        table = take(data, directory + 8, size)
        if count > size // 13:
            raise ValueError('invalid member count')
        self.members = []
        at = 0
        for _ in range(count):
            length = u32(table, at)
            name = take(table, at + 4, length).decode('utf-8').replace('\\', '/')
            at += 4 + length
            offset, size = u32(table, at), u32(table, at + 4)
            at += 8
            if not name or '\0' in name or name.startswith('/') or ':' in name or any(p in ('', '.', '..') for p in name.split('/')):
                raise ValueError('unsafe resource name')
            self.members.append((name, offset, size))
        if at != len(table):
            raise ValueError('unexpected directory suffix')
        self.data = data
        self.payload = directory + 8 + len(table)
        self.blocks = None
        if flags == 0x340:
            table_size = u32(data, 16)
            if table_size < 2 or table_size % 2 or table_size != directory - 20 or take(data, directory - 2, 2) != b'\0\0':
                raise ValueError('invalid block table')
            self.blocks = []
            at = self.payload
            for pos in range(20, directory - 2, 2):
                length = struct.unpack('<H', data[pos:pos + 2])[0] + 1
                self.blocks.append(take(data, at, length))
                at += length
            if at != len(data):
                raise ValueError('invalid resource payload length')
            logical = len(self.blocks) * BLOCK
        else:
            logical = len(data) - self.payload
        if any(offset + size > logical or size > 128 * 1024 * 1024 for _, offset, size in self.members):
            raise ValueError('invalid resource member range')

    def extract(self, index):
        _, offset, size = self.members[index]
        if self.blocks is None:
            return take(self.data, self.payload + offset, size)
        end = offset + size
        result = bytearray()
        if not size:
            return bytes(result)
        for i in range(offset // BLOCK, (end - 1) // BLOCK + 1):
            decoder = zlib.decompressobj()
            decoded = decoder.decompress(self.blocks[i], BLOCK + 1)
            if len(decoded) != BLOCK or not decoder.eof or decoder.unused_data not in (b'', b'\xff') or decoder.unconsumed_tail:
                raise ValueError('invalid compressed block')
            result.extend(decoded[max(0, offset - i * BLOCK):min(BLOCK, end - i * BLOCK)])
        if len(result) != size:
            raise ValueError('extraction size mismatch')
        return bytes(result)

    def named(self, suffix):
        matches = [i for i, (name, _, _) in enumerate(self.members) if name.lower().split('/')[-1] == suffix.lower()]
        if len(matches) != 1:
            raise ValueError(f'expected one resource member: {suffix}')
        return self.extract(matches[0])


def pack_resource(members):
    directory, plain = bytearray(), bytearray()
    for name, data in members:
        encoded = name.encode('utf-8')
        directory.extend(struct.pack('<I', len(encoded)) + encoded + struct.pack('<II', len(plain), len(data)))
        plain.extend(data)
    if not plain:
        raise ValueError('empty compact archive')
    plain.extend(bytes((-len(plain)) % BLOCK))
    blocks = [zlib.compress(plain[i:i + BLOCK], 9) for i in range(0, len(plain), BLOCK)]
    if any(not 1 <= len(block) <= 65536 for block in blocks):
        raise ValueError('compressed resource block too large')
    table = b''.join(struct.pack('<H', len(block) - 1) for block in blocks) + b'\0\0'
    packed = struct.pack('<IIIII', 3, 12 + len(table), 0x340, 0, len(table)) + table
    packed += struct.pack('<II', len(members), len(directory)) + directory + b''.join(blocks)
    reader = Resource(packed)
    for i, (name, original) in enumerate(members):
        if reader.members[i][0] != name or reader.extract(i) != original:
            raise ValueError('compact archive round-trip failed')
    return packed


def config(data):
    # Preserve original first-match case-insensitive semantics and quoted comments.
    sections = {}
    current = ''
    for line in data.decode('utf-8-sig').splitlines():
        line = re.split(r';(?=(?:[^\"]*\"[^\"]*\")*[^\"]*$)', line, maxsplit=1)[0].strip()
        if not line:
            continue
        if line.startswith('[') and line.endswith(']'):
            current = line[1:-1].strip().lower()
            sections.setdefault(current, {})
        elif '=' in line:
            key, value = line.split('=', 1)
            sections.setdefault(current, {}).setdefault(key.strip().lower(), value.strip().strip('"'))
    return sections


def selected_engine_names(table, quad=1):
    data = config(table)
    bank = int(data['enginesets'][f'quad{quad}'].split(',')[0])
    names = set()
    for sound in ('Idle', 'LowAccel', 'MidAccel', 'HighAccel', 'Power', 'Sputter', 'Decel', 'Rev', 'Reset'):
        section = data[f'{sound}-{bank}'.lower()]
        count = int(section['numsounds'])
        if not 0 <= count <= 128:
            raise ValueError('invalid engine bank count')
        for i in range(1, count + 1):
            name = section[f'sound_{i}'].replace('\\', '/').lower()
            names.add(name.removeprefix('audio/'))
    if quad == 1 and len(names) != 21:
        raise ValueError(f'exact-build ATV01 expected 21 unique engine samples, got {len(names)}')
    return names


def selected_train_names(toy):
    """Only source-bound Arlington body sounds used by the authored runtime."""
    data = config(toy)
    count = int(data.get('animations', {}).get('numberofanimations', '0'))
    if not 0 <= count <= 4096:
        raise ValueError('invalid train animation count')
    names = set()
    for index in range(1, count + 1):
        section = data.get(f'animation{index}', {})
        route = section.get('vue', '').replace('\\', '/').split('/')[-1].lower()
        if section.get('animationtype', '').lower() != 'procedural' or route != 'locomotive.vue':
            continue
        sounds = int(section.get('numberofloopingsounds', '0'))
        if not 0 <= sounds <= 32:
            raise ValueError('invalid train loop count')
        for sound in range(1, sounds + 1):
            loop = data[f'animation{index}loopsound{sound}']
            if loop.get('attachtoobject', '').lower() != 'body':
                raise ValueError('unsupported train audio attachment')
            name = loop['soundresourcefile'].replace('\\', '/').lower().removeprefix('audio/')
            if not name or '..' in name or name.startswith('/') or len(name) > 260:
                raise ValueError('invalid train sample path')
            if not name.endswith('.vag'):
                name += '.vag'
            names.add(name)
    return names


def selected_body_impact_names(table):
    """Rider BodyImpact samples only; unused voice/wreck banks stay excluded."""
    section = config(table).get('bodyimpact', {})
    count = int(section.get('numsounds', '0'))
    if not 1 <= count <= 70:
        raise ValueError('invalid rider BodyImpact bank count')
    names = set()
    for index in range(1, count + 1):
        name = section[f'sound_{index}'].replace('\\', '/').lower().removeprefix('audio/')
        if not name or '..' in name or name.startswith('/') or len(name) > 260 or not name.endswith('.vag'):
            raise ValueError('invalid rider BodyImpact sample path')
        names.add(name)
    if len(names) != count:
        raise ValueError('duplicated rider BodyImpact sample')
    return names


def mesh_material_names(data):
    """Read the bounded PSG v3 material table, as in atv-core's mesh reader."""
    if take(data, 0, 4) != b'psg\0' or u32(data, 4) != 3:
        raise ValueError('unsupported PSG signature/version')
    nodes = u32(data, 8)
    if not 1 <= nodes <= 256:
        raise ValueError('unsupported PSG node count')
    at = 12 + nodes * 224
    count = u32(data, at)
    if not 1 <= count <= 4096:
        raise ValueError('unsupported PSG material count')
    return {take(data, at + 4 + i * 64, 64).split(b'\0', 1)[0].decode('utf-8').lower()
            for i in range(count)}


def ai_outfit_names(ai_riders):
    """Outfit material stems named by AIRiders.pb (Helmet/Glove/.../Shirt)."""
    fields = ('helmet', 'glove', 'boot', 'sleeve', 'pant', 'goggle', 'shirt')
    names = {value.replace('\\', '/').rsplit('/', 1)[-1].lower()
             for name, section in config(ai_riders).items() if name.startswith('ai')
             for key, value in section.items() if key in fields and value}
    # Per-rider `_aiN` jerseys are composed at runtime; ship the base shirt's
    # mesh-slot variant used by the rider mesh (`atv2_shirt001`).
    return {re.sub(r'_ai\d+$', '001', name) if re.search(r'shirt_ai\d+$', name) else name for name in names}


def gear_catalog_names(rideinfo):
    """In-game material candidates for every uiRider.txt gear item."""
    catalog = config(Resource(rideinfo).named('uirider.txt'))
    names = set()
    for section in ('helmets', 'gloves', 'boots', 'shirts', 'sleeves', 'pants', 'goggles'):
        values = catalog.get(section, {})
        for i in range(1, int(values.get('numitems', '0')) + 1):
            base = values.get(f'texture_{i}', '').strip('"').replace('\\', '/').rsplit('/', 1)[-1].lower().removesuffix('_ui')
            names.update({base, f'{base}001', f'{base}s'} if base else ())
    return names


def compact_gear_catalog(rideinfo):
    """Only the Profile Store catalog text; store artwork stays on the disc."""
    return pack_resource([('ui/frontend/uiRider.txt', Resource(rideinfo).named('uirider.txt'))])


def compact_rider_materials(mesh_archive, material_archive, additional_meshes=(), extra_names=()):
    """Keep every material/pass texture referenced by the packaged rider meshes
    and by the original CPU rider outfits."""
    materials = Resource(material_archive)
    names = set()
    for source in (mesh_archive, *additional_meshes):
        meshes = Resource(source)
        for i, (name, _, _) in enumerate(meshes.members):
            if name.lower().endswith('.psg'):
                names.update(mesh_material_names(meshes.extract(i)))
    indices = {}
    for i, (name, _, _) in enumerate(materials.members):
        basename = name.lower().split('/')[-1]
        if basename in indices:
            raise ValueError('ambiguous rider material member')
        indices[basename] = i
    # AIRiders names some outfits the disc does not ship (e.g. skin_black_shirt_ai12);
    # those slots keep the mesh's default material, so only present ones are added.
    names.update(name for name in extra_names
                 if f'{name}.psm' in indices or f'{name}.pst' in indices)
    selected = set()
    for name in names:
        psm = indices.get(f'{name}.psm')
        if psm is None:
            textures = {name}
        else:
            selected.add(psm)
            textures = {value.lower() for section in config(materials.extract(psm)).values()
                        for key, value in section.items() if key.startswith('texturemap_') and value}
        for texture in textures:
            if f'{texture}.pst' not in indices:
                raise ValueError(f'missing referenced rider texture: {texture}')
            selected.add(indices[f'{texture}.pst'])
    return pack_resource([(materials.members[i][0], materials.extract(i)) for i in sorted(selected)])


def compact_player_archive(data, path):
    """Only mesh/catalog or live quad PHY/AO/AR; no unused VUT/arm resources."""
    archive = Resource(data)
    stem = path.rsplit('/', 1)[-1].removesuffix('.RES').lower()
    required = ({f'{stem}.psg', f'{stem}s01.phy', f'{stem}ao.col', f'{stem}ar.col', 'quaddust.emt', 'quaddirtchunk.emt', 'quaddirtspray.emt'}
                if path.startswith('/QUAD/') else {f'{stem}.psg', f'{stem}.mcf'})
    selected = [(name, archive.extract(i)) for i, (name, _, _) in enumerate(archive.members)
                if name.lower().rsplit('/', 1)[-1] in required]
    selected_names = [name.lower().rsplit('/', 1)[-1] for name, _ in selected]
    if len(selected_names) != len(set(selected_names)) or set(selected_names) != required:
        raise ValueError(f'missing or duplicated live player resource in {path}')
    return pack_resource(selected)


def compact_rider_motions(data, catalogs):
    """Only the MOT files used by the connected male/female body catalogs."""
    required = set()
    for catalog in catalogs:
        table = config(catalog)
        count = int(table['general info']['numberofmotions'])
        if not 1 <= count <= 512:
            raise ValueError('invalid rider motion catalog count')
        for i in range(1, count + 1):
            path = table[f'motion {i}']['vue'].replace('\\', '/')
            if not path or '..' in path or ':' in path:
                raise ValueError('unsafe rider motion path')
            required.add(path.rsplit('/', 1)[-1].rsplit('.', 1)[0].lower() + '.mot')
    archive = Resource(data)
    selected = [(name, archive.extract(i)) for i, (name, _, _) in enumerate(archive.members)
                if name.lower().rsplit('/', 1)[-1] in required]
    selected_names = [name.lower().rsplit('/', 1)[-1] for name, _ in selected]
    if len(selected_names) != len(set(selected_names)) or set(selected_names) != required:
        raise ValueError('missing or duplicated connected rider motion')
    return pack_resource(selected)


def compact_map_placements(data, environment):
    """ENDL supplies only the common placement TOY; FR owns map rendering data."""
    resource = Resource(data)
    matches = [i for i, (name, _, _) in enumerate(resource.members)
               if name.lower().split('/')[-1] == f'{environment.lower()}.toy']
    if len(matches) != 1:
        raise ValueError('expected one common map placement member')
    index = matches[0]
    return pack_resource([(resource.members[index][0], resource.extract(index))])


def compact_vancouver_thumbnail(data):
    """Only the two source Vancouver preview halves used by the selector."""
    resource = Resource(data)
    names = {'trk_free05_a.pst', 'trk_free05_b.pst'}
    selected = [(name, resource.extract(i)) for i, (name, _, _) in enumerate(resource.members)
                if name.lower().rsplit('/', 1)[-1] in names]
    if len(selected) != 2 or {name.lower().rsplit('/', 1)[-1] for name, _ in selected} != names:
        raise ValueError('missing or duplicated Vancouver thumbnail halves')
    return pack_resource(selected)


def shared_freestyle_overlays(data, environment_archive):
    """Common jump HUD art for Freestyle on venues such as the hockey arena.

    The arena's SO archive intentionally lacks jump HUD textures. Aggregate the
    five original ENV01 preload images into the small shared overlay archive
    instead of fetching a second full environment just for those images.
    """
    archive, environment = Resource(data), Resource(environment_archive)
    names = {'preloadblue.pst', 'preloadbkg.pst', 'preloadhighlight.pst',
             'preloadorange.pst', 'preloadblocker3.pst'}
    members = [(name, archive.extract(i)) for i, (name, _, _) in enumerate(archive.members)]
    available = {name.lower().rsplit('/', 1)[-1] for name, _ in members}
    for name in sorted(names - available):
        members.append((f'textures/{name}', environment.named(name)))
    return pack_resource(members)


MX_SOURCE_SHA = '4b74c0c76ffe0838e00c1b9f75b31e1bb91adae06ec9da1416355b363572102a'


def validate_motorcycle_resource(record, payload):
    if record.get('source_build_sha256') != MX_SOURCE_SHA or not record.get('repacked'):
        raise ValueError('unsupported motorcycle source build')
    archive = Resource(payload)
    if record.get('member_count') != len(archive.members):
        raise ValueError('motorcycle member count mismatch')
    for name, _, _ in archive.members:
        # Chassis whisker rays are the only approved collision members.
        chassis = re.fullmatch(r'Veh/Col/(?:bikem|WreckBikem|bike50m|WreckBike50m)\.col', name)
        if not chassis and not re.fullmatch(r'(?:Veh|Rider|Aud|Part)/[A-Za-z0-9_ /.-]+\.(?:sub|psg|psm|pst|mot|mcf|ini|phy|aud|vag|emt|prt)', name, re.I):
            raise ValueError('unapproved motorcycle runtime member')
        if '..' in name or name.startswith('/'):
            raise ValueError('invalid motorcycle member path')


def validated_content(directory):
    """Validate and return only manifest-authorized runtime files for Sites."""
    directory = Path(directory)
    manifest = directory / 'manifest.json'
    if manifest.is_symlink() or manifest.stat().st_size > 1024 * 1024:
        raise ValueError('invalid hosted content manifest')
    data = json.loads(manifest.read_text())
    if data.get('version') != 1 or data.get('source_build_sha256') != SOURCE_SHA:
        raise ValueError('unsupported hosted content build')
    records = data.get('files', [])
    if not 1 <= len(records) <= 512:
        raise ValueError('invalid hosted content count')
    allowed = {manifest}
    for record in records:
        if record['path'] not in STARTUP and record['path'] != '/AUDIO/AUDIO.RES' and not re.fullmatch(r'/MX/BIKE(?:50|125|250|500)\.RES', record['path']) and not re.fullmatch(r'/AUDIO/MUSIC/[A-Z0-9_]+\.VAG', record['path']):
            raise ValueError('unapproved hosted resource path')
        chunks = record.get('chunks', [dict(record, offset=0)])
        end = 0
        whole = hashlib.sha256()
        for chunk in chunks:
            digest = chunk['sha256']
            if not re.fullmatch(r'[a-f0-9]{64}', digest) or chunk['url'] != f'/content/{digest}.bin' or chunk['offset'] != end:
                raise ValueError('invalid content-addressed reference')
            path = directory / f'{digest}.bin'
            if path.is_symlink() or not 0 < path.stat().st_size <= MAX_FILE:
                raise ValueError('invalid hosted content blob')
            payload = path.read_bytes()
            if len(payload) != chunk['bytes'] or hashlib.sha256(payload).hexdigest() != digest:
                raise ValueError('hosted content digest mismatch')
            whole.update(payload)
            end += len(payload)
            allowed.add(path)
        if record['path'].startswith('/MX/'):
            if len(chunks) != 1:
                raise ValueError('motorcycle runtime archive must be compact')
            validate_motorcycle_resource(record, payload)
        if end != record['bytes'] or whole.hexdigest() != record['sha256']:
            raise ValueError('hosted resource digest mismatch')
    return sorted(allowed)


def compact_menu_riders(data, materials, audio_definition):
    """Only the two rendered menu rigs, body clips, seven base maps and UI audio config."""
    archive = Resource(data)
    required = {'mrider_ui.psg', 'frider_ui.psg'}
    for stem, count in [('mrider', 7), ('frider', 5)]:
        required.add(f'{stem}_body_breathe.mot')
        required.update(f'{stem}_body_idle{i:02}.mot' for i in range(1, count + 1))
    selected = [(name, archive.extract(i)) for i, (name, _, _) in enumerate(archive.members) if name.lower().split('/')[-1] in required]
    if len(selected) != len(required):
        raise ValueError('menu rider resources missing or duplicated')
    textures = Resource(materials)
    for name in ['atv2_helmet_ui', 'atv2_shirt_ui', 'atv2_black_gloves_ui', 'atv2_black_boots_ui', 'atv2_black_goggles_ui', 'atv2_pants_ui', 'atv2_sleeves_ui']:
        selected.append((f'ui/frontend/textures/{name}.pst', textures.named(f'{name}.pst')))
    selected.append(('audio/UI/uimusic.aud', audio_definition))
    return pack_resource(selected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--iso', type=Path, default=ROOT / 'private/source/ATV Offroad Fury 2 (USA) (v2.00).iso')
    parser.add_argument('--output', type=Path, default=ROOT / 'private/reports/hosted-content')
    args = parser.parse_args()
    output = args.output.resolve()
    private = (ROOT / 'private').resolve()
    if not output.is_relative_to(private):
        parser.error('generated content must stay under ignored private/ storage')
    entries = json.loads((ROOT / 'private/reports/disc-files.json').read_text())
    by_path = {entry['path'].lower(): entry for entry in entries if not entry['directory']}
    output.mkdir(parents=True, exist_ok=True)
    with args.iso.open('rb') as disc:
        def source(path):
            entry = by_path[path.lower()]
            if not 0 <= entry['bytes'] <= 128 * 1024 * 1024:
                raise ValueError('source file size exceeds bound')
            disc.seek(entry['lba'] * 2048)
            data = disc.read(entry['bytes'])
            if len(data) != entry['bytes']:
                raise ValueError('truncated disc input')
            return data
        if hashlib.sha256(source('/SCUS_972.11')).hexdigest() != SOURCE_SHA:
            raise ValueError('source build does not match USA v2.00')
        def blob(data):
            if not 0 < len(data) <= MAX_FILE:
                raise ValueError('hosted file exceeds 25MiB static limit')
            digest = hashlib.sha256(data).hexdigest()
            path = output / f'{digest}.bin'
            if path.exists():
                if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
                    raise ValueError('existing content-addressed file has wrong digest')
            else:
                path.write_bytes(data)
            return {'bytes': len(data), 'sha256': digest, 'url': f'/content/{digest}.bin'}
        records = []
        originals = {}
        for path in STARTUP:
            data = source(path)
            originals[path] = data
            if (path.startswith('/QUAD/') and not path.endswith('_MT.RES')) or path in ('/RIDER/MRIDER.RES', '/RIDER/FRIDER.RES'):
                data = compact_player_archive(data, path)
            elif path == '/UI/RESOURCE/PAGEA/PAGE07.RES':
                data = compact_menu_riders(data, source('/UI/RESOURCE/RIDEINFO.RES'), source('/AUDIO/UI/UIMUSIC.AUD'))
            elif path == '/OVERLAYS.RES':
                data = shared_freestyle_overlays(data, originals['/TRACKS/TRACKSA/ENV01/E01_FR.RES'])
            elif path == '/ANIMSRDR.RES':
                catalogs = [Resource(originals[f'/RIDER/{stem}.RES']).named(stem.lower() + '.mcf') for stem in ('MRIDER', 'FRIDER')]
                data = compact_rider_motions(data, catalogs)
            elif path == '/UI/RESOURCE/RIDEINFO.RES':
                data = compact_gear_catalog(data)
            elif path.endswith('_ENDL.RES'):
                data = compact_map_placements(data, path.split('/')[-2])
            elif path == '/RIDER/RIDER_MT.RES':
                outfits = ai_outfit_names(Resource(originals['/TRACKS/TRACKSA/ENV01/E01_NAT.RES']).named('airiders.pb'))
                outfits |= gear_catalog_names(source('/UI/RESOURCE/RIDEINFO.RES'))
                data = compact_rider_materials(originals['/RIDER/MRIDER.RES'], data, (originals['/RIDER/FRIDER.RES'],), outfits)
            record = {'path': path, 'lba': by_path[path.lower()]['lba'], 'directory': False, 'lazy': path.startswith(('/QUAD/', '/UI/LOADICON/')) or path in ('/RIDER/FRIDER.RES',), **blob(data)}
            if data != originals[path]:
                record.update(repacked=True, member_count=len(Resource(data).members))
            records.append(record)
        engine_names = set().union(*(selected_engine_names(Resource(originals['/AUDIOTBL.RES']).named('vehicle.aud'), quad) for quad in range(1, 27)))
        train_names = selected_train_names(Resource(originals['/TRACKS/TRACKSA/ENV08/E08_FR.RES']).named('env08fr.toy'))
        if len(train_names) != 3:
            raise ValueError('exact-build train expected three unique body sounds')
        body_names = selected_body_impact_names(Resource(originals['/AUDIOTBL.RES']).named('rider.aud'))
        if len(body_names) != 4:
            raise ValueError('exact-build rider BodyImpact bank expected four samples')
        menu_names = {f'ui/uitune{i}{channel}.vag' for i in range(1, 5) for channel in 'lr'}
        weather_names = {'rain.vag'} #00213bf0, only the active scene Rain loop.
        # Race start: 321 countdown beep/go (002e79a0) and the Supercross gate drop.
        race_names = {'beep01.vag', 'go.vag', 'gate.vag'}
        # Supercross stadium crowd (CrowdPresent): the venue Soultree ambient
        # loops (<venue>.aud); crowd.aud itself ships in each race archive.
        crowd_names = {'crowd/crowdf1/crdloop3.vag', 'crowd/crowdf1/crdloop4.vag',
                       'crowd/sx/crdloop5.vag', 'crowd/sx/crdloop6.vag'}
        names = engine_names | train_names | body_names | menu_names | weather_names | race_names | crowd_names
        audio = Resource(source('/AUDIO/AUDIO.RES'))
        samples = [(name, audio.extract(i)) for i, (name, _, _) in enumerate(audio.members) if name.lower().removeprefix('audio/') in names]
        if len(samples) != len(names):
            raise ValueError('missing or duplicated selected runtime sound sample')
        # The crowd reaction clips are loose disc files beside crowd.aud.
        crowd_clips = sorted(path for path in by_path if re.fullmatch(r'/audio/crowd/(amcrowd|procrowd)/[a-z0-9]+\.vag', path))
        if len(crowd_clips) != 18:
            raise ValueError('exact-build crowd reactions expected 18 clips')
        samples += [(path.removeprefix('/'), source(by_path[path]['path'])) for path in crowd_clips]
        # Race intro/outro stingers (IntOut\intout.aud): INT001-INT006 at race
        # load, OUT004 (win) and the OUT001 loop at race over (00217bb0, 00216260).
        intout = sorted(path for path in by_path if re.fullmatch(r'/audio/intout/(int00[1-6]|out00[14])\.vag', path))
        if len(intout) != 8:
            raise ValueError('exact-build IntOut expected eight stereo cues')
        samples += [(path.removeprefix('/'), source(by_path[path]['path'])) for path in intout]
        packed = pack_resource(samples)
        records.append({'path': '/AUDIO/AUDIO.RES', 'lba': by_path['/audio/audio.res']['lba'], 'directory': False, 'lazy': False, 'repacked': True, 'member_count': len(samples), **blob(packed)})
        music = config(Resource(originals['/STARTAPP.RES']).named('music.pb'))
        count = int(music['songs']['numberofsongs'])
        if count != 21:
            raise ValueError('exact-build music catalog expected 21 songs')
        for i in range(1, count + 1):
            path = '/AUDIO/' + music[f'song {i}']['filename'].replace('\\', '/')
            entry = by_path[path.lower()]
            data = source(path)
            if data[:4] != b'VAGp' and data[:12] != bytes(12):
                raise ValueError('music source is not VAG')
            chunks = [{'offset': offset, **blob(data[offset:offset + CHUNK])} for offset in range(0, len(data), CHUNK)]
            records.append({'path': entry['path'], 'lba': entry['lba'], 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'directory': False, 'lazy': True, 'chunks': chunks})
        manifest = {'version': 1, 'build': 'USA v2.00', 'source_build_sha256': SOURCE_SHA, 'engine_sample_count': len(engine_names), 'train_sample_count': len(train_names), 'rider_body_sample_count': len(body_names), 'weather_sample_count': len(weather_names), 'music_song_count': count, 'files': records}
        temporary = output / 'manifest.json.tmp'
        temporary.write_text(json.dumps(manifest, indent=2) + '\n')
        temporary.replace(output / 'manifest.json')
        print(json.dumps({'manifest': str(output / 'manifest.json'), 'records': len(records), 'startup_bytes': sum(r['bytes'] for r in records if not r['lazy']), 'music_bytes': sum(r['bytes'] for r in records if r['path'].endswith('.VAG')), 'audio_archive_bytes': len(packed), 'blobs': len(list(output.glob('*.bin')))}))


if __name__ == '__main__':
    main()
