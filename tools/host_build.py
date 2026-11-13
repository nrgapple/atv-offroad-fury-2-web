#!/usr/bin/env python3
"""Cooperative local heavy-work lease. CI bypasses it; never stops existing work."""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
import re
from pathlib import Path
import signal
import subprocess
import sys
import time

STATE = Path.home() / '.local/share/project-service-data/fury-build-budget'
OWNER = 'FURY_BUILD_OWNER'
KIND = 'FURY_BUILD_KIND'
CHECKOUT = 'FURY_BUILD_CHECKOUT'
KINDS = ('exclusive', 'compile', 'browser')


def processes():
    # args only: macOS truncates comm to 16 characters (Homebrew Python shows as
    # /opt/homebrew/Ce), which hid waiting guards and deadlocked runners (#497).
    rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,args='], text=True)
    # Executable paths can contain spaces: use a separate unambiguous parent map.
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


def owns_lease(kind=None):
    try:
        owner = int(os.environ.get(OWNER, '0'))
        parents, _ = processes()
        held_kind = os.environ.get(KIND, 'exclusive')
        return (owner > 1 and owner in ancestors(os.getpid(), parents)
                and (kind is None or held_kind == 'exclusive'
                     or (held_kind == kind and os.environ.get(CHECKOUT) == str(checkout_root()))))
    except (ValueError, OSError, subprocess.SubprocessError):
        return False


def _executable(command):
    """Interpreter/compiler name from a process's args (never the truncated comm)."""
    words = command.split()
    return Path(words[0]).name if words else ''


def _python(command):
    name = _executable(command)
    return name.startswith('python') or name == 'Python' or 'Python' in command.split()[0]


def active_owners(directory, exclude=None):
    """Trust metadata only while its kernel lock is held (never stale PID files)."""
    owners = set()
    for kind in ('compile', 'browser'):
        if kind == exclude:
            continue
        path = directory / f'{kind}.lock'
        if not path.exists():
            continue
        with open(path, 'r+') as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                try:
                    owners.add(int(json.load(lock)['pid']))
                except (ValueError, KeyError):
                    pass
    return owners


def busy_reason(kind='exclusive'):
    """Adopt safely alongside older entry points without signalling any process."""
    if sys.platform == 'darwin':
        value = subprocess.check_output(['/usr/sbin/sysctl', '-n', 'kern.memorystatus_vm_pressure_level'], text=True)
        if int(value.strip()) != 1:
            return 'macOS memory pressure is elevated'
        if kind != 'exclusive':
            report = subprocess.check_output(['/usr/bin/memory_pressure', '-Q'], text=True)
            match = re.search(r'System-wide memory free percentage: (\d+)%', report)
            if not match or int(match[1]) < 25:
                return 'parallel work needs at least 25% reported memory headroom'
    parents, rows = processes()
    ours = ancestors(os.getpid(), parents)
    participating = active_owners(Path(os.environ.get('FURY_BUILD_STATE', STATE)), exclude=kind) if kind != 'exclusive' else set()
    # sccache's daemon, not its client, parents rustc. Match the exact compiler
    # arguments to a client in an admitted tree; do not exempt unrelated cache jobs.
    cache_requests = set()
    executables = {}
    for row in rows.splitlines():
        parts = row.split(None, 2)
        if len(parts) == 3:
            pid, command = int(parts[0]), parts[2]
            executables[pid] = _executable(command)
            suffix = command.partition(' --crate-name ')[2]
            if executables[pid] == 'sccache' and suffix and participating.intersection(ancestors(pid, parents)):
                cache_requests.add(suffix)
    guarded_parents = set()
    for row in rows.splitlines():
        parts = row.split(None, 2)
        if len(parts) == 3 and 'host_build.py' in parts[2] and _python(parts[2]):
            guarded_parents.update(ancestors(int(parts[0]), parents))
    for row in rows.splitlines():
        parts = row.split(None, 2)
        if len(parts) < 3:
            continue
        pid = int(parts[0])
        if pid in ours or os.getpid() in ancestors(pid, parents) or participating.intersection(ancestors(pid, parents)):
            continue
        # Match executable/script signatures, not incidental shell/agent prompt text.
        command = parts[2]
        compiler = _executable(command) in ('rustc', 'clippy-driver', 'rust-lld', 'xcodebuild')
        if compiler and executables.get(parents.get(pid)) == 'sccache' and command.partition(' --crate-name ')[2] in cache_requests:
            continue
        script = ((_python(command) or _executable(command).startswith('node'))
                  and 'host_build.py' not in command
                  and any(name in command for name in
                          ('/build_desktop.py', '/run_browser_tests.mjs')))
        if compiler or (script and pid not in guarded_parents):
            return f'existing heavy work (PID {pid})'
    return None


def checkout_root():
    # Commands launched from a repository subdirectory still write the same
    # checkout outputs. Canonicalize symlink entry points too.
    try:
        root = subprocess.check_output(['git', 'rev-parse', '--show-toplevel'],
                                       text=True, stderr=subprocess.DEVNULL).strip()
        return Path(root).resolve()
    except subprocess.CalledProcessError:
        return Path.cwd().resolve()


