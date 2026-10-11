#!/usr/bin/env python3
"""Run the PS2-aware Ghidra analysis; retain all game-derived output privately.

REA's published provider currently rejects this machine-specific MIPS target.
This direct-engine adapter is explicitly separate from REA Evidence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = '11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('addresses', nargs='*', default=['0x00100008'])
    parser.add_argument('--data', nargs='*', default=[], help='Export data references without disassembling data')
    parser.add_argument('--ghidra', type=Path, default=Path(os.environ.get(
        'GHIDRA_INSTALL_DIR', ROOT / 'work/toolchains/ghidra_12.1.4_PUBLIC')))
    args = parser.parse_args()
    for address in args.addresses + args.data:
        if not address.startswith('0x') or not 0 <= int(address, 16) < 2**32:
            parser.error('Use explicit ELF addresses in 0x hexadecimal form')
    executable = ROOT / 'private/extracted/SCUS_972.11'
    if hashlib.sha256(executable.read_bytes()).hexdigest() != EXPECTED:
        raise ValueError('Source executable does not match USA v2.00')
    launcher = args.ghidra / 'support/analyzeHeadless'
    if not launcher.is_file():
        parser.error('Install Ghidra with the matching Emotion Engine extension; see research/REA.md')
    project = ROOT / 'work/ghidra'
    if not project.is_dir() or not project.is_symlink():
        parser.error('Prepare SSD storage: ~/.local/bin/ssd-build-dir "$PWD" work/ghidra')
    output = ROOT / 'private/reports/ghidra'
    logs = ROOT / 'work/rea'
    logs.mkdir(parents=True, exist_ok=True)
    command = [str(launcher), str(project), 'atv2']
    if (project / 'atv2.gpr').exists():
        command += ['-process', executable.name, '-noanalysis', '-readOnly']
    else:
        command += ['-import', str(executable), '-processor', 'r5900:LE:32:default',
                    '-cspec', 'default', '-preScript', 'AtvPrepare.java',
                    '-analysisTimeoutPerFile', '600', '-max-cpu', '4']
    command += ['-scriptPath', str(ROOT / 'tools'), '-postScript', 'AtvRecover.java',
                str(output), *args.addresses, *['data:' + address for address in args.data]]
    started = time.time()
    log = logs / f'analysis-{time.time_ns()}.log'
    with log.open('w') as stream:
        result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT)
    if hashlib.sha256(executable.read_bytes()).hexdigest() != EXPECTED:
        raise ValueError('Source executable changed during analysis')
    inventory = output / 'inventory.json'
    if result.returncode or not inventory.exists() or inventory.stat().st_mtime < started:
        raise RuntimeError(f'Analysis/export failed; inspect {log}')
    report = json.loads(inventory.read_text())
    if (report['language'] != 'r5900:LE:32:default'
            or report['source_sha256'] != EXPECTED
            or not report['loaded_segments']
            or not all(s['byte_identical'] for s in report['loaded_segments'])):
        raise ValueError('Analysis identity or loaded-byte verification failed')
    print(f'Private Ghidra observations: {inventory}')
    print(f'{len(report["functions"])} analyzed functions (not verified identities); '
          'original load bytes match. No game code executed.')
    print(f'Log: {log}')


if __name__ == '__main__':
    main()
