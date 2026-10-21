#!/usr/bin/env python3
"""Build the standalone desktop apps for the current commit, with the game content packed in.

    python3 tools/build_desktop.py                      # all four platforms
    python3 tools/build_desktop.py --platforms macos-universal,windows-x86_64
    python3 tools/build_desktop.py --smoke              # also launch the Linux builds under Docker
    make desktop

These archives contain the extracted runtime resources, so they are built on the
owner's machine and never in CI. Outputs go to ~/LargeFiles/fury-desktop/<short-commit>/
(SSD backed). See research/DESKTOP_RELEASES.md.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import desktop_archive  # noqa: E402
import pack_content  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
OUT_ROOT = Path.home() / 'LargeFiles/fury-desktop'
CARGO = Path.home() / '.cargo/bin/cargo'
PLATFORMS = ('macos-universal', 'linux-x86_64', 'linux-aarch64', 'windows-x86_64')
LINUX_TRIPLES = {'linux-x86_64': 'x86_64-unknown-linux-gnu', 'linux-aarch64': 'aarch64-unknown-linux-gnu'}
WINDOWS_TRIPLE = 'x86_64-pc-windows-gnu'
BUNDLE_ID = 'com.furyremastered.desktop'
MACOS_MINIMUM = '11.0'
# Debian 11 keeps the Linux builds loadable on glibc 2.31 (Ubuntu 20.04+, Debian 11+).
BUILDER_IMAGE = 'fury-desktop-builder:bullseye-4'
# libudev-dev for both Linux architectures: gilrs (controllers) links libudev on Linux.
# Debian 11 is end of life, so its packages come from archive.debian.org.
BUILDER_DOCKERFILE = '''FROM rust:1-bullseye
RUN sed -i -e '/bullseye-updates/d' -e 's|deb.debian.org|archive.debian.org|' /etc/apt/sources.list && \\
    echo 'Acquire::Check-Valid-Until "false";' > /etc/apt/apt.conf.d/99archive
RUN set -e; native=$(dpkg --print-architecture); \\
    if [ "$native" = arm64 ]; then foreign=amd64; cross="gcc-x86-64-linux-gnu:$native libc6-dev-amd64-cross"; \\
    else foreign=arm64; cross="gcc-aarch64-linux-gnu:$native libc6-dev-arm64-cross"; fi; \\
    dpkg --add-architecture $foreign; apt-get update; \\
    apt-get install -y --no-install-recommends pkg-config $cross gcc-mingw-w64-x86-64 \\
      libudev-dev:$native libudev-dev:$foreign; \\
    rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-gnu
'''
# Debian 12 userland with a software Vulkan driver (lavapipe) and a virtual X server.
SMOKE_IMAGE = 'fury-desktop-smoke:bookworm-2'
SMOKE_DOCKERFILE = '''FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \\
      xvfb xauth mesa-vulkan-drivers libvulkan1 libgl1 libegl1 libx11-6 libx11-xcb1 libxcb1 \\
      libxkbcommon0 libxkbcommon-x11-0 libxcursor1 libxi6 libxrandr2 libwayland-client0 libudev1 ca-certificates \\
    && rm -rf /var/lib/apt/lists/*
'''


def run(cmd, **kwargs):
    print('+', ' '.join(str(c) for c in cmd), flush=True)
    return subprocess.run([str(c) for c in cmd], check=True, **kwargs)


def git(*args):
    return subprocess.run(['git', *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()


class Context:
    def __init__(self, args):
        self.commit = git('rev-parse', 'HEAD')
        self.short = git('rev-parse', '--short', 'HEAD')
        self.dirty = bool(git('status', '--porcelain'))
        if self.dirty and not args.allow_dirty:
            sys.exit('build_desktop: the working tree is dirty; commit first (or --allow-dirty for a local test build)')
        if self.dirty:
            self.short += '-dirty'
        self.timestamp = int(git('log', '-1', '--format=%ct'))
        self.build_number = git('rev-list', '--count', 'HEAD')
        manifest = (ROOT / 'Cargo.toml').read_text()
        self.version = next(l.split('"')[1] for l in manifest.splitlines() if l.startswith('version = '))
        self.out = Path(args.output or OUT_ROOT / self.short)
        self.work = self.out / 'work'
        self.work.mkdir(parents=True, exist_ok=True)
        self.args = args
        self.timings = {}

    def archive_name(self, platform):
        return f'fury-remastered-{self.version}-{self.short}-{platform}{desktop_archive.SUFFIX[platform]}'

    def folder(self, platform):
        if platform == 'macos-universal':
            return 'Fury Remastered'
        return f'fury-remastered-{self.version}-{self.short}-{platform}'

    def readme(self, platform):
        run_text = {
            'macos-universal': (
                'Unzip, then open Fury Remastered.app (drag it to Applications first if you like).\n'
                'The app is signed ad hoc, not notarized by Apple, so the first launch is blocked:\n'
                'Control-click the app, choose Open, then Open again. If macOS reports the app as\n'
                'damaged, run this once in Terminal from the unzipped folder:\n'
                '  xattr -dr com.apple.quarantine "Fury Remastered.app"\n'
                'Runs natively on Apple silicon and Intel Macs (macOS 11 or later, Metal).'),
            'linux-x86_64': 'Extract the archive and run ./fury from a graphical session (X11 or Wayland).\n'
                            'Needs a Vulkan driver (Mesa or the vendor driver), libudev (libudev.so.1) and glibc 2.31 or newer.',
            'linux-aarch64': 'Extract the archive and run ./fury from a graphical session (X11 or Wayland).\n'
                             'Needs a Vulkan driver (Mesa or the vendor driver), libudev (libudev.so.1) and glibc 2.31 or newer.',
            'windows-x86_64': 'Unzip and run fury.exe. Windows SmartScreen may warn about an unrecognized\n'
                              'app (the build is not code signed): choose More info, then Run anyway.\n'
                              'Needs a DirectX 12 or Vulkan capable graphics driver. A console window opens\n'
                              'beside the game and shows diagnostics.',
        }[platform]
        text = (ROOT / 'tools/desktop/README.txt.in').read_text()
        for key, value in {'VERSION': self.version, 'COMMIT': self.short, 'PLATFORM': platform,
                           'RUN': run_text, 'NOTES': ''}.items():
            text = text.replace(f'@{key}@', value)
        return text


def timed(ctx, name):
    class T:
        def __enter__(self):
            self.t = time.time()

        def __exit__(self, *exc):
            ctx.timings[name] = round(time.time() - self.t, 1)
    return T()


def ensure_pack(args):
    if args.pack:
        pack = Path(args.pack)
    else:
        pack = pack_content.build(pack_content.DEFAULT_SOURCE, None)
    count, size = pack_content.verify(pack)
    print(f'content pack {pack}: {count} blobs, {size / 1e6:.1f} MB', flush=True)
    return pack


# ----- macOS ---------------------------------------------------------------------------

def cargo_env():
    env = dict(os.environ)
    env['PATH'] = f"{Path.home() / '.cargo/bin'}:{env['PATH']}"
    env.setdefault('CARGO_TARGET_DIR', str(OUT_ROOT / 'target'))
    env['MACOSX_DEPLOYMENT_TARGET'] = MACOS_MINIMUM
    return env


def make_icns(destination, work):
    icons = json.loads((ROOT / 'web/fury-app-icons.json').read_text())['icons']
    import base64
    source = work / 'icon-512.png'
    source.write_bytes(base64.b64decode(next(i for i in icons if i['size'] == 512)['base64']))
    iconset = work / 'fury.iconset'
    shutil.rmtree(iconset, ignore_errors=True)
    iconset.mkdir()
    # (name, pixels); 512 is the largest authored size, so 512x512@2x is omitted.
    for name, pixels in (('16x16', 16), ('16x16@2x', 32), ('32x32', 32), ('32x32@2x', 64), ('128x128', 128),
                         ('128x128@2x', 256), ('256x256', 256), ('256x256@2x', 512), ('512x512', 512)):
        run(['sips', '-z', pixels, pixels, source, '--out', iconset / f'icon_{name}.png'], stdout=subprocess.DEVNULL)
    run(['iconutil', '-c', 'icns', iconset, '-o', destination])


def build_macos(ctx, pack):
    env = cargo_env()
    triples = ('aarch64-apple-darwin', 'x86_64-apple-darwin')
    for triple in triples:
        run([CARGO, 'build', '--release', '--locked', '-p', 'atv-app', '--bin', 'fury', '--target', triple],
            cwd=ROOT, env=env)
    target = Path(env['CARGO_TARGET_DIR'])
    stage = ctx.work / 'macos' / ctx.folder('macos-universal')
    shutil.rmtree(stage.parent, ignore_errors=True)
    app = stage / desktop_archive.APP
    (app / 'Contents/MacOS').mkdir(parents=True)
    (app / 'Contents/Resources').mkdir()
    run(['lipo', '-create', '-output', app / 'Contents/MacOS/fury',
         *[target / t / 'release/fury' for t in triples]])
    shutil.copyfile(pack, app / 'Contents/Resources/content.pack')
    make_icns(app / 'Contents/Resources/fury.icns', ctx.work)
    plist = {
        'CFBundleName': 'Fury Remastered', 'CFBundleDisplayName': 'Fury Remastered',
        'CFBundleIdentifier': BUNDLE_ID, 'CFBundleExecutable': 'fury', 'CFBundleIconFile': 'fury',
        'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': ctx.version,
        'CFBundleVersion': ctx.build_number, 'LSMinimumSystemVersion': MACOS_MINIMUM,
        'NSHighResolutionCapable': True, 'LSApplicationCategoryType': 'public.app-category.games',
        'NSPrincipalClass': 'NSApplication',
        'NSHumanReadableCopyright': 'Unofficial fan port. Commit ' + ctx.short,
    }
    (app / 'Contents/Info.plist').write_bytes(plistlib.dumps(plist))
    (stage / 'README.txt').write_text(ctx.readme('macos-universal'))
    # Ad hoc signature: the pack lives in Resources so the Mach-O stays signable.
    run(['codesign', '--force', '--sign', '-', '--timestamp=none', app])
    run(['codesign', '--verify', '--deep', '--strict', app])
    archive = ctx.out / ctx.archive_name('macos-universal')
    archive.unlink(missing_ok=True)
    run(['ditto', '-c', '-k', '--norsrc', '--noextattr', '--noqtn', '--keepParent', stage, archive])
    return archive


# ----- Docker (Linux, Windows) ----------------------------------------------------------

def docker_image(tag, dockerfile, platform=None):
    if subprocess.run(['docker', 'image', 'inspect', tag], capture_output=True).returncode == 0:
        return
    extra = ['--platform', platform] if platform else []
    with tempfile.TemporaryFile() as stdin:  # a real file: a pipe can leave buildx waiting
        stdin.write(dockerfile.encode())
        stdin.seek(0)
        run(['docker', 'build', *extra, '-t', tag, '-'], stdin=stdin)


def source_tar(ctx):
    """The commit's tracked files (never the private symlink, content or build output)."""
    tar = ctx.work / 'source.tar'
    if ctx.dirty:
        listing = git('ls-files', '-co', '--exclude-standard', '-z').split('\0')
        files = [f for f in listing if f and f != 'private' and os.path.lexists(ROOT / f)]
        env = dict(os.environ, COPYFILE_DISABLE='1')
        run(['tar', '-cf', tar, '-T', '-'], cwd=ROOT, env=env, input='\n'.join(files).encode())
    else:
        run(['git', 'archive', '--format=tar', '-o', tar, 'HEAD'], cwd=ROOT)
    return tar


