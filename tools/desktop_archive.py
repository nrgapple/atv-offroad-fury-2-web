"""Archive layout, allowlist and content checks shared by the desktop build and publish tools.

A desktop archive carries the game's authorized runtime resources inside the
executable (appended pack) or the macOS bundle (Contents/Resources/content.pack).
These checks make sure nothing else rides along: the disc image, BIOS, boot
executable, analysis output or private reports.
"""
import hashlib
import os
from pathlib import Path
import re
import shutil
import tarfile
import tempfile
import zipfile

import pack_content

APP = 'Fury Remastered.app'
# Allowed member paths below the single top-level folder, per platform.
LAYOUTS = {
    'macos-universal': [
        re.escape(f'{APP}/Contents/') + p for p in (
            r'Info\.plist', r'MacOS/fury', r'Resources/content\.pack',
            r'Resources/fury\.icns', r'_CodeSignature/CodeResources')
    ] + [r'README\.txt'],
    'linux-x86_64': [r'fury', r'README\.txt'],
    'linux-aarch64': [r'fury', r'README\.txt'],
    'windows-x86_64': [r'fury\.exe', r'README\.txt'],
}
SUFFIX = {
    'macos-universal': '.zip', 'linux-x86_64': '.tar.gz',
    'linux-aarch64': '.tar.gz', 'windows-x86_64': '.zip',
}
# Belt and braces on top of the allowlist.
FORBIDDEN = re.compile(r'(SCUS|\.iso$|\.bin$|\.elf$|bios|\.cue$|private|reports|/work/|\.pi-web|\.git)',
                       re.IGNORECASE)
MAGIC = {
    'macos-universal': (b'\xca\xfe\xba\xbe',),
    'linux-x86_64': (b'\x7fELF',),
    'linux-aarch64': (b'\x7fELF',),
    'windows-x86_64': (b'MZ',),
}


class ArchiveError(Exception):
    pass


def platform_of(name):
    for platform, suffix in SUFFIX.items():
        if platform in name and name.endswith(suffix):
            return platform
    raise ArchiveError(f'cannot tell the platform of {name}')


def members(path):
    """`[(name, size, is_dir)]` of a zip or tar.gz archive."""
    path = Path(path)
    if path.suffix == '.zip':
        with zipfile.ZipFile(path) as z:
            return [(i.filename, i.file_size, i.is_dir()) for i in z.infolist()]
    with tarfile.open(path) as t:
        for m in t:
            if not (m.isfile() or m.isdir()):
                raise ArchiveError(f'unexpected archive entry type for {m.name}')
        return [(m.name, m.size, m.isdir()) for m in t.getmembers()]


def check_layout(path):
    """Raise unless every member is on the platform allowlist; return the platform."""
    platform = platform_of(Path(path).name)
    names = members(path)
    tops = {n.split('/', 1)[0] for n, _, _ in names}
    if len(tops) != 1:
        raise ArchiveError(f'expected one top-level folder, found {sorted(tops)}')
    top = tops.pop()
    allowed = [re.compile(p + '$') for p in LAYOUTS[platform]]
    files = set()
    for name, size, is_dir in names:
        if name.startswith('/') or '..' in name.split('/'):
            raise ArchiveError(f'unsafe member path {name}')
        if '/' not in name.rstrip('/'):
            continue  # the top-level folder itself
        relative = name.split('/', 1)[1].rstrip('/')
        if FORBIDDEN.search(relative):
            raise ArchiveError(f'forbidden member {name}')
        if is_dir:
            if not any(p.pattern.startswith(re.escape(relative + '/')) for p in allowed):
                raise ArchiveError(f'unexpected folder {name}')
            continue
        if not any(p.match(relative) for p in allowed):
            raise ArchiveError(f'member outside the allowlist: {name}')
        files.add(relative)
    required = {'README.txt'} | ({f'{APP}/Contents/Resources/content.pack', f'{APP}/Contents/MacOS/fury'}
                                  if platform == 'macos-universal' else
                                  {'fury.exe'} if platform == 'windows-x86_64' else {'fury'})
    if missing := required - files:
        raise ArchiveError(f'archive lacks {sorted(missing)}')
    return platform, top


def extract_member(path, relative, destination):
    """Extract `<top>/<relative>` to `destination` (a new file path)."""
    path = Path(path)
    top = members(path)[0][0].split('/', 1)[0]
    name = f'{top}/{relative}'
    with open(destination, 'wb') as out:
        if path.suffix == '.zip':
            with zipfile.ZipFile(path) as z, z.open(name) as f:
                shutil.copyfileobj(f, out, 8 << 20)
        else:
            with tarfile.open(path) as t:
                shutil.copyfileobj(t.extractfile(name), out, 8 << 20)


def check_contents(path, scratch=None):
    """Layout plus the packed content: authorized manifest, every chunk hashed, executable magic."""
    platform, _ = check_layout(path)
    executable = {'macos-universal': f'{APP}/Contents/MacOS/fury', 'windows-x86_64': 'fury.exe'}.get(platform, 'fury')
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        exe = Path(tmp) / 'exe'
        extract_member(path, executable, exe)
        with open(exe, 'rb') as f:
            if not f.read(4).startswith(MAGIC[platform]):
                raise ArchiveError(f'{executable} is not a {platform} executable')
        try:
            if platform == 'macos-universal':
                pack = Path(tmp) / 'pack'
                extract_member(path, f'{APP}/Contents/Resources/content.pack', pack)
                if pack_content.attached_region(exe) is not None:
                    raise ArchiveError('macOS executable must not also carry a pack')
                count, size = pack_content.verify(pack)
            else:
                region = pack_content.attached_region(exe)
                if region is None:
                    raise ArchiveError('executable carries no content pack')
                count, size = pack_content.verify(exe, *region)
        except pack_content.PackError as e:
            raise ArchiveError(f'content pack: {e}') from e
    return platform, count, size


def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        while chunk := f.read(8 << 20):
            h.update(chunk)
    return h.hexdigest()


def write_checksums(directory, names):
    lines = [f'{sha256(Path(directory) / n)}  {n}\n' for n in sorted(names)]
    (Path(directory) / 'SHA256SUMS.txt').write_text(''.join(lines))
    return lines
