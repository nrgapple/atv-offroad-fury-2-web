#!/usr/bin/env python3
"""Install the pinned Playwright browsers and their Ubuntu packages for CI smoke tests.

`playwright install --with-deps` downloads about 119 MB of Ubuntu packages from the
runner's apt mirror; on slow days that download crawled until the step's timeout.
This helper bounds every network attempt and retries once (on another apt mirror),
installs packages offline from the downloaded archives so a timeout never interrupts
dpkg, and reuses trusted main-branch caches only after validating them: restored
archives must match the hashes in the runner's signed apt indexes, the npm package
must report the pinned version, and both browsers must launch. Anything else falls
back to a clean install. See research/CI_TOOL_CACHE.md.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
from urllib.parse import unquote

VERSION = '1.58.2'
# Chromium installs only its headless shell (--only-shell); WebKit is the full build.
BROWSERS = ('chromium', 'webkit')
BROWSER_LABEL = 'chromium-headless-shell-webkit'
SCHEMA = 1
IDENTITY = re.compile(r'[A-Za-z0-9_.-]+')
PACKAGE = re.compile(r'[a-z0-9][a-z0-9+.-]*')
# `'url' local-name size [hash]`; the hash is MD5 or absent (security pockets).
URI = re.compile(r"^'([^']+)' (\S+\.deb) (\d+)(?: \S+)?$")
VERSION_TEXT = re.compile(r'[A-Za-z0-9.+~:-]+')
MIRRORS = Path('/etc/apt/apt-mirrors.txt')
SLOW_MIRROR = 'azure.archive.ubuntu.com'
# Seconds per attempt: (first, retry). A healthy runner needs about 17 s for the
# package download and 8 s for the browsers.
LIMITS = {'npm': (60, 90), 'update': (45, 60), 'download': (120, 180),
          'browsers': (90, 150), 'launch': (60, 60)}


def identity(env=os.environ):
    values = {'os': env.get('RUNNER_OS', ''), 'arch': env.get('RUNNER_ARCH', ''),
              'image': env.get('ImageOS', ''), 'image_version': env.get('ImageVersion', '')}
    for name, value in values.items():
        if not IDENTITY.fullmatch(value):
            raise ValueError(f'runner identity {name} is unavailable or invalid')
    return values


def cache_keys(stamp):
    base = f'{stamp["os"]}-{stamp["arch"]}-{stamp["image"]}'
    return {
        # Browser builds are per distribution (webkit-ubuntu-24.04), not per image.
        'browsers-key': f'fury-playwright-v{SCHEMA}-{base}-{VERSION}-{BROWSER_LABEL}',
        # Packages are what this image still lacks; apt revalidates every file.
        'apt-key': f'fury-playwright-apt-v{SCHEMA}-{base}-{stamp["image_version"]}-{VERSION}-{BROWSER_LABEL}',
    }


def parse_dry_run(text):
    """Package names from `playwright install-deps --dry-run` output."""
    match = re.search(r'apt-get install -y --no-install-recommends ([^"\n]+)', text)
    packages = match.group(1).split() if match else []
    if not packages or not all(PACKAGE.fullmatch(name) for name in packages):
        raise ValueError('could not read the pinned Playwright dependency list')
    return packages


def parse_print_uris(text):
    """Archives apt would fetch: {local name: (url basename, size, `name=version` spec)}.

    `--print-uris` reports only MD5 sums, so SHA-256 comes from the indexes (below).
    """
    archives = {}
    for line in text.splitlines():
        match = URI.match(line.strip())
        if not match:
            if line.strip().startswith("'"):
                raise ValueError(f'unexpected apt archive line: {line.strip()[:120]}')
            continue
        url, name, size = match.groups()
        parts = name[:-len('.deb')].split('_')
        if len(parts) != 3 or not PACKAGE.fullmatch(parts[0]) or not VERSION_TEXT.fullmatch(unquote(parts[1])):
            raise ValueError(f'unexpected archive name {name!r}')
        archives[name] = (unquote(url.rsplit('/', 1)[-1]), int(size), f'{parts[0]}={unquote(parts[1])}')
    return archives


def index_hashes(archives, show_text):
    """{local name: (size, 'SHA256', digest)} from `apt-cache show name=version` records."""
    by_file = {}
    for record in show_text.split('\n\n'):
        fields = dict(line.split(': ', 1) for line in record.splitlines() if ': ' in line and not line.startswith(' '))
        if 'Filename' in fields and re.fullmatch(r'[0-9a-f]{64}', fields.get('SHA256', '')):
            by_file[fields['Filename'].rsplit('/', 1)[-1]] = (int(fields.get('Size', -1)), fields['SHA256'])
    expected = {}
    for name, (basename, size, _) in archives.items():
        if by_file.get(basename, (None,))[0] != size:
            raise ValueError(f'no signed index SHA-256 for {name}')
        expected[name] = (size, 'SHA256', by_file[basename][1])
    return expected


def file_digest(path, algorithm):
    digest = hashlib.new(algorithm.lower())
    with open(path, 'rb') as stream:
        for block in iter(lambda: stream.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def matches(path, entry):
    size, algorithm, digest = entry
    return (not path.is_symlink() and path.is_file() and path.stat().st_size == size
            and file_digest(path, algorithm) == digest)


def prune_archives(directory, expected):
    """Keep only restored archives whose size and hash the signed indexes expect."""
    kept, removed = 0, 0
    if directory.is_symlink():
        raise ValueError('package archive directory must not be a symlink')
    directory.mkdir(parents=True, exist_ok=True)
    for path in directory.iterdir():
        if path.name in {'partial', 'lock'} and not path.is_symlink():
            continue
        if path.name in expected and matches(path, expected[path.name]):
            kept += 1
            continue
        if path.is_dir() and not path.is_symlink():
            shutil.rmtree(path)
        else:
            path.unlink()
        removed += 1
    return kept, removed


def fallback_mirrors(text):
    """The runner's mirror list without the slow default mirror, or None to keep it."""
    lines = text.splitlines(keepends=True)
    kept = [line for line in lines if SLOW_MIRROR not in line]
    if len(kept) == len(lines) or not any(line.strip() and not line.lstrip().startswith('#') for line in kept):
        return None
    return ''.join(kept)


