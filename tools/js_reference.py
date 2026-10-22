#!/usr/bin/env python3
"""The retired JavaScript game at a pinned pre-switch commit, for references.

After the switch-over play.html serves the Rust engine and the JavaScript game
(web/game.js and its modules, worker.js, crates/atv-web) is gone from the tree.
Parity tools (tools/compare_engine.mjs, compare_audio.mjs), JavaScript guests in
the multiplayer tests and the `reference` browser-test group still need it, so
this builds the last pre-switch commit in an ignored worktree and serves it:

  python3 tools/js_reference.py prepare             # work/js-reference, built web/dist
  python3 tools/js_reference.py serve [--port 4178] # that build with the private content
  python3 tools/js_reference.py test [runner args]  # its own browser tests (the
                                                    # JavaScript game's suite), e.g.
                                                    # --group content --only test_race.mjs
  ATV_JS_URL=http://127.0.0.1:4178 node tools/compare_engine.mjs env01

For multiplayer JavaScript guests, put the reference behind its own relay proxy
(node tools/serve_multiplayer.mjs with PORT=4188 ASSET_PORT=4178) and point
ATV_JS_URL there. See research/SWITCHOVER.md.
"""
import argparse
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# origin/main before the switch-over: the last commit with the JavaScript game.
REFERENCE = '4783519a6f86173e6afce9ce7a679dfe6a8169ed'
CHECKOUT = ROOT / 'work/js-reference'


def run(*command, cwd=ROOT):
    subprocess.run(command, cwd=cwd, check=True)


def prepare():
    if not CHECKOUT.exists():
        run('git', 'fetch', 'origin')
        run('git', 'worktree', 'add', '--detach', str(CHECKOUT), REFERENCE)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=CHECKOUT, text=True).strip()
    if head != REFERENCE:
        sys.exit(f'{CHECKOUT} is at {head}, not the pinned reference {REFERENCE}; remove it and prepare again')
    private = CHECKOUT / 'private'
    if not private.exists():
        private.symlink_to(ROOT / 'private')
    env = dict(os.environ)
    # Its own make web: the JavaScript game, atv_web.wasm and the shell.
    for script in ('tools/build_web.py', 'tools/build_engine.py'):
        subprocess.run([sys.executable, script], cwd=CHECKOUT, env=env, check=True)
    print(f'JavaScript reference {REFERENCE[:12]} built in {CHECKOUT / "web/dist"}')


def serve(port):
    if not (CHECKOUT / 'web/dist/game.js').is_file():
        sys.exit('run: python3 tools/js_reference.py prepare')
    os.execv(sys.executable, [sys.executable, str(CHECKOUT / 'tools/serve_web.py'), '--port', str(port),
                              '--root', str(CHECKOUT / 'web/dist'), '--project', str(ROOT),
                              '--content', str(ROOT / 'private/reports/hosted-content')])


def test(arguments):
    if not (CHECKOUT / 'web/dist/game.js').is_file():
        sys.exit('run: python3 tools/js_reference.py prepare')
    # The reference checkout's runner serves its own web/dist with the content
    # (private/ is linked into it).
    os.execvp('node', ['node', str(CHECKOUT / 'tools/run_browser_tests.mjs'), *arguments])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('prepare')
    serve_parser = sub.add_parser('serve')
    serve_parser.add_argument('--port', type=int, default=4178)
    sub.add_parser('test', help='run the reference checkout runner; extra arguments pass through')
    args, rest = parser.parse_known_args()
    if args.command == 'prepare':
        prepare()
    elif args.command == 'serve':
        serve(args.port)
    else:
        test(rest)


if __name__ == '__main__':
    main()