CONTAINER_SCRIPT = r'''set -e
mkdir -p /src && tar -xf - -C /src && cd /src
export CARGO_TARGET_DIR=/target CARGO_TERM_COLOR=never
case "$TRIPLE" in
  x86_64-unknown-linux-gnu) [ "$(uname -m)" = x86_64 ] && L=gcc || L=x86_64-linux-gnu-gcc
    LIBDIR=/usr/lib/x86_64-linux-gnu ;;
  aarch64-unknown-linux-gnu) [ "$(uname -m)" = aarch64 ] && L=gcc || L=aarch64-linux-gnu-gcc
    LIBDIR=/usr/lib/aarch64-linux-gnu ;;
  x86_64-pc-windows-gnu) L=x86_64-w64-mingw32-gcc ;;
esac
U=$(echo "$TRIPLE" | tr 'a-z-' 'A-Z_')
export "CARGO_TARGET_${U}_LINKER=$L" "CC_$(echo "$TRIPLE" | tr - _)=$L"
if [ -n "$LIBDIR" ]; then  # the target's libudev (multiarch) for cross builds
  export PKG_CONFIG_ALLOW_CROSS=1 "PKG_CONFIG_PATH=$LIBDIR/pkgconfig" "CARGO_TARGET_${U}_RUSTFLAGS=-L $LIBDIR"
fi
cargo build --release --locked -p atv-app --bin fury --target "$TRIPLE" -j 2 >&2
cd "/target/$TRIPLE/release" && tar -cf - "$BINARY"
'''