def bounded(seconds, command, root=False):
    """Run a command with a hard timeout delivered by `timeout` itself (as root for apt)."""
    prefix = ['sudo', 'env', 'DEBIAN_FRONTEND=noninteractive'] if root else []
    return prefix + ['timeout', '-k', '10', str(seconds)] + list(command)


class Installer:
    def __init__(self, root, browsers_hit, apt_hit, run=subprocess.run, log=print):
        self.root = root
        self.package = root / 'package'
        self.archives = root / 'apt'
        self.browsers = Path(os.environ.get('PLAYWRIGHT_BROWSERS_PATH') or Path.home() / '.cache/ms-playwright')
        self.browsers_hit, self.apt_hit = browsers_hit, apt_hit
        self.run, self.log = run, log
        self.report = {'version': VERSION, 'phases': {}}
        self.mirror_switched = False

    @property
    def cli(self):
        return self.package / 'node_modules/.bin/playwright'

    @property
    def module(self):
        return self.package / 'node_modules/playwright'

    def attempts(self, phase, command, root=False, before_retry=None):
        started = time.monotonic()
        for attempt, seconds in enumerate(LIMITS[phase], 1):
            if attempt > 1 and before_retry:
                before_retry()
            result = self.run(bounded(seconds, command, root), check=False)
            if result.returncode == 0:
                self.report['phases'][phase] = {'attempts': attempt, 'seconds': round(time.monotonic() - started, 1)}
                return
            reason = 'timed out' if result.returncode in (124, 137) else f'exit {result.returncode}'
            self.log(f'::warning::Playwright {phase} attempt {attempt} {reason} after {seconds}s limit')
        raise RuntimeError(f'Playwright {phase} failed after {len(LIMITS[phase])} bounded attempts')

    def output(self, *command):
        return subprocess.run(command, check=True, text=True, capture_output=True, timeout=120).stdout

    def package_valid(self):
        try:
            for name in ('playwright', 'playwright-core'):
                manifest = self.package / 'node_modules' / name / 'package.json'
                if manifest.is_symlink() or json.loads(manifest.read_text()).get('version') != VERSION:
                    return False
            return self.cli.exists()
        except (OSError, ValueError):
            return False

    def ensure_package(self):
        if self.browsers_hit and self.package_valid():
            self.report['package'] = 'cache'
            return
        shutil.rmtree(self.package, ignore_errors=True)
        self.attempts('npm', ['npm', 'install', '--no-save', '--no-audit', '--no-fund',
                              '--prefix', str(self.package), f'playwright@{VERSION}'],
                      before_retry=lambda: shutil.rmtree(self.package, ignore_errors=True))
        if not self.package_valid():
            raise RuntimeError(f'npm did not install playwright {VERSION}')
        self.report['package'] = 'cold'

    def switch_mirror(self):
        if self.mirror_switched or not MIRRORS.is_file():
            return
        replacement = fallback_mirrors(MIRRORS.read_text())
        if replacement is None:
            return
        staged = self.root / 'apt-mirrors.txt'
        staged.write_text(replacement)
        self.run(['sudo', 'cp', str(staged), str(MIRRORS)], check=True)
        self.mirror_switched = True
        self.log(f'Retrying apt without {SLOW_MIRROR}')

    def apt(self, *args):
        return ['apt-get', '-q', '-o', f'Dir::Cache::Archives={self.archives}', *args]

    def install_packages(self):
        packages = parse_dry_run(self.output(str(self.cli), 'install-deps', '--dry-run', *BROWSERS))
        self.attempts('update', ['apt-get', '-qq', 'update'], root=True, before_retry=self.switch_mirror)
        # List what this image needs against an empty archive directory, so every
        # file appears with the hash from the signed indexes.
        empty = self.root / 'apt-empty'
        shutil.rmtree(empty, ignore_errors=True)
        (empty / 'partial').mkdir(parents=True)
        archives = parse_print_uris(self.output(
            'sudo', 'apt-get', '-q', '-o', f'Dir::Cache::Archives={empty}', 'install', '-y',
            '--no-install-recommends', '--print-uris', *packages))
        specs = sorted({spec for _, _, spec in archives.values()})
        expected = index_hashes(archives, self.output('apt-cache', 'show', *specs) if specs else '')
        kept, removed = prune_archives(self.archives, expected) if self.apt_hit else (0, 0)
        if not self.apt_hit:
            shutil.rmtree(self.archives, ignore_errors=True)
            self.archives.mkdir(parents=True)
        (self.archives / 'partial').mkdir(exist_ok=True)
        if len(expected) > kept:
            self.attempts('download', self.apt('install', '-y', '--no-install-recommends',
                                               '--download-only', *packages),
                          root=True, before_retry=self.switch_mirror)
        missing = [name for name, entry in expected.items() if not matches(self.archives / name, entry)]
        if missing:
            raise RuntimeError(f'{len(missing)} package archives failed hash validation, e.g. {missing[0]}')
        started = time.monotonic()
        self.run(['sudo', 'env', 'DEBIAN_FRONTEND=noninteractive', *self.apt(
            'install', '-y', '--no-install-recommends', '--no-download', *packages)], check=True)
        self.report['phases']['install'] = {'seconds': round(time.monotonic() - started, 1)}
        # The cache action runs as the runner user; archives stay world-readable.
        self.run(['sudo', 'chown', '-R', f'{os.getuid()}:{os.getgid()}', str(self.archives)], check=True)
        self.report['apt'] = {'packages': len(packages), 'archives': len(expected),
                              'from_cache': kept, 'discarded': removed,
                              'downloaded': len(expected) - kept}

    def browser_dirs(self):
        if not self.browsers.is_dir():
            return []
        return sorted(path.name for path in self.browsers.iterdir() if not path.name.startswith('.'))

    def install_browsers(self):
        before = self.browser_dirs()
        clear = lambda: shutil.rmtree(self.browsers, ignore_errors=True)
        if not self.browsers_hit:
            clear()
        self.attempts('browsers', [str(self.cli), 'install', '--only-shell', *BROWSERS], before_retry=clear)
        return before

    def launch(self):
        script = ("const pw=require(process.argv[1]);(async()=>{for(const n of ['chromium','webkit']){"
                  "const b=await pw[n].launch();const p=await b.newPage();"
                  "await p.setContent('<p>ok</p>');if(await p.textContent('p')!=='ok')throw new Error(n);"
                  "console.log(n+' '+b.version());await b.close();}})().catch(e=>{console.error(e);process.exit(1);});")
        result = self.run(bounded(LIMITS['launch'][0], ['node', '-e', script, str(self.module)]), check=False)
        return result.returncode == 0

    def install(self):
        self.root.mkdir(parents=True, exist_ok=True)
        self.ensure_package()
        self.install_packages()
        before = self.install_browsers()
        if not self.launch():
            self.log('::warning::Restored or downloaded browsers did not launch; reinstalling clean')
            shutil.rmtree(self.browsers, ignore_errors=True)
            self.browsers_hit = False
            self.install_browsers()
            before = []
            if not self.launch():
                raise RuntimeError('Playwright browsers do not launch after a clean install')
        after = self.browser_dirs()
        self.report['browsers'] = {'source': 'cache' if self.browsers_hit and before == after and after else 'cold',
                                   'installed': after}
        return self.report


