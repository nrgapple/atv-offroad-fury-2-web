"""Validate the separate, private original-logo app icon package."""
import hashlib
import re
import struct


def validated_icons(metadata, directory):
    if metadata.get('version') != 1 or len(metadata.get('icons', [])) != 3:
        raise ValueError('expected three app icon records')
    records = metadata['icons']
    if {record.get('size') for record in records} != {180, 192, 512}:
        raise ValueError('unexpected app icon sizes')
    result = []
    for record in records:
        digest = record.get('sha256', '')
        if not re.fullmatch('[a-f0-9]{64}', digest) or record.get('url') != f'/app-icons/{digest}.png':
            raise ValueError('unsafe app icon URL')
        path = directory / f'{digest}.png'
        if path.is_symlink():
            raise ValueError('symlinked app icon')
        data = path.read_bytes()
        if len(data) != record.get('bytes') or not 24 <= len(data) <= 1_000_000 or hashlib.sha256(data).hexdigest() != digest:
            raise ValueError('app icon checksum/size mismatch')
        if data[:8] != b'\x89PNG\r\n\x1a\n' or data[12:16] != b'IHDR' or struct.unpack('>II', data[16:24]) != (record['size'], record['size']):
            raise ValueError('invalid app icon PNG dimensions')
        result.append((record['url'].lstrip('/'), data))
    return result