def docker_build(ctx, triple, binary):
    docker_image(BUILDER_IMAGE, BUILDER_DOCKERFILE)
    source = source_tar(ctx)
    out = ctx.work / f'{triple}.tar'
    registry = Path.home() / '.cargo/registry'
    registry.mkdir(parents=True, exist_ok=True)
    with open(source, 'rb') as stdin, open(out, 'wb') as stdout:
        run(['docker', 'run', '--rm', '-i', '-e', f'TRIPLE={triple}', '-e', f'BINARY={binary}',
             '-v', f'fury-desktop-target-{triple}:/target', '-v', f'{registry}:/usr/local/cargo/registry',
             BUILDER_IMAGE, 'bash', '-c', CONTAINER_SCRIPT], stdin=stdin, stdout=stdout)
    folder = ctx.work / triple
    shutil.rmtree(folder, ignore_errors=True)
    folder.mkdir(parents=True)
    with tarfile.open(out) as t, open(folder / binary, 'wb') as f:
        shutil.copyfileobj(t.extractfile(binary), f, 8 << 20)
    out.unlink()
    return folder / binary


def attach_and_archive(ctx, platform, executable, pack):
    binary = 'fury.exe' if platform == 'windows-x86_64' else 'fury'
    folder = ctx.folder(platform)
    stage = ctx.work / platform / folder
    shutil.rmtree(stage.parent, ignore_errors=True)
    stage.mkdir(parents=True)
    pack_content.attach_pack(executable, pack, stage / binary)
    (stage / 'README.txt').write_text(ctx.readme(platform))
    archive = ctx.out / ctx.archive_name(platform)
    archive.unlink(missing_ok=True)
    stamp = ctx.timestamp
    if platform == 'windows-x86_64':
        date = datetime.datetime.fromtimestamp(stamp, datetime.timezone.utc).timetuple()[:6]
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as z:
            for name in (binary, 'README.txt'):
                info = zipfile.ZipInfo(f'{folder}/{name}', date_time=max(date, (1980, 1, 1, 0, 0, 0)))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o644 << 16
                with open(stage / name, 'rb') as f, z.open(info, 'w', force_zip64=True) as out:
                    shutil.copyfileobj(f, out, 8 << 20)
    else:
        def normalize(info):
            info.uid = info.gid = 0
            info.uname = info.gname = ''
            info.mtime = stamp
            info.mode = 0o755 if info.isdir() or info.name.endswith('/fury') else 0o644
            return info
        with tarfile.open(archive, 'w:gz', compresslevel=6) as t:
            t.add(stage, arcname=folder, filter=normalize)
    return archive