def write_lines(variable, lines):
    path = os.environ.get(variable)
    if path:
        with open(path, 'a') as stream:
            stream.write(''.join(line + '\n' for line in lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['key', 'install'])
    parser.add_argument('--root', type=Path)
    parser.add_argument('--browsers-hit', default='', choices=['', 'true', 'false'])
    parser.add_argument('--apt-hit', default='', choices=['', 'true', 'false'])
    args = parser.parse_args()
    if args.mode == 'key':
        keys = cache_keys(identity())
        write_lines('GITHUB_OUTPUT', [f'{name}={value}' for name, value in keys.items()])
        print(json.dumps(keys))
        return
    if args.root is None:
        parser.error('install requires --root')
    started = time.monotonic()
    installer = Installer(args.root.absolute(), args.browsers_hit == 'true', args.apt_hit == 'true')
    report = installer.install()
    report['seconds'] = round(time.monotonic() - started, 1)
    print(json.dumps(report))
    write_lines('GITHUB_ENV', [f'PLAYWRIGHT_MODULE={installer.module}'])
    write_lines('GITHUB_STEP_SUMMARY', [
        '', f'Playwright {VERSION}: {report["seconds"]}s; package {report["package"]}, '
        f'browsers {report["browsers"]["source"]} ({", ".join(report["browsers"]["installed"])}); '
        f'apt archives {report["apt"]["from_cache"]} cached, {report["apt"]["downloaded"]} downloaded, '
        f'{report["apt"]["discarded"]} discarded.', '', f'Phases: `{json.dumps(report["phases"])}`'])


if __name__ == '__main__':
    main()
