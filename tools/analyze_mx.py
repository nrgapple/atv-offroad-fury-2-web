#!/usr/bin/env python3
"""Exact-build private R5900 analysis of MX vs. ATV Unleashed and its game module.

This reads/decompiles instructions; it never executes the game. It does not
establish complete function identities or gameplay equivalence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
BOOT_SHA = '4b74c0c76ffe0838e00c1b9f75b31e1bb91adae06ec9da1416355b363572102a'
MODULE_SHA = 'bea3eb36260f5ab64d9de1919318593a6a58b08fb2c160e9450f5a0e1e334957'


def checked_source(path, digest):
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise ValueError(f'Exact-build source identity mismatch: {path.name}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('addresses', nargs='*', default=[])
    parser.add_argument('--game-module', action='store_true', help='Map/recheck the original game module and analyze its code')
    parser.add_argument('--ghidra', type=Path, default=ROOT / 'work/toolchains/ghidra_12.1.4_PUBLIC')
    parser.add_argument('--java', type=Path, default=None)
    args = parser.parse_args()
    for address in args.addresses:
        value = address.removeprefix('data:')
        if not value.startswith('0x') or not 0 <= int(value, 16) < 2**32:
            parser.error('Use explicit 0x addresses, optionally prefixed with data:')
    executable = (ROOT / 'private/extracted/mx-unleashed/SLUS_211.04').resolve()
    module = (executable.parent / 'GAME_REL.BIN').resolve()
    checked_source(executable, BOOT_SHA)
    if args.game_module:
        checked_source(module, MODULE_SHA)
    project = ROOT / 'work/ghidra-mx'
    if not project.is_symlink() or not project.is_dir():
        parser.error('Prepare SSD storage with ssd-build-dir "$PWD" work/ghidra-mx')
    ghidra = args.ghidra.resolve()
    utility = ghidra / 'Ghidra/Framework/Utility/lib/Utility.jar'
    if not utility.is_file():
        parser.error('The installed PS2-aware Ghidra toolchain is missing; see research/REA.md')
    candidates = [args.java]
    if os.environ.get('JAVA_HOME'):
        candidates.append(Path(os.environ['JAVA_HOME']) / 'bin/java')
    candidates += [Path('/opt/homebrew/opt/openjdk/bin/java'), Path(shutil.which('java') or '/missing')]
    java = next((p for p in candidates if p and p.is_file()), None)
    if java is None:
        parser.error('Supply --java for a supported installed JDK')
    output = (ROOT / 'private/reports/mx-unleashed/ghidra').resolve()
    log_dir = ROOT / 'work'
    log_dir.mkdir(exist_ok=True)
    log = log_dir / f'mx-analysis-{time.time_ns()}.log'
    # Direct Java invocation avoids the shell launcher's slow per-argument expr
    # calls. Resolving the project symlink also avoids Ghidra rejecting .codex.
    command = [str(java), '-Xmx2G', '-Djava.awt.headless=true',
               '-Djava.system.class.loader=ghidra.GhidraClassLoader',
               '-Dfile.encoding=UTF8', '-cp', str(utility), 'ghidra.Ghidra',
               'ghidra.app.util.headless.AnalyzeHeadless', str(project.resolve()), 'mx-direct']
    exists = (project / 'mx-direct.gpr').exists()
    if exists:
        command += ['-process', executable.name]
        if not args.game_module:
            command += ['-noanalysis', '-readOnly']
    else:
        command += ['-import', str(executable), '-processor', 'r5900:LE:32:default',
                    '-cspec', 'default', '-preScript', 'AtvPrepare.java']
    if args.game_module:
        command += ['-preScript', 'MxLoadModule.java', str(module)]
    command += ['-analysisTimeoutPerFile', '600', '-max-cpu', '4',
                '-scriptPath', str(ROOT / 'tools'), '-postScript', 'AtvRecover.java',
                str(output), *(args.addresses or ['0x00100008'])]
    started = time.time()
    with log.open('w') as stream:
        result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT)
    checked_source(executable, BOOT_SHA)
    if args.game_module:
        checked_source(module, MODULE_SHA)
    inventory = output / 'inventory.json'
    if result.returncode or not inventory.is_file() or inventory.stat().st_mtime < started:
        raise RuntimeError(f'Analysis/export failed; inspect {log}')
    report = json.loads(inventory.read_text())
    if report['source_sha256'] != BOOT_SHA or report['language'] != 'r5900:LE:32:default' \
            or not report['loaded_segments'] or not all(s['byte_identical'] for s in report['loaded_segments']):
        raise ValueError('Exported source identity or original segment bytes do not match')
    if args.game_module and '854016 original bytes match' not in log.read_text():
        raise ValueError('Module byte verification was not confirmed')
    print(f'Private analysis: {inventory}; {len(report["functions"])} function candidates, not verified identities')
    print(f'Log: {log}')


if __name__ == '__main__':
    main()
