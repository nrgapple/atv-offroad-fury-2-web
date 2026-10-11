#!/usr/bin/env python3
"""Package explicitly authorized original content for the hosted ENV01 slice.

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
    '/TRACKS/TRACKSA/ENV01/E01_FR.RES', '/TRACKS/TRACKSA/ENV01/E01_ENDL.RES',
    '/QUAD/BIKESA/ATV01.RES', '/QUAD/BIKESA/ATV01_MT.RES',
    '/RIDER/MRIDER.RES', '/RIDER/RIDER_MT.RES', '/ANIMSRDR.RES',
    '/AUDIOTBL.RES', '/STARTAPP.RES',
    '/TRACKS/TRACKSA/ENV02/E02_FR.RES', '/TRACKS/TRACKSA/ENV02/E02_ENDL.RES',
    '/UI/RESOURCE/TP/TP_ANAT.RES',
    '/UI/RESOURCE/TITLE.RES', '/UI/RESOURCE/PAGEA/PAGE01.RES',
    '/UI/RESOURCE/PAGEA/PAGE02.RES', '/UI/RESOURCE/PAGEA/PAGE06.RES',
)


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
        raise ValueError('empty compact audio archive')
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


def selected_engine_names(table):
    data = config(table)
    bank = int(data['enginesets']['quad1'].split(',')[0])
    names = set()
    for sound in ('Idle', 'LowAccel', 'MidAccel', 'HighAccel', 'Power', 'Sputter', 'Decel', 'Rev', 'Reset'):
        section = data[f'{sound}-{bank}'.lower()]
        count = int(section['numsounds'])
        if not 0 <= count <= 128:
            raise ValueError('invalid engine bank count')
        for i in range(1, count + 1):
            name = section[f'sound_{i}'].replace('\\', '/').lower()
            names.add(name.removeprefix('audio/'))
    if len(names) != 21:
        raise ValueError(f'exact-build ATV01 expected 21 unique engine samples, got {len(names)}')
    return names


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
        if record['path'] not in STARTUP and record['path'] != '/AUDIO/AUDIO.RES' and not re.fullmatch(r'/AUDIO/MUSIC/[A-Z0-9_]+\.VAG', record['path']):
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
        if end != record['bytes'] or whole.hexdigest() != record['sha256']:
            raise ValueError('hosted resource digest mismatch')
    return sorted(allowed)


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
            records.append({'path': path, 'lba': by_path[path.lower()]['lba'], 'directory': False, 'lazy': False, **blob(data)})
        names = selected_engine_names(Resource(originals['/AUDIOTBL.RES']).named('vehicle.aud'))
        audio = Resource(source('/AUDIO/AUDIO.RES'))
        samples = [(name, audio.extract(i)) for i, (name, _, _) in enumerate(audio.members) if name.lower().removeprefix('audio/') in names]
        if len(samples) != len(names):
            raise ValueError('missing or duplicated selected engine sample')
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
        manifest = {'version': 1, 'build': 'USA v2.00', 'source_build_sha256': SOURCE_SHA, 'engine_sample_count': len(samples), 'music_song_count': count, 'files': records}
        temporary = output / 'manifest.json.tmp'
        temporary.write_text(json.dumps(manifest, indent=2) + '\n')
        temporary.replace(output / 'manifest.json')
        print(json.dumps({'manifest': str(output / 'manifest.json'), 'records': len(records), 'startup_bytes': sum(r['bytes'] for r in records if not r['lazy']), 'music_bytes': sum(r['bytes'] for r in records if r['lazy']), 'audio_archive_bytes': len(packed), 'blobs': len(list(output.glob('*.bin')))}))


if __name__ == '__main__':
    main()
