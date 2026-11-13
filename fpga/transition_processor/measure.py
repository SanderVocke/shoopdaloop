#!/usr/bin/env python3
"""Measure transition processor resources in a registered Vivado timing context."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / 'fpga'))
from synthesis.run import context


def summarize(output, stage):
    path = output / f'{stage}.json'
    result = json.loads(path.read_text())
    report = (output / f'{stage}_tp_utilization.rpt').read_text()
    fields = {'luts': 'Slice LUTs', 'flip_flops': 'Slice Registers',
              'bram36_equivalent': 'Block RAM Tile', 'dsps': 'DSPs'}
    result['utilization'] = {}
    for name, label in fields.items():
        match = re.search(r'\|\s*' + re.escape(label) + r'\*?\s*\|\s*([\d.]+)', report)
        if match is None:
            raise RuntimeError(f'missing {label} in utilization report')
        result['utilization'][name] = float(match[1]) if name == 'bram36_equivalent' else int(match[1])
    path.write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--vivado', default='vivado')
    parser.add_argument('--source', type=Path, default=HERE / 'rtl')
    parser.add_argument('--stage', choices=('synth', 'route'), default='synth')
    parser.add_argument('--context', choices=('native', 'shared'), default='native')
    parser.add_argument('--loops', type=int, default=64)
    parser.add_argument('--requests', type=int, default=272)
    parser.add_argument('--events', type=int, default=400)
    parser.add_argument('--integrated', type=int, choices=(0, 1), default=1)
    parser.add_argument('--threads', type=int, default=4)
    args = parser.parse_args()
    if min(args.loops, args.requests, args.events, args.threads) < 1:
        parser.error('capacities and thread count must be positive')
    source = args.source.resolve()
    if not (source / 'transition_processor.sv').is_file():
        parser.error('source must contain transition_processor.sv')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error('output must be empty to preserve run provenance')
    (output / 'source').mkdir()
    hashes = {}
    for path in sorted(source.glob('*.sv')):
        shutil.copyfile(path, output / 'source' / path.name)
        hashes[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    rtl = (source / 'transition_processor.sv').read_text()
    independent_execution = 'input wire execute_valid' in rtl
    if args.context == 'shared':
        adapter = HERE / 'measurement_adapter.sv'
        shutil.copyfile(adapter, output / adapter.name)
        generated = context(adapter.read_text(), 'transition_measure_adapter')
    else:
        generated = context(rtl, 'transition_processor')
    generated = generated.replace('`include "state_processor_defs.svh"\n', '')
    (output / 'context.sv').write_text(generated)
    shutil.copyfile(HERE / 'measure.tcl', output / 'run.tcl')
    shutil.copyfile(ROOT / 'fpga/synthesis/core.xdc', output / 'context.xdc')
    (output / 'source-manifest.json').write_text(json.dumps(hashes, indent=2) + '\n')
    artifacts = [output / name for name in ('context.sv', 'context.xdc', 'run.tcl')]
    if args.context == 'shared':
        artifacts.append(output / 'measurement_adapter.sv')
    (output / 'artifact-manifest.json').write_text(json.dumps({
        path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in artifacts
    }, indent=2) + '\n')
    (output / 'provenance.json').write_text(json.dumps(dict(
        revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        working_tree_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
        command=sys.argv, part='xc7z020clg484-1', loops=args.loops, requests=args.requests,
        events=args.events, integrated=args.integrated, clock_ns=10, uncertainty_ns=0.2,
        threads=args.threads, stage=args.stage, context=args.context,
        independent_execution=independent_execution,
    ), indent=2) + '\n')
    command = [args.vivado, '-mode', 'batch', '-nojournal', '-log', 'vivado.log',
               '-source', 'run.tcl', '-tclargs', args.stage, str(args.threads),
               str(args.loops), str(args.requests), str(args.events), str(args.integrated)]
    if args.context == 'shared':
        command.append(str(int(independent_execution)))
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
    result = summarize(output, 'post_synth')
    if args.stage == 'route':
        result = summarize(output, 'post_route')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
