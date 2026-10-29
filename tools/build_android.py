#!/usr/bin/env python3
"""Build the signed Android APK (arm64) of the Rust engine for the current commit.

    python3 tools/build_android.py                  # build, sign, check
    python3 tools/build_android.py --install        # also adb install -r (emulator or phone)
    python3 tools/build_android.py --install --launch
    python3 tools/build_android.py --allow-dirty    # local test build of an uncommitted tree
    make android                                    # ANDROID_ARGS='--install --launch'

The APK carries no game content: on first launch the app downloads the hosted content
package the browser autoloads (research/ANDROID.md). It is still built and signed on the
owner's machine only (the release key never enters CI). Outputs go to
~/LargeFiles/fury-android/<short-commit>/ (SSD backed):

    fury-remastered-<version>-<short>-android-arm64.apk
    SHA256SUMS.txt
    release.json
"""
import argparse
import base64
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
OUT_ROOT = Path.home() / 'LargeFiles/fury-android'
KEY_DIR = OUT_ROOT / 'keys'
KEYSTORE = KEY_DIR / 'fury-release.jks'
KEY_PASSWORD = KEY_DIR / 'fury-release.pass'
KEY_ALIAS = 'fury'
CARGO = Path.home() / '.cargo/bin/cargo'
PACKAGE = 'com.furyremastered.android'
LABEL = 'Fury Remastered'
TRIPLE = 'aarch64-linux-android'
ABI = 'arm64-v8a'
LIBRARY = 'libatv_app.so'
# AAudio (cpal) and Vulkan 1.1 / OpenGL ES 3 (wgpu) are all present from Android 8.0.
MIN_SDK = 26
TARGET_SDK = 35
# Launcher icon densities: (folder, pixels). 192 px is the largest authored PWA icon.
DENSITIES = (('mipmap-mdpi', 48), ('mipmap-hdpi', 72), ('mipmap-xhdpi', 96),
             ('mipmap-xxhdpi', 144), ('mipmap-xxxhdpi', 192))
# The engine library is about 30 MB; a content pack (450+ MB) can never hide in it.
MAX_LIBRARY = 120 * 1024 * 1024
# Every member a built APK may contain; anything else fails the build.
ALLOWED_MEMBER = re.compile(
    r'^(AndroidManifest\.xml|resources\.arsc|lib/arm64-v8a/libatv_app\.so'
    r'|res/mipmap-[a-z]+(-v\d+)?/ic_launcher\.png'
    r'|META-INF/[A-Z0-9_.-]+\.(SF|RSA|EC|DSA|MF))$')
FORBIDDEN_MEMBER = re.compile(r'(?i)(\.(pack|res|vag|lsi|bin|m4a|iso|elf)$|scus_|private/|manifest\.json$)')


def run(cmd, **kwargs):
    print('+', ' '.join(str(c) for c in cmd), flush=True)
    return subprocess.run([str(c) for c in cmd], check=True, **kwargs)


