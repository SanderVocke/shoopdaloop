#!/usr/bin/env python3
"""Measure individual processing subsystems on the ZedBoard at 150 MHz."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from run import ROOT, HERE, context


PROFILES = {
    'state': ('state_processor', 'state_processor', {
        'PROGRAM_DEPTH': 512, 'STATE_DEPTH': 512, 'JOB_DEPTH': 512,
    }),
    'job': ('job_processor', 'job_processor', {
        'EXTERNAL_IO': 1, 'ENABLE_MEDIA': 1, 'MAX_FRAMES': 32,
        'BUFFERS': 256, 'INPUT_PORTS': 8, 'OUTPUT_PORTS': 8, 'CHUNK_FRAMES': 32,
    }),
    'memory': ('axi_memory', 'media_store_axi', {
        'VIEWS': 64, 'SLOTS': 2048, 'CHUNK_FRAMES': 32, 'BLOCK_CHUNKS': 32,
        'READ_BUFFERS': 4, 'WRITE_BUFFERS': 4, 'TABLE_LINES': 8, 'REF_LINES': 4,
        'STACK_LINES': 2, 'AXI_DATA_WIDTH': 64, 'BURST_MEMORY': 1,
    }),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('subsystem', choices=PROFILES)
    parser.add_argument('--vivado', default='vivado')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--stage', choices=('synth', 'place', 'route'), default='route')
    parser.add_argument('--threads', type=int, default=4)
    parser.add_argument('--strategy', choices=('default', 'timing'), default='default')
    args = parser.parse_args()
    if args.threads < 1:
        parser.error('threads must be positive')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error('output must be empty to preserve run provenance')
    directory, top, parameters = PROFILES[args.subsystem]
    hashes = {}
    for library in ('state_processor', 'job_processor', 'transition_processor',
                    'chunk_memory', 'media_store', 'loop_pipeline', 'axi_memory'):
        for path in sorted((ROOT / 'fpga' / library / 'rtl').glob('*')):
            if path.suffix not in ('.sv', '.svh'):
                continue
            relative = path.relative_to(ROOT)
            destination = output / 'source' / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
            hashes[str(relative)] = hashlib.sha256(path.read_bytes()).hexdigest()
    (output / 'context.sv').write_text(context(
        (ROOT / 'fpga' / directory / 'rtl' / f'{top}.sv').read_text(), top))
    shutil.copyfile(HERE / 'subsystems.tcl', output / 'run.tcl')
    (output / 'context.xdc').write_text(
        'create_clock -name core_clk -period 6.666 [get_ports clk]\n'
        'set_clock_uncertainty 0.200 [get_clocks core_clk]\n'
        'set_property HD.CLK_SRC BUFGCTRL_X0Y0 [get_ports clk]\n')
    (output / 'source-manifest.json').write_text(json.dumps(hashes, indent=2) + '\n')
    artifacts = ('context.sv', 'context.xdc', 'run.tcl')
    (output / 'artifact-manifest.json').write_text(json.dumps({
        name: hashlib.sha256((output / name).read_bytes()).hexdigest()
        for name in artifacts
    }, indent=2) + '\n')
    (output / 'provenance.json').write_text(json.dumps(dict(
        revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        working_tree_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
        command=sys.argv, subsystem=args.subsystem, top=top, parameters=parameters,
        part='xc7z020clg484-1', clock_mhz=150, uncertainty_ns=0.2,
        stage=args.stage, threads=args.threads,
        strategy=args.strategy,
        implementation_setup_uncertainty_ns=0.9 if args.strategy == 'timing' else 0.2,
    ), indent=2) + '\n')
    command = [args.vivado, '-mode', 'batch', '-nojournal', '-log', 'vivado.log',
               '-source', 'run.tcl', '-tclargs', args.stage, str(args.threads),
               ' '.join(f'{key}={value}' for key, value in parameters.items()),
               args.strategy]
    if sys.platform == 'linux':
        import ctypes
        if ctypes.CDLL(None).prctl(36, 1, 0, 0, 0) != 0:
            raise RuntimeError('cannot enable child subreaper')
    with (output / 'console.log').open('w') as log:
        process = subprocess.Popen(command, cwd=output, stdout=log, stderr=subprocess.STDOUT)
        main_status = None
        while True:
            try:
                pid, status = os.waitpid(-1, 0)
            except ChildProcessError:
                break
            if pid == process.pid:
                main_status = os.waitstatus_to_exitcode(status)
    if main_status != 0:
        raise SystemExit(f'Vivado failed ({main_status}); see {output}/console.log')
    result = json.loads((output / f'post_{args.stage}.json').read_text())
    print(json.dumps(result, indent=2))
    if args.stage == 'route' and not result['met_150mhz']:
        raise SystemExit('Routed subsystem does not meet 150 MHz; see timing reports')


if __name__ == '__main__':
    main()
