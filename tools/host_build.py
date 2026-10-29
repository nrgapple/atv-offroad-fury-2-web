#!/usr/bin/env python3
"""Cooperative local heavy-work lease. CI bypasses it; never stops existing work."""
import argparse
import contextlib
import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

STATE = Path.home() / '.local/share/project-service-data/fury-build-budget'
OWNER = 'FURY_BUILD_OWNER'


def processes():
    rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,comm=,args='], text=True)
    # comm can contain spaces: use a separate unambiguous parent map.
    parents = {}
    for row in subprocess.check_output(['ps', '-axo', 'pid=,ppid='], text=True).splitlines():
        pid, parent = row.split()
        parents[int(pid)] = int(parent)
    return parents, rows


def ancestors(pid, parents):
    result = set()
    while pid > 1 and pid not in result:
        result.add(pid)
        pid = parents.get(pid, 0)
    return result


def owns_lease():
    try:
        owner = int(os.environ.get(OWNER, '0'))
        parents, _ = processes()
        return owner > 1 and owner in ancestors(os.getpid(), parents)
    except (ValueError, OSError, subprocess.SubprocessError):
        return False


def busy_reason():
    """Adopt safely alongside older entry points without signalling any process."""
    if sys.platform == 'darwin':
        value = subprocess.check_output(['sysctl', '-n', 'kern.memorystatus_vm_pressure_level'], text=True)
        if int(value.strip()) != 1:
            return 'macOS memory pressure is elevated'
    parents, rows = processes()
    ours = ancestors(os.getpid(), parents)
    guarded_parents = set()
    for row in rows.splitlines():
        parts = row.split(None, 2)
        if len(parts) == 3 and 'host_build.py' in parts[2] and ('Python' in parts[2].split()[0] or Path(parts[2].split()[0]).name.startswith('python')):
            guarded_parents.update(ancestors(int(parts[0]), parents))
    for row in rows.splitlines():
        parts = row.split(None, 2)
        if len(parts) < 3:
            continue
        pid = int(parts[0])
        if pid in ours or os.getpid() in ancestors(pid, parents):
            continue
        # Match executable/script signatures, not incidental shell/agent prompt text.
        command = parts[2]
        first = command.split()[0]
        compiler = Path(first).name in ('rustc', 'clippy-driver', 'rust-lld', 'xcodebuild')
        script = (('Python' in first or Path(first).name.startswith(('python', 'node')))
                  and 'host_build.py' not in command
                  and any(name in command for name in
                          ('/build_desktop.py', '/run_browser_tests.mjs')))
        if compiler or (script and pid not in guarded_parents):
            return f'existing heavy work (PID {pid})'
    return None


@contextlib.contextmanager
def lease(wait=1800, poll=2, state_dir=None, check_busy=True):
    if os.environ.get('CI') or owns_lease():
        yield None
        return
    directory = Path(state_dir or os.environ.get('FURY_BUILD_STATE', STATE))
    directory.mkdir(parents=True, exist_ok=True)
    deadline = time.monotonic() + wait
    with open(directory / 'heavy.lock', 'a+') as lock:
        last_notice = 0
        while True:
            acquired = False
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                acquired = True
                reason = busy_reason() if check_busy else None
            except BlockingIOError:
                reason = 'another guarded build/test owns the slot'
            except (OSError, ValueError, subprocess.SubprocessError) as exc:
                reason = f'cannot verify host availability: {exc}'
            if acquired and not reason:
                break
            if acquired:
                fcntl.flock(lock, fcntl.LOCK_UN)
            if time.monotonic() >= deadline:
                raise BlockingIOError(reason)
            if time.monotonic() - last_notice >= 30:
                print(f'Build waiting: {reason}', file=sys.stderr, flush=True)
                last_notice = time.monotonic()
            time.sleep(min(poll, max(0, deadline - time.monotonic())))
        previous = os.environ.get(OWNER)
        os.environ[OWNER] = str(os.getpid())
        lock.seek(0)
        lock.truncate()
        lock.write(json.dumps({'pid': os.getpid(), 'cwd': str(Path.cwd()), 'started': time.time()}))
        lock.flush()
        try:
            yield lock.fileno()
        finally:
            if previous is None:
                os.environ.pop(OWNER, None)
            else:
                os.environ[OWNER] = previous
            # Closing releases this holder; a surviving child retains its inherited fd.


def run(command, wait=1800):
    with lease(wait=wait) as fd:
        env = dict(os.environ)
        if not env.get('CI'):
            env.setdefault('CARGO_BUILD_JOBS', '2')
        child = subprocess.Popen(command, env=env, start_new_session=True,
                                 pass_fds=(() if fd is None else (fd,)))
        old = {}
        cancelled = [None]
        def forward(signum, _frame):
            cancelled[0] = time.monotonic()
            try:
                os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            old[sig] = signal.signal(sig, forward)
        try:
            while True:
                try:
                    returncode = child.wait(timeout=.25)
                    break
                except subprocess.TimeoutExpired:
                    if cancelled[0] is not None and time.monotonic() - cancelled[0] > 5:
                        forward(signal.SIGKILL, None)
            return returncode if returncode >= 0 else 128 - returncode
        finally:
            for sig, handler in old.items():
                signal.signal(sig, handler)


def ensure_guard(script, args):
    if not os.environ.get('CI') and not owns_lease():
        try:
            raise SystemExit(run([sys.executable, str(script), *args]))
        except BlockingIOError as exc:
            raise SystemExit(f'Build deferred: {exc}')


def grandfathered():
    """Only build trees already active when the host installer ran may finish unchanged."""
    try:
        records = json.loads((STATE / 'grandfather.json').read_text())
        parents, _ = processes()
        family = ancestors(os.getpid(), parents)
        for record in records:
            if record['pid'] in family:
                identity = subprocess.check_output(['ps', '-p', str(record['pid']), '-o', 'lstart='], text=True).strip()
                if identity == record['started']:
                    return True
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
        pass
    return False


def cargo_proxy(args):
    rustup = Path.home() / '.cargo/bin/rustup'
    override = args.pop(0)[1:] if args and args[0].startswith('+') else None
    if override:
        os.environ['RUSTUP_TOOLCHAIN'] = override
    query = [str(rustup), 'which', *(['--toolchain', override] if override else []), 'cargo']
    cargo = subprocess.check_output(query, text=True).strip()
    # Informational commands do not consume the heavy slot. Unknown subcommands
    # are guarded because cargo aliases/plugins can compile.
    light = {'--version', '-V', '--help', '-h', 'help', 'metadata', 'locate-project',
             'version', 'fmt', 'search', 'tree', 'pkgid'}
    command = [cargo, *args]
    if not args or args[0] in light or os.environ.get('CI') or grandfathered():
        os.execv(cargo, command)
    return run(command)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wait', type=float, default=1800)
    parser.add_argument('--owns', action='store_true')
    parser.add_argument('--cargo-proxy', action='store_true')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.owns:
        return 0 if owns_lease() else 1
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    try:
        if args.cargo_proxy:
            return cargo_proxy(command)
        if not command:
            parser.error('a command is required')
        return run(command, wait=args.wait)
    except BlockingIOError as exc:
        print(f'Build deferred: {exc}', file=sys.stderr)
        return 75


if __name__ == '__main__':
    sys.exit(main())
