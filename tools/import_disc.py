#!/usr/bin/env python3
"""Import the supplied ZIP/ISO and record evidence. No external dependencies."""
import argparse
import hashlib
import json
import pathlib
import re
import struct
import zipfile

SECTOR = 2048

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for b in iter(lambda: f.read(8 * 1024 * 1024), b''):
            h.update(b)
    return h.hexdigest()

def read_at(f, offset, size):
    f.seek(offset)
    data = f.read(size)
    if len(data) != size:
        raise ValueError(f'truncated image at {offset}')
    return data

def iso_entries(f):
    pvd = None
    for sector in range(16, 64):
        b = read_at(f, sector * SECTOR, SECTOR)
        if b[1:6] != b'CD001':
            raise ValueError('invalid ISO9660 descriptor')
        if b[0] == 1:
            pvd = b
            break
        if b[0] == 255:
            break
    if pvd is None:
        raise ValueError('no primary volume descriptor')
    root = pvd[156:190]
    entries = []
    visited = set()
    def walk(lba, size, prefix):
        if (lba, size) in visited:
            return
        visited.add((lba, size))
        data = read_at(f, lba * SECTOR, size)
        pos = 0
        while pos < len(data):
            n = data[pos]
            if not n:
                pos = (pos // SECTOR + 1) * SECTOR
                continue
            rec = data[pos:pos+n]
            if len(rec) < 34 or 33 + rec[32] > len(rec):
                raise ValueError('invalid directory record')
            name = rec[33:33+rec[32]]
            pos += n
            if name in (b'\0', b'\1'):
                continue
            name = name.decode('ascii').split(';')[0]
            path = prefix + '/' + name
            block = struct.unpack_from('<I', rec, 2)[0]
            count = struct.unpack_from('<I', rec, 10)[0]
            directory = bool(rec[25] & 2)
            entries.append(dict(path=path, lba=block, bytes=count, directory=directory))
            if directory:
                walk(block, count, path)
    walk(struct.unpack_from('<I', root, 2)[0], struct.unpack_from('<I', root, 10)[0], '')
    return pvd[40:72].decode('ascii').strip(), entries

def elf_report(data):
    if data[:7] != b'\x7fELF\x01\x01\x01':
        raise ValueError('expected ELF32 little endian')
    hdr = struct.unpack_from('<HHIIIIIHHHHHH', data, 16)
    _, machine, _, entry, phoff, shoff, flags, _, phsize, phnum, shsize, shnum, shstr = hdr
    if machine != 8:
        raise ValueError('expected MIPS')
    sections = [struct.unpack_from('<IIIIIIIIII', data, shoff+i*shsize) for i in range(shnum)]
    def cstring(blob, off):
        return blob[off:blob.find(b'\0', off)].decode('utf8', 'replace')
    names = data[sections[shstr][4]:sections[shstr][4]+sections[shstr][5]]
    section_report = [dict(name=cstring(names,s[0]),type=s[1],address=s[3],offset=s[4],bytes=s[5]) for s in sections]
    symbols = []
    for s in sections:
        if s[1] != 2:
            continue
        st = sections[s[6]]
        strings = data[st[4]:st[4]+st[5]]
        for pos in range(s[4],s[4]+s[5],s[9] or 16):
            name, value, size, info, other, index = struct.unpack_from('<IIIBBH', data, pos)
            symbols.append(dict(name=cstring(strings,name),address=value,bytes=size,type=info&15,section=index))
    segments = []
    for i in range(phnum):
        p = struct.unpack_from('<IIIIIIII',data,phoff+i*phsize)
        segments.append(dict(type=p[0],offset=p[1],address=p[2],file_bytes=p[4],memory_bytes=p[5],flags=p[6]))
    functions = [s for s in symbols if s['type']==2 and s['section'] != 0 and s['address']]
    return dict(entry=entry,machine=machine,flags=flags,sections=section_report,segments=segments,symbols=symbols,functions=functions)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('source',type=pathlib.Path)
    ap.add_argument('--root',type=pathlib.Path,default=pathlib.Path(__file__).resolve().parents[1])
    args = ap.parse_args()
    private = args.root / 'private'
    reports = private / 'reports'
    extracted = private / 'extracted'
    for p in (reports,extracted):
        p.mkdir(parents=True,exist_ok=True)
    iso = args.source
    if zipfile.is_zipfile(iso):
        with zipfile.ZipFile(iso) as z:
            members = [i for i in z.infolist() if i.filename.lower().endswith('.iso')]
            if len(members) != 1:
                raise ValueError('archive must contain exactly one ISO')
            member = members[0]
            iso = private / 'source' / pathlib.Path(member.filename).name
            if not iso.exists():
                tmp = iso.with_suffix('.iso.partial')
                print('Extracting ISO',flush=True)
                with z.open(member) as src, tmp.open('wb') as out:
                    for b in iter(lambda: src.read(8*1024*1024),b''):
                        out.write(b)
                tmp.rename(iso)
            if iso.stat().st_size != member.file_size:
                raise ValueError('existing ISO size mismatch')
    with iso.open('rb') as f:
        volume, entries = iso_entries(f)
        cnf = next(e for e in entries if e['path'].upper()=='/SYSTEM.CNF')
        config = read_at(f,cnf['lba']*SECTOR,cnf['bytes']).decode('ascii')
        match = re.search(r'BOOT2\s*=\s*cdrom0:\\([^;\s]+)',config,re.I)
        if not match:
            raise ValueError('missing BOOT2')
        executable = next(e for e in entries if e['path'].upper()=='/'+match[1].upper())
        data = read_at(f,executable['lba']*SECTOR,executable['bytes'])
        (extracted / match[1]).write_bytes(data)
        (extracted / 'SYSTEM.CNF').write_text(config)
        elf = elf_report(data)
        (reports / 'disc-files.json').write_text(json.dumps(entries,indent=2))
        (reports / 'elf.json').write_text(json.dumps(elf,indent=2))
    summary = dict(game='ATV Offroad Fury 2',source_archive=args.source.name,iso_bytes=iso.stat().st_size,iso_sha256=digest(iso),elf=match[1],elf_sha256=hashlib.sha256(data).hexdigest(),volume=volume,system_cnf=config,entry=hex(elf['entry']),file_count=sum(not e['directory'] for e in entries),symbol_count=len(elf['symbols']),function_count=len(elf['functions']),status='imported; game runtime not implemented')
    (reports / 'import.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary,indent=2))
    print('Root files:',[(e['path'],e['bytes']) for e in entries if e['path'].count('/')==1])

if __name__=='__main__':
    main()
