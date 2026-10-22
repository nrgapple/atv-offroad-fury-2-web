#!/usr/bin/env python3
"""Flatten the private hosted content package into one pack file for desktop builds.

The pack (format documented in crates/atv-content/src/pack.rs) is embedded in, or
shipped beside, the standalone desktop executables. It is game content: keep it in
ignored storage (default ~/LargeFiles/fury-desktop/packs on the SSD), never in Git
or CI. Only the manifest's authorized runtime files (.RES/.VAG/.LSI) are accepted,
with the soundtrack's AAC (.m4a) remaster songs the web streams (the desktop app
plays them; --no-streams leaves them out, and the app then plays the original
VAGs); the ISO, BIOS, boot executable and anything else are refused.

    tools/pack_content.py build [--source DIR] [--output FILE] [--no-streams]
    tools/pack_content.py verify PACK
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
MAGIC = b'FURYPAK1'
VERSION = 1
HEADER = 64
ENTRY = 48
SOURCE_SHA = '11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49'
ALLOWED_EXTENSIONS = ('.RES', '.VAG', '.LSI')
PATH_RE = re.compile(r'^/[A-Za-z0-9_./-]+$')
DIGEST_RE = re.compile(r'^[0-9a-f]{64}$')
FORBIDDEN = re.compile(r'(SCUS|\.ISO$|\.BIN$|\.ELF$|BIOS|\.CUE$|\.ZIP$)', re.IGNORECASE)
MAX_BLOB = 64 * 1024 * 1024
TRAILER_MAGIC = b'FURYEXE1'
TRAILER = 16
DEFAULT_SOURCE = ROOT / 'private/reports/hosted-content'
DEFAULT_PACK_DIR = Path.home() / 'LargeFiles/fury-desktop/packs'


class PackError(Exception):
    pass


def _digest(value, what):
    if not isinstance(value, str) or not DIGEST_RE.match(value):
        raise PackError(f'bad digest in {what}')
    return value


def authorized_blobs(manifest, include_streams=False):
    """Validate the manifest; return `{digest: (size, extension)}` of every blob it needs."""
    if manifest.get('version') != 1 or manifest.get('source_build_sha256') != SOURCE_SHA:
        raise PackError('manifest is not the USA v2.00 runtime content package')
    blobs = {}

    def add(digest, size, extension, what):
        _digest(digest, what)
        if not isinstance(size, int) or not 0 < size <= MAX_BLOB:
            raise PackError(f'bad size in {what}')
        if blobs.setdefault(digest, (size, extension))[0] != size:
            raise PackError(f'conflicting sizes for {digest}')

    for record in manifest.get('files', []):
        path = record.get('path', '')
        if not PATH_RE.match(path) or '..' in path or FORBIDDEN.search(path):
            raise PackError(f'refusing unauthorized path {path!r}')
        if not path.endswith(ALLOWED_EXTENSIONS) or record.get('directory'):
            raise PackError(f'refusing file type outside the runtime list: {path}')
        chunks = record.get('chunks')
        if chunks is None:
            chunks = [{'offset': 0, 'bytes': record.get('bytes'), 'sha256': record.get('sha256')}]
        end = 0
        for chunk in chunks:
            if chunk.get('offset') != end:
                raise PackError(f'gap in {path}')
            add(chunk.get('sha256'), chunk.get('bytes'), 'bin', path)
            end += chunk['bytes']
        if end != record.get('bytes'):
            raise PackError(f'truncated record {path}')
        stream = record.get('stream')
        if include_streams and stream:
            if stream.get('type') != 'audio/mp4':
                raise PackError(f'unexpected stream type for {path}')
            add(stream.get('sha256'), stream.get('bytes'), 'm4a', path)
    if not blobs:
        raise PackError('manifest lists no files')
    return blobs


def write_pack(output, manifest_bytes, blobs, read_blob):
    """Write `output` atomically. `read_blob(digest)` returns verified bytes."""
    entries = sorted(blobs)
    manifest_at = HEADER + ENTRY * len(entries)
    offset = manifest_at + len(manifest_bytes)
    table = []
    # Blobs are laid out in digest order too, so a pack is reproducible.
    for digest in entries:
        size = blobs[digest][0]
        table.append((bytes.fromhex(digest), offset, size))
        offset += size
    header = (MAGIC + struct.pack('<IIQQ', VERSION, len(entries), manifest_at, len(manifest_bytes))
              + hashlib.sha256(manifest_bytes).digest())
    assert len(header) == HEADER
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    temp = output.with_name(output.name + '.partial')
    try:
        with open(temp, 'wb') as out:
            out.write(header)
            for key, at, size in table:
                out.write(key + struct.pack('<QQ', at, size))
            out.write(manifest_bytes)
            for digest in entries:
                data = read_blob(digest)
                if len(data) != blobs[digest][0] or hashlib.sha256(data).hexdigest() != digest:
                    raise PackError(f'integrity check failed for {digest}')
                out.write(data)
        os.replace(temp, output)
    finally:
        temp.unlink(missing_ok=True)


def refuse_tracked_destination(output):
    """The pack is game content: never write it where Git could pick it up."""
    output = Path(output).resolve()
    try:
        output.relative_to(ROOT)
    except ValueError:
        return
    ignored = subprocess.run(['git', 'check-ignore', '-q', str(output)], cwd=ROOT).returncode == 0
    if not ignored:
        raise PackError(f'refusing to write content inside the repository: {output}')


def build(source, output, include_streams=True):
    source = Path(source)
    manifest_bytes = (source / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_bytes)
    blobs = authorized_blobs(manifest, include_streams)
    for digest, (size, extension) in blobs.items():
        path = source / f'{digest}.{extension}'
        if path.is_symlink() or not path.is_file():
            raise PackError(f'missing package chunk {path.name}')
    output = Path(output) if output else DEFAULT_PACK_DIR / (
        'fury-content-' + hashlib.sha256(manifest_bytes).hexdigest()[:12]
        + ('' if include_streams else '-nostreams') + '.pack')
    refuse_tracked_destination(output)
    write_pack(output, manifest_bytes, blobs,
               lambda digest: (source / f'{digest}.{blobs[digest][1]}').read_bytes())
    return output


def read_pack(path, base=0, length=None):
    """Parse a pack stored at `base` in `path`; return `(manifest_bytes, {digest: (offset, size)})`.

    Offsets are relative to the pack start."""
    if length is None:
        length = os.path.getsize(path) - base
    with open(path, 'rb') as f:
        f.seek(base)
        header = f.read(HEADER)
        if len(header) < HEADER or header[:8] != MAGIC:
            raise PackError('not a content pack')
        version, count, manifest_at, manifest_len = struct.unpack('<IIQQ', header[8:32])
        if version != VERSION:
            raise PackError('unsupported pack version')
        if manifest_at + manifest_len > length or HEADER + ENTRY * count > length:
            raise PackError('truncated pack')
        table = f.read(ENTRY * count)
        f.seek(base + manifest_at)
        manifest_bytes = f.read(manifest_len)
    if hashlib.sha256(manifest_bytes).digest() != header[32:64]:
        raise PackError('manifest hash mismatch')
    entries = {}
    for i in range(count):
        key, at, size = struct.unpack('<32sQQ', table[i * ENTRY:(i + 1) * ENTRY])
        entries[key.hex()] = (at, size)
    return manifest_bytes, entries


def attached_region(path):
    """`(base, length)` of a pack appended to an executable, or None without a trailer."""
    total = os.path.getsize(path)
    if total < TRAILER:
        return None
    with open(path, 'rb') as f:
        f.seek(total - TRAILER)
        trailer = f.read(TRAILER)
    if trailer[:8] != TRAILER_MAGIC:
        return None
    length = struct.unpack('<Q', trailer[8:])[0]
    if length > total - TRAILER:
        raise PackError('attached pack is truncated')
    return total - TRAILER - length, length


def attach_pack(executable, pack, output):
    """Write `output` = content-free `executable` + `pack` + trailer (see pack.rs)."""
    if attached_region(executable) is not None:
        raise PackError('executable already carries a pack')
    verify(pack)
    size = os.path.getsize(pack)
    with open(output, 'wb') as out:
        for source in (executable, pack):
            with open(source, 'rb') as f:
                while chunk := f.read(8 << 20):
                    out.write(chunk)
        out.write(TRAILER_MAGIC + struct.pack('<Q', size))
    os.chmod(output, 0o755)


def verify(path, base=0, length=None):
    """Full check: authorized manifest, every blob present, hashed, nothing extra."""
    if length is None:
        length = os.path.getsize(path) - base
    manifest_bytes, entries = read_pack(path, base, length)
    manifest = json.loads(manifest_bytes)
    streams = any(entries.get(d) for d in _stream_digests(manifest))
    needed = authorized_blobs(manifest, include_streams=streams)
    if set(needed) != set(entries):
        raise PackError('pack contents differ from the manifest')
    with open(path, 'rb') as f:
        for digest, (at, size) in entries.items():
            if size != needed[digest][0] or at + size > length:
                raise PackError(f'bad entry {digest}')
            f.seek(base + at)
            if hashlib.sha256(f.read(size)).hexdigest() != digest:
                raise PackError(f'blob hash mismatch {digest}')
    return len(entries), length


def _stream_digests(manifest):
    return [r['stream']['sha256'] for r in manifest.get('files', []) if r.get('stream')]


def stream_count(path, base=0, length=None):
    """`(packed, listed)` soundtrack songs of a pack (the desktop app plays the packed ones)."""
    manifest_bytes, entries = read_pack(path, base, length)
    digests = _stream_digests(json.loads(manifest_bytes))
    return sum(1 for d in digests if d in entries), len(digests)


def write_fixture(output):
    """Synthetic two-file pack (no game data) used by the Rust and Python tests."""
    one, a, b = b'first file', b'chunk-a|', b'chunk-b'
    sha = lambda data: hashlib.sha256(data).hexdigest()
    manifest = {
        'version': 1,
        'source_build_sha256': SOURCE_SHA,
        'files': [
            {'path': '/MINI/ONE.RES', 'bytes': len(one), 'sha256': sha(one)},
            {'path': '/MINI/TWO.VAG', 'bytes': len(a + b), 'chunks': [
                {'offset': 0, 'bytes': len(a), 'sha256': sha(a)},
                {'offset': len(a), 'bytes': len(b), 'sha256': sha(b)}]},
        ],
    }
    manifest_bytes = json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()
    store = {sha(one): one, sha(a): a, sha(b): b}
    write_pack(output, manifest_bytes, authorized_blobs(manifest), store.__getitem__)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    b = sub.add_parser('build', help='build a pack from the hosted package')
    b.add_argument('--source', type=Path, default=DEFAULT_SOURCE)
    b.add_argument('--output', type=Path)
    b.add_argument('--no-streams', dest='include_streams', action='store_false',
                   help='leave out the soundtrack .m4a songs (the desktop app then plays the original VAGs)')
    v = sub.add_parser('verify', help='check a pack end to end')
    v.add_argument('pack', type=Path)
    f = sub.add_parser('fixture', help='write the synthetic test fixture')
    f.add_argument('output', type=Path)
    f.add_argument('--hex', action='store_true', help='write hex text (the committed form; Git tracks no binaries)')
    args = parser.parse_args(argv)
    try:
        if args.command == 'build':
            output = build(args.source, args.output, args.include_streams)
            count, size = verify(output)
            print(f'{output}\n{count} blobs, {size / 1e6:.1f} MB, sha256 {hashlib.sha256(output.read_bytes()).hexdigest()}')
        elif args.command == 'verify':
            count, size = verify(args.pack)
            print(f'ok: {count} blobs, {size / 1e6:.1f} MB')
        elif args.hex:
            write_fixture(args.output)
            data = args.output.read_bytes().hex()
            args.output.write_text('\n'.join(data[i:i + 96] for i in range(0, len(data), 96)) + '\n')
        else:
            write_fixture(args.output)
    except PackError as e:
        print(f'pack_content: {e}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