# ----- smoke tests ---------------------------------------------------------------------

def smoke_linux(out_dir, archive, platform):
    """Launch the Linux build under Xvfb with software Vulkan and keep the captured frame."""
    docker_platform = 'linux/amd64' if platform == 'linux-x86_64' else 'linux/arm64'
    image = f'{SMOKE_IMAGE}-{docker_platform.split("/")[1]}'
    docker_image(image, SMOKE_DOCKERFILE, docker_platform)
    emulated = platform == 'linux-x86_64' and os.uname().machine == 'arm64'
    png = Path(out_dir) / f'smoke-{platform}.png'
    script = ('set -e; mkdir /app && tar -xzf - -C /app && cd /app/* && ./fury --verify-content >&2 && start=$(date +%s%N) && '
              'xvfb-run -a -s "-screen 0 1280x720x24" ./fury --capture /tmp/out.png 1280x720 >&2 && '
              'end=$(date +%s%N) && echo "capture milliseconds: $(( (end - start) / 1000000 ))" >&2; '
              'cat /tmp/out.png')
    command = ['docker', 'run', '--rm', '-i', '--platform', docker_platform,
               '-e', 'VK_ICD_FILENAMES=' + ('/usr/share/vulkan/icd.d/lvp_icd.x86_64.json' if platform == 'linux-x86_64'
                                            else '/usr/share/vulkan/icd.d/lvp_icd.aarch64.json'),
               image, 'bash', '-c', script]
    started = time.time()
    with open(archive, 'rb') as stdin, open(png, 'wb') as stdout:
        result = subprocess.run(command, stdin=stdin, stdout=stdout, timeout=1800)
    seconds = round(time.time() - started, 1)
    ok = result.returncode == 0 and png.stat().st_size > 1000
    print(f'smoke {platform}: {"ok" if ok else "FAILED"} in {seconds}s{" (emulated)" if emulated else ""}; {png}')
    return {'ok': ok, 'seconds': seconds, 'emulated': emulated, 'image': str(png)}


