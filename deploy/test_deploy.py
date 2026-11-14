#!/usr/bin/env python3
"""deploy.sh against a stand-in host: ssh runs fury-release in a temporary
root, "restarting" records what the current binary serves, and a local HTTP
server answers /healthz from that record. Covers first deploy, upgrade,
rollback of an unhealthy release, a binary that does not run, the no-op
redeploy and pruning. Run: python3 deploy/test_deploy.py"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import os
import subprocess
import tempfile
import threading
import unittest

DEPLOY = Path(__file__).resolve().parent

SSH = r'''#!/usr/bin/env bash
# Drop ssh options up to user@host; the rest is the forced command's request.
while [[ $# -gt 0 && $1 != *@* ]]; do shift; done
shift
SSH_ORIGINAL_COMMAND="$*" exec "$FURY_RELEASE"
'''

# The service: what current/fury-server serves, or nothing if it fails to start.
RESTART = r'''#!/usr/bin/env bash
if out=$("$FURY_ROOT/current/fury-server" --serve); then
  . "$FURY_ROOT/current/release.env"
  echo "$FURY_COMMIT" >"$FURY_HEALTH_STATE"
else
  : >"$FURY_HEALTH_STATE"
fi
'''

BINARIES = {
    'good': '#!/bin/sh\n[ "$1" = --version ] && echo "fury-server test" && exit 0\n[ "$1" = --serve ] && exit 0\nexit 1\n',
    'unhealthy': '#!/bin/sh\n[ "$1" = --version ] && echo "fury-server test" && exit 0\nexit 1\n',
    'broken': '#!/bin/sh\necho "cannot execute" >&2\nexit 126\n',
}


class Health(BaseHTTPRequestHandler):
    def do_GET(self):
        commit = Path(self.server.state).read_text().strip() if Path(self.server.state).exists() else ''
        body = json.dumps({'ok': True, 'commit': commit, 'protocol': 6, 'uptimeSeconds': 1,
                           'rooms': 0, 'riders': 0, 'connections': 0}).encode()
        self.send_response(200 if commit else 503)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body if commit else b'{"ok":false}')

    def log_message(self, *args):
        pass


class DeployTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        base = Path(self.temp.name)
        self.root = base / 'opt'
        self.state = base / 'health'
        bin_dir = base / 'bin'
        bin_dir.mkdir()
        for name, text in {'ssh': SSH, 'restart': RESTART}.items():
            (bin_dir / name).write_text(text)
            (bin_dir / name).chmod(0o755)
        self.binaries = {}
        for name, text in BINARIES.items():
            path = base / f'{name}-server'
            path.write_text(text)
            self.binaries[name] = path
        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Health)
        self.server.state = str(self.state)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.env = {**os.environ,
                    'PATH': f'{bin_dir}:{os.environ["PATH"]}',
                    'FURY_RELEASE': str(DEPLOY / 'fury-release'),
                    'FURY_ROOT': str(self.root),
                    'FURY_RESTART': str(bin_dir / 'restart'),
                    'FURY_HEALTH_STATE': str(self.state),
                    'FURY_HEALTH_URL': f'http://127.0.0.1:{self.server.server_port}/healthz',
                    'FURY_HEALTH_TIMEOUT': '3'}

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.temp.cleanup()

    def deploy(self, binary, commit, **env):
        return subprocess.run([str(DEPLOY / 'deploy.sh'), 'mp.example.test', str(self.binaries[binary]), commit],
                              env={**self.env, **env}, capture_output=True, text=True, timeout=60)

    def current(self, link='current'):
        return os.path.basename(os.readlink(self.root / link))

    def live(self):
        return self.state.read_text().strip()

    def test_deploy_upgrade_rollback_and_noop(self):
        first = self.deploy('good', 'a' * 40)
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual((self.current(), self.live()), ('a' * 40, 'a' * 40))
        self.assertEqual((self.root / 'releases' / ('a' * 40) / 'release.env').read_text(), f'FURY_COMMIT={"a" * 40}\n')

        second = self.deploy('good', 'b' * 40)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual((self.current(), self.current('previous'), self.live()), ('b' * 40, 'a' * 40, 'b' * 40))

        bad = self.deploy('unhealthy', 'c' * 40)
        self.assertEqual(bad.returncode, 1)
        self.assertIn('b' * 40 + ' is healthy again', bad.stderr)
        self.assertEqual((self.current(), self.live()), ('b' * 40, 'b' * 40))

        again = self.deploy('good', 'b' * 40)
        self.assertEqual(again.returncode, 0, again.stderr)
        self.assertIn('already live', again.stdout)

    def test_first_deploy_failure_has_nothing_to_roll_back(self):
        result = self.deploy('unhealthy', 'd' * 40)
        self.assertEqual(result.returncode, 1)
        self.assertIn('no previous release', result.stderr)

    def test_binary_that_does_not_run_is_never_activated(self):
        self.assertEqual(self.deploy('good', 'a' * 40).returncode, 0)
        result = self.deploy('broken', 'e' * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('binary does not run', result.stderr)
        self.assertEqual((self.current(), self.live()), ('a' * 40, 'a' * 40))
        self.assertFalse((self.root / 'releases' / ('e' * 40) / 'fury-server').exists())

    def test_rejects_bad_arguments(self):
        for commit in ['main', 'A' * 40, 'abc', 'a' * 41]:
            with self.subTest(commit=commit):
                self.assertEqual(self.deploy('good', commit).returncode, 2)

    def test_prune_keeps_newest_and_both_links(self):
        commits = [f'{n:x}' * 40 for n in range(1, 8)]
        for commit in commits:
            result = self.deploy('good', commit, FURY_KEEP='3')
            self.assertEqual(result.returncode, 0, result.stderr)
            # Distinct modification times for the newest-first order.
            os.utime(self.root / 'releases' / commit, (0, 1_000_000 + int(commit[0], 16)))
        left = sorted(path.name for path in (self.root / 'releases').iterdir())
        self.assertEqual(left, sorted(commits[-3:]))


if __name__ == '__main__':
    unittest.main()