@contextlib.contextmanager
def lease(wait=1800, poll=2, state_dir=None, check_busy=True, kind='exclusive'):
    if kind not in KINDS:
        raise ValueError(f'unknown workload: {kind}')
    if os.environ.get('CI') or owns_lease(kind):
        yield ()
        return
    if owns_lease():
        # Waiting for another resource while holding one can deadlock. A browser
        # test that compiles must be launched as exclusive from the outset.
        raise ValueError(f'cannot nest {kind} inside {os.environ.get(KIND)} across workloads/checkouts; use an exclusive outer lease')
    directory = Path(state_dir or os.environ.get('FURY_BUILD_STATE', STATE))
    directory.mkdir(parents=True, exist_ok=True)
    root = str(checkout_root())
    checkout = hashlib.sha256(root.encode()).hexdigest()[:24]
    deadline = time.monotonic() + wait
    last_notice = 0
    queued = time.monotonic()
    while True:
        stack = contextlib.ExitStack()
        try:
            # Old guards take heavy.lock exclusively, so new shared leases never
            # overlap legacy jobs. Hold admission only while acquiring permits.
            with open(directory / 'admission.lock', 'a+') as admission:
                fcntl.flock(admission, fcntl.LOCK_EX | fcntl.LOCK_NB)
                paths = [('heavy.lock', fcntl.LOCK_EX if kind == 'exclusive' else fcntl.LOCK_SH)]
                if kind != 'exclusive':
                    paths += [(f'checkout-{checkout}.lock', fcntl.LOCK_EX), (f'{kind}.lock', fcntl.LOCK_EX)]
                locks = []
                for name, mode in paths:
                    lock = stack.enter_context(open(directory / name, 'a+'))
                    fcntl.flock(lock, mode | fcntl.LOCK_NB)
                    locks.append(lock)
                reason = (busy_reason() if kind == 'exclusive' else busy_reason(kind)) if check_busy else None
                if reason:
                    raise BlockingIOError(reason)
                record = locks[-1]  # Never truncate the shared compatibility lock.
                record.seek(0)
                record.truncate()
                record.write(json.dumps({'pid': os.getpid(), 'cwd': str(Path.cwd()),
                                         'kind': kind, 'started': time.time()}))
                record.flush()
            break
        except BlockingIOError as exc:
            reason = str(exc) if exc.errno is None else f'{kind} slot, checkout or exclusive work is busy'
        except (OSError, ValueError, subprocess.SubprocessError) as exc:
            reason = f'cannot verify host availability: {exc}'
        stack.close()
        if time.monotonic() >= deadline:
            raise BlockingIOError(reason)
        if time.monotonic() - last_notice >= 30:
            print(f'Build waiting ({kind}): {reason}', file=sys.stderr, flush=True)
            last_notice = time.monotonic()
        time.sleep(min(poll, max(0, deadline - time.monotonic())))
    previous = {key: os.environ.get(key) for key in (OWNER, KIND, CHECKOUT)}
    os.environ[OWNER], os.environ[KIND], os.environ[CHECKOUT] = str(os.getpid()), kind, root
    print(f'Build admitted ({kind}) after {time.monotonic() - queued:.1f}s', file=sys.stderr, flush=True)
    try:
        yield tuple(lock.fileno() for lock in locks)
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        # Children inherit every permit, including the per-checkout lock.
        stack.close()


def run(command, wait=1800, kind='exclusive'):
    with lease(wait=wait, kind=kind) as fds:
        env = dict(os.environ)
        if not env.get('CI'):
            env.setdefault('CARGO_BUILD_JOBS', '2')
        child = subprocess.Popen(command, env=env, start_new_session=True,
                                 pass_fds=fds)
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


def ensure_guard(script, args, kind='exclusive'):
    if not os.environ.get('CI') and not owns_lease(kind):
        try:
            raise SystemExit(run([sys.executable, str(script), *args], kind=kind))
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
    # Only known compiler commands use the compile permit. cargo run and custom
    # aliases/plugins can do arbitrary work and remain exclusive.
    kind = 'compile' if args[0] in {'build', 'check', 'clippy', 'test', 'rustc'} else 'exclusive'
    return run(command, kind=kind)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wait', type=float, default=1800)
    parser.add_argument('--owns', action='store_true')
    parser.add_argument('--kind', choices=KINDS, default='exclusive')
    parser.add_argument('--cargo-proxy', action='store_true')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.owns:
        return 0 if owns_lease(args.kind) else 1
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    try:
        if args.cargo_proxy:
            return cargo_proxy(command)
        if not command:
            parser.error('a command is required')
        return run(command, wait=args.wait, kind=args.kind)
    except BlockingIOError as exc:
        print(f'Build deferred: {exc}', file=sys.stderr)
        return 75


if __name__ == '__main__':
    sys.exit(main())