def git(*args):
    return subprocess.run(['git', *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()


def refuse_ci(environ=os.environ):
    """Signed release output is built on the owner's machine only."""
    if environ.get('GITHUB_ACTIONS') or environ.get('CI'):
        raise SystemExit('build_android: refusing to build or sign the APK in CI '
                         '(the release key stays on the owner\'s machine; CI only runs cargo check)')


def workspace_version(cargo_toml):
    """The `version = "..."` of `[workspace.package]`."""
    section = None
    for line in cargo_toml.splitlines():
        line = line.strip()
        if line.startswith('['):
            section = line
        elif section == '[workspace.package]' and line.startswith('version'):
            return line.split('"')[1]
    raise SystemExit('build_android: no [workspace.package] version in Cargo.toml')


def apk_name(version, short):
    return f'fury-remastered-{version}-{short}-android-arm64.apk'


def android_manifest(version, version_code):
    """The NativeActivity manifest: no Java code, the engine library is the app."""
    config_changes = ('orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden'
                      '|navigation|uiMode|density|fontScale|layoutDirection|locale')
    return f'''<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{PACKAGE}"
    android:versionCode="{int(version_code)}"
    android:versionName="{version}">
    <uses-feature android:glEsVersion="0x00030000" android:required="true" />
    <uses-feature android:name="android.hardware.touchscreen" android:required="false" />
    <uses-feature android:name="android.hardware.gamepad" android:required="false" />
    <!-- Downloads the hosted game content on first launch; multiplayer rooms. -->
    <uses-permission android:name="android.permission.INTERNET" />
    <application
        android:label="{LABEL}"
        android:icon="@mipmap/ic_launcher"
        android:hasCode="false"
        android:hardwareAccelerated="true"
        android:extractNativeLibs="false"
        android:allowBackup="false"
        android:appCategory="game"
        android:isGame="true">
        <activity
            android:name="android.app.NativeActivity"
            android:label="{LABEL}"
            android:exported="true"
            android:launchMode="singleTask"
            android:theme="@style/Fury"
            android:screenOrientation="sensorLandscape"
            android:configChanges="{config_changes}">
            <meta-data android:name="android.app.lib_name" android:value="atv_app" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
'''


STYLES = '''<?xml version="1.0" encoding="utf-8"?>
<resources>
    <style name="Fury" parent="@android:style/Theme.DeviceDefault.NoActionBar.Fullscreen">
        <item name="android:windowBackground">@android:color/black</item>
    </style>
</resources>
'''
# Android 9+: draw into the display cutout in landscape too.
STYLES_V28 = '''<?xml version="1.0" encoding="utf-8"?>
<resources>
    <style name="Fury" parent="@android:style/Theme.DeviceDefault.NoActionBar.Fullscreen">
        <item name="android:windowBackground">@android:color/black</item>
        <item name="android:windowLayoutInDisplayCutoutMode">shortEdges</item>
    </style>
</resources>
'''


def check_members(names):
    """Raises unless every APK member is on the allowlist (no game content, ever)."""
    names = list(names)
    for name in names:
        if FORBIDDEN_MEMBER.search(name) and name != 'AndroidManifest.xml':
            raise SystemExit(f'build_android: forbidden member in APK: {name}')
        if not ALLOWED_MEMBER.match(name):
            raise SystemExit(f'build_android: unexpected member in APK: {name}')
    for required in ('AndroidManifest.xml', f'lib/{ABI}/{LIBRARY}', 'resources.arsc'):
        if required not in names:
            raise SystemExit(f'build_android: APK lacks {required}')


def check_apk(path):
    with zipfile.ZipFile(path) as apk:
        infos = apk.infolist()
        check_members(info.filename for info in infos)
        library = next(i for i in infos if i.filename == f'lib/{ABI}/{LIBRARY}')
        if library.file_size > MAX_LIBRARY:
            raise SystemExit(f'build_android: {LIBRARY} is {library.file_size} bytes, over {MAX_LIBRARY}')
        if library.compress_type != zipfile.ZIP_STORED:
            raise SystemExit(f'build_android: {LIBRARY} must be stored uncompressed (extractNativeLibs=false)')
        with apk.open(library) as so:
            if so.read(4) != b'\x7fELF':
                raise SystemExit(f'build_android: {LIBRARY} is not an ELF library')
    return len(infos)


def sdk_root(environ=os.environ):
    for key in ('ANDROID_HOME', 'ANDROID_SDK_ROOT'):
        if environ.get(key) and Path(environ[key]).is_dir():
            return Path(environ[key])
    default = Path.home() / 'LargeFiles/android-sdk'
    if default.is_dir():
        return default
    raise SystemExit('build_android: no Android SDK; set ANDROID_HOME (research/ANDROID.md)')


def newest(folder, pattern='*'):
    def key(path):
        return [int(p) if p.isdigit() else p for p in re.split(r'[.\-]', path.name)]
    found = sorted((p for p in folder.glob(pattern) if p.is_dir()), key=key)
    if not found:
        raise SystemExit(f'build_android: nothing matching {pattern} in {folder}')
    return found[-1]


class Toolchain:
    def __init__(self, environ=os.environ):
        self.sdk = sdk_root(environ)
        self.build_tools = newest(self.sdk / 'build-tools')
        self.platform = self.sdk / f'platforms/android-{TARGET_SDK}'
        if not (self.platform / 'android.jar').is_file():
            self.platform = newest(self.sdk / 'platforms', 'android-*')
        ndk = environ.get('ANDROID_NDK_HOME') or environ.get('ANDROID_NDK_ROOT')
        self.ndk = Path(ndk) if ndk else newest(self.sdk / 'ndk')
        prebuilt = self.ndk / 'toolchains/llvm/prebuilt'
        hosts = sorted(p for p in prebuilt.glob('*') if p.is_dir())
        if not hosts:
            raise SystemExit(f'build_android: no NDK LLVM toolchain under {prebuilt}')
        self.llvm = hosts[0] / 'bin'
        self.clang = self.llvm / f'{TRIPLE}{MIN_SDK}-clang'
        for tool in (self.tool('aapt2'), self.tool('zipalign'), self.tool('apksigner'), self.clang,
                     self.platform / 'android.jar'):
            if not tool.exists():
                raise SystemExit(f'build_android: missing {tool}')

    def tool(self, name):
        return self.build_tools / name

    def cargo_env(self, environ=os.environ):
        env = dict(environ)
        env['PATH'] = f"{Path.home() / '.cargo/bin'}:{env.get('PATH', '')}"
        env.setdefault('CARGO_TARGET_DIR', str(OUT_ROOT / 'target'))
        upper = TRIPLE.upper().replace('-', '_')
        env[f'CARGO_TARGET_{upper}_LINKER'] = str(self.clang)
        env[f'CC_{TRIPLE.replace("-", "_")}'] = str(self.clang)
        env[f'AR_{TRIPLE.replace("-", "_")}'] = str(self.llvm / 'llvm-ar')
        return env


def ensure_keystore(keystore=KEYSTORE, password_file=KEY_PASSWORD):
    """The stable release key (updates install over older builds only with the same key).
    Made once, outside the repository; back up both files."""
    if keystore.exists():
        if not password_file.exists():
            raise SystemExit(f'build_android: {keystore} exists but {password_file} is missing')
        return keystore, password_file
    keystore.parent.mkdir(parents=True, exist_ok=True)
    os.chmod(keystore.parent, 0o700)
    descriptor = os.open(password_file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'w') as handle:
        handle.write(secrets.token_urlsafe(32) + '\n')
    run(['keytool', '-genkeypair', '-keystore', keystore, '-storetype', 'PKCS12', '-alias', KEY_ALIAS,
         '-keyalg', 'RSA', '-keysize', '4096', '-validity', '36500',
         '-dname', 'CN=Fury Remastered, O=Fury Remastered',
         '-storepass:file', password_file, '-keypass:file', password_file])
    os.chmod(keystore, 0o600)
    print(f'created release keystore {keystore} (password in {password_file}); back both up', flush=True)
    return keystore, password_file


def write_icons(res, work):
    icons = json.loads((ROOT / 'web/fury-app-icons.json').read_text())['icons']
    source = work / 'icon-192.png'
    source.write_bytes(base64.b64decode(next(i for i in icons if i['size'] == 192)['base64']))
    sips = shutil.which('sips')
    for folder, pixels in DENSITIES:
        target = res / folder / 'ic_launcher.png'
        target.parent.mkdir(parents=True, exist_ok=True)
        if pixels == 192:
            shutil.copyfile(source, target)
        elif sips:
            run([sips, '-z', pixels, pixels, source, '--out', target], stdout=subprocess.DEVNULL)
        else:
            # Without a resizer Android scales the xxxhdpi icon for other densities.
            target.parent.rmdir()


def build_library(tools, args):
    env = tools.cargo_env()
    profile = 'release'
    run([CARGO, 'build', '--release', '--locked', '-p', 'atv-app', '--lib', '--target', TRIPLE], cwd=ROOT, env=env)
    built = Path(env['CARGO_TARGET_DIR']) / TRIPLE / profile / LIBRARY
    if not built.is_file():
        raise SystemExit(f'build_android: cargo did not produce {built}')
    return built


def package(tools, ctx, library, keystore, password_file):
    work = ctx['work']
    shutil.rmtree(work, ignore_errors=True)
    res = work / 'res'
    (res / 'values').mkdir(parents=True)
    (res / 'values-v28').mkdir()
    (res / 'values/styles.xml').write_text(STYLES)
    (res / 'values-v28/styles.xml').write_text(STYLES_V28)
    write_icons(res, work)
    manifest = work / 'AndroidManifest.xml'
    manifest.write_text(android_manifest(ctx['version'], ctx['version_code']))
    compiled = work / 'compiled.zip'
    run([tools.tool('aapt2'), 'compile', '--dir', res, '-o', compiled])
    unsigned = work / 'unsigned.apk'
    run([tools.tool('aapt2'), 'link', '-o', unsigned, '--manifest', manifest, '-I', tools.platform / 'android.jar',
         '--min-sdk-version', MIN_SDK, '--target-sdk-version', TARGET_SDK, '--version-code', ctx['version_code'],
         '--version-name', ctx['version'], compiled])
    # The engine, stripped of its symbol table and stored uncompressed so it maps in place.
    stripped = work / LIBRARY
    run([tools.llvm / 'llvm-strip', '--strip-unneeded', '-o', stripped, library])
    with zipfile.ZipFile(unsigned, 'a') as apk:
        info = zipfile.ZipInfo(f'lib/{ABI}/{LIBRARY}', date_time=time.gmtime(ctx['timestamp'])[:6])
        info.compress_type = zipfile.ZIP_STORED
        info.external_attr = 0o644 << 16
        apk.writestr(info, stripped.read_bytes())
    aligned = work / 'aligned.apk'
    # 16 KB page alignment for the library (Android 15 devices with 16 KB pages).
    run([tools.tool('zipalign'), '-f', '-P', '16', '4', unsigned, aligned])
    final = ctx['out'] / apk_name(ctx['version'], ctx['short'])
    run([tools.tool('apksigner'), 'sign', '--ks', keystore, '--ks-key-alias', KEY_ALIAS,
         # PKCS12: the key's password is the store's (apksigner reads a file once per flag).
         '--ks-pass', f'file:{password_file}',
         '--min-sdk-version', MIN_SDK, '--out', final, aligned])
    run([tools.tool('apksigner'), 'verify', '--min-sdk-version', MIN_SDK, final])
    run([tools.tool('zipalign'), '-c', '-P', '16', '4', final], stdout=subprocess.DEVNULL)
    members = check_apk(final)
    return final, members


def sha256(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def release_record(ctx, apk, members, seconds):
    return {
        'commit': ctx['commit'], 'short': ctx['short'], 'version': ctx['version'],
        'version_code': int(ctx['version_code']), 'package': PACKAGE, 'abi': ABI,
        'min_sdk': MIN_SDK, 'target_sdk': TARGET_SDK, 'built': datetime.datetime.now(datetime.timezone.utc)
        .strftime('%Y-%m-%dT%H:%M:%SZ'), 'build_seconds': seconds,
        'content': 'none (downloaded from https://furyremastered.com/content/ on first launch)',
        'artifacts': [{'name': apk.name, 'bytes': apk.stat().st_size, 'sha256': sha256(apk), 'members': members}],
    }


def adb(tools):
    return tools.sdk / 'platform-tools/adb'


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--allow-dirty', action='store_true', help='build an uncommitted tree (marked -dirty)')
    parser.add_argument('--output', help='output folder (default ~/LargeFiles/fury-android/<short-commit>)')
    parser.add_argument('--install', action='store_true', help='adb install -r the APK afterwards')
    parser.add_argument('--launch', action='store_true', help='start the app after installing')
    args = parser.parse_args(argv)
    refuse_ci()
    started = time.time()
    commit = git('rev-parse', 'HEAD')
    short = git('rev-parse', '--short', 'HEAD')
    if git('status', '--porcelain'):
        if not args.allow_dirty:
            raise SystemExit('build_android: the working tree is dirty; commit first (or --allow-dirty for a local test build)')
        short += '-dirty'
    out = Path(args.output) if args.output else OUT_ROOT / short
    out.mkdir(parents=True, exist_ok=True)
    ctx = {
        'commit': commit, 'short': short, 'out': out, 'work': out / 'work',
        'version': workspace_version((ROOT / 'Cargo.toml').read_text()),
        # Monotonic across commits on main: the commit count.
        'version_code': git('rev-list', '--count', 'HEAD'),
        'timestamp': int(git('log', '-1', '--format=%ct')),
    }
    tools = Toolchain()
    keystore, password_file = ensure_keystore()
    library = build_library(tools, args)
    apk, members = package(tools, ctx, library, keystore, password_file)
    seconds = round(time.time() - started, 1)
    (out / 'SHA256SUMS.txt').write_text(f'{sha256(apk)}  {apk.name}\n')
    record = release_record(ctx, apk, members, seconds)
    (out / 'release.json').write_text(json.dumps(record, indent=2) + '\n')
    shutil.rmtree(ctx['work'], ignore_errors=True)
    print(f'{apk} ({apk.stat().st_size / 1e6:.1f} MB, versionCode {ctx["version_code"]})', flush=True)
    if args.install or args.launch:
        run([adb(tools), 'install', '-r', apk])
    if args.launch:
        run([adb(tools), 'shell', 'am', 'start', '-n', f'{PACKAGE}/android.app.NativeActivity'])
    return apk


if __name__ == '__main__':
    from host_build import ensure_guard
    ensure_guard(__file__, sys.argv[1:])
    main()