# ----- main ----------------------------------------------------------------------------

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--platforms', default=','.join(PLATFORMS), help='comma separated subset of: ' + ', '.join(PLATFORMS))
    parser.add_argument('--pack', help='content pack to use (default: build one from private/reports/hosted-content)')
    parser.add_argument('--output', help=f'output folder (default {OUT_ROOT}/<short-commit>)')
    parser.add_argument('--allow-dirty', action='store_true', help='build a local test version of an uncommitted tree')
    parser.add_argument('--smoke', action='store_true', help='launch the Linux builds under Docker (Xvfb + software Vulkan)')
    parser.add_argument('--keep-work', action='store_true')
    parser.add_argument('--smoke-only', type=Path, metavar='ARCHIVE',
                        help='launch an existing Linux archive under Docker and exit')
    args = parser.parse_args(argv)
    if args.smoke_only:
        platform = desktop_archive.platform_of(args.smoke_only.name)
        if platform not in LINUX_TRIPLES:
            parser.error('--smoke-only takes a Linux archive')
        return 0 if smoke_linux(args.smoke_only.parent, args.smoke_only, platform)['ok'] else 1
    platforms = [p for p in args.platforms.split(',') if p]
    unknown = set(platforms) - set(PLATFORMS)
    if unknown:
        parser.error(f'unknown platforms {sorted(unknown)}')
    ctx = Context(args)
    pack = ensure_pack(args)
    results = {}
    for platform in platforms:
        print(f'\n=== {platform} ===', flush=True)
        with timed(ctx, platform):
            if platform == 'macos-universal':
                archive = build_macos(ctx, pack)
            elif platform == 'windows-x86_64':
                archive = attach_and_archive(ctx, platform, docker_build(ctx, WINDOWS_TRIPLE, 'fury.exe'), pack)
            else:
                archive = attach_and_archive(ctx, platform, docker_build(ctx, LINUX_TRIPLES[platform], 'fury'), pack)
        desktop_archive.check_contents(archive, scratch=ctx.work)
        results[platform] = {'file': archive.name, 'bytes': archive.stat().st_size, 'seconds': ctx.timings[platform]}
        print(f'{archive} ({archive.stat().st_size / 1e6:.1f} MB)', flush=True)
        if args.smoke and platform in LINUX_TRIPLES:
            results[platform]['smoke'] = smoke_linux(ctx.out, archive, platform)
    for record in results.values():
        record['sha256'] = desktop_archive.sha256(ctx.out / record['file'])
    # Rebuilding a subset of platforms for the same commit keeps the other archives.
    previous = {}
    try:
        old = json.loads((ctx.out / 'release.json').read_text())
        if old.get('commit') == ctx.commit and old.get('content_pack') == pack.name:
            previous = old['archives']
    except (OSError, ValueError, KeyError):
        pass
    archives = {**previous, **results}
    desktop_archive.write_checksums(ctx.out, [r['file'] for r in archives.values()])
    (ctx.out / 'release.json').write_text(json.dumps({
        'commit': ctx.commit, 'short': ctx.short, 'version': ctx.version, 'dirty': ctx.dirty,
        'content_pack': pack.name, 'archives': archives}, indent=2) + '\n')
    if not args.keep_work:
        shutil.rmtree(ctx.work, ignore_errors=True)
    print(f'\nDone: {ctx.out}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
