#!/usr/bin/env python3
"""Activate successful private GitHub main artifacts on the existing Tailscale host.

Installed as a persistent user LaunchAgent. No checkout changes, host builds,
GitHub runner, public listener, or private-source upload is involved.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time
from urllib.request import urlopen

STATIC_FILES = {'index.html', 'game.css', 'game.js', 'audio.js', 'devlog.html',
                'devlog.css', 'devlog.js', 'workbench.html', 'style.css', 'app.js',
                'worker.js', 'input.js', 'controls.js', 'atv_web.wasm', 'deployment.json', 'music.js'}
EXPECTED_FILES = {'serve_web.py', 'release.json'} | {'static/' + name for name in STATIC_FILES}


def validate_release(root, expected_commit):
    paths = list(root.rglob('*'))
    if any(path.is_symlink() for path in paths):
        raise ValueError('release contains a symlink')
    actual = {path.relative_to(root).as_posix() for path in paths if path.is_file()}
    if actual != EXPECTED_FILES:
        raise ValueError(f'release file allowlist mismatch: {sorted(actual ^ EXPECTED_FILES)}')
    manifest = json.loads((root / 'release.json').read_text())
    if manifest['commit'] != expected_commit:
        raise ValueError('release commit does not match successful main build')
    if set(manifest['sha256']) != EXPECTED_FILES - {'release.json'}:
        raise ValueError('release checksum list mismatch')
    for name, digest in manifest['sha256'].items():
        path = root / name
        if path.stat().st_size > 20_000_000:
            raise ValueError(f'release file too large: {name}')
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f'release checksum mismatch: {name}')
    if (root / 'static/atv_web.wasm').read_bytes()[:4] != b'\0asm':
        raise ValueError('invalid WebAssembly header')
    if json.loads((root / 'static/deployment.json').read_text())['commit'] != expected_commit:
        raise ValueError('deployment metadata commit mismatch')
    return manifest


def atomic_json(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    os.replace(temporary, path)


class Deployer:
    def __init__(self, config):
        self.config = config
        if config.get('volume') and not Path(config['volume']).is_mount():
            raise RuntimeError('authorized External SSD is not mounted')
        self.storage = Path(config['storage'])
        self.releases = self.storage / 'releases'
        self.releases.mkdir(parents=True, exist_ok=True)
        self.current = self.storage / 'current'
        self.child = None
        self.stopping = False

    def gh_json(self, endpoint):
        result = subprocess.run([self.config['gh'], 'api', endpoint], check=True,
                                capture_output=True, text=True, timeout=40)
        return json.loads(result.stdout)

    def main_commit(self):
        return self.gh_json(f"repos/{self.config['repository']}/commits/main")['sha']

    def successful_run(self, commit):
        endpoint = f"repos/{self.config['repository']}/actions/workflows/ci.yml/runs?branch=main&status=success&per_page=10"
        for run in self.gh_json(endpoint)['workflow_runs']:
            if run['head_sha'] == commit and run['event'] in ('push', 'workflow_dispatch'):
                return run
        return None

    def stop_server(self):
        if self.child and self.child.poll() is None:
            self.child.terminate()
            try:
                self.child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.child.kill()
                self.child.wait(timeout=5)
        self.child = None

    def start_server(self, release):
        self.child = subprocess.Popen([
            self.config['python'], '-u', str(release / 'serve_web.py'),
            '--root', str(release / 'static'), '--project', self.config['project'],
            '--bind', '127.0.0.1', '--port', str(self.config['port'])])
        for _ in range(50):
            if self.child.poll() is not None:
                raise RuntimeError('allowlist server exited before becoming healthy')
            try:
                with urlopen(f"http://127.0.0.1:{self.config['port']}/deployment.json", timeout=1) as response:
                    metadata = json.load(response)
                manifest = json.loads((release / 'release.json').read_text())
                if metadata['commit'] == manifest['commit']:
                    return
            except (OSError, ValueError, KeyError):
                pass
            time.sleep(0.1)
        self.stop_server()
        raise RuntimeError('allowlist server did not report expected release')

    def activate(self, release, run):
        previous = self.current.resolve() if self.current.is_symlink() else None
        self.stop_server()
        try:
            self.start_server(release)
        except Exception:
            self.stop_server()
            if previous and previous.exists():
                self.start_server(previous)
            raise
        link = self.storage / 'current-next'
        link.unlink(missing_ok=True)
        link.symlink_to(release, target_is_directory=True)
        os.replace(link, self.current)
        atomic_json(self.storage / 'status.json', {
            'commit': run['head_sha'], 'run_url': run['html_url'],
            'activated_at': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
            'private_remote_url': self.config['remote_url']})
        print(f"Activated {run['head_sha']} from {run['html_url']}", flush=True)
        # Bound disk growth while keeping recent rollback candidates.
        releases = [path for path in self.releases.iterdir()
                    if path.is_dir() and not path.is_symlink() and len(path.name) == 40
                    and all(c in '0123456789abcdef' for c in path.name)]
        for old in sorted(releases, key=lambda path: path.stat().st_mtime, reverse=True)[5:]:
            if old != release and old != previous:
                shutil.rmtree(old)

    def poll(self):
        commit = self.main_commit()
        if self.current.is_symlink() and self.current.resolve().name == commit:
            return
        run = self.successful_run(commit)
        if not run:
            return
        release = self.releases / commit
        if not release.exists():
            with tempfile.TemporaryDirectory(prefix='download-', dir=self.storage) as temporary:
                stage = Path(temporary)
                subprocess.run([self.config['gh'], 'run', 'download', str(run['id']),
                                '--repo', self.config['repository'], '--name', 'web-release',
                                '--dir', str(stage)], check=True, timeout=120)
                validate_release(stage, commit)
                if self.main_commit() != commit:
                    return
                stage.rename(release)
        validate_release(release, commit)
        if self.stopping or self.main_commit() != commit:
            return
        self.activate(release, run)

    def stop(self, *_):
        self.stopping = True
        self.stop_server()

    def watch(self):
        signal.signal(signal.SIGTERM, self.stop)
        signal.signal(signal.SIGINT, self.stop)
        if self.current.is_symlink():
            release = self.current.resolve()
            validate_release(release, release.name)
            self.start_server(release)
        while not self.stopping:
            try:
                if self.current.is_symlink() and (not self.child or self.child.poll() is not None):
                    self.start_server(self.current.resolve())
                self.poll()
            except Exception as error:
                print(f'Deployment check failed; retaining prior release: {error}', file=sys.stderr, flush=True)
            deadline = time.monotonic() + self.config.get('poll_seconds', 60)
            while not self.stopping and time.monotonic() < deadline:
                time.sleep(1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--once', action='store_true', help='Check deployment once; intended for diagnostics')
    args = parser.parse_args()
    deployer = Deployer(json.loads(args.config.read_text()))
    if args.once:
        deployer.poll()
        deployer.stop_server()
    else:
        deployer.watch()


if __name__ == '__main__':
    main()
