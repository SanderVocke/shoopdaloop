#!/usr/bin/env python3
"""Run the fixed callback matrix sequentially and retain binary provenance."""

import argparse
import itertools
import json
from pathlib import Path
import subprocess
import time

from area_results import benchmark, digest


def cases():
    return [{'name': f'{scenario}-{state}-delay{delay}', 'scenario': scenario,
             'state': state, 'delay': delay}
            for scenario, state, delay in itertools.product(
                ('audio', 'midi', 'mixed'), ('off', 'on'), (0, 7))]


def run(binary, output, parameters=None, selected=None, quanta=16):
    binary = binary.resolve()
    if output.exists():
        raise ValueError('Use a new output directory')
    rows = [row for row in cases() if selected is None or row['name'] in selected]
    if not rows or (selected and set(selected)-{row['name'] for row in rows}):
        raise ValueError('Unknown or empty workload selection')
    output.mkdir(parents=True)
    manifest = {'schema_version': 1, 'binary': str(binary), 'binary_sha256': digest(binary),
                'quanta': quanta, 'complete': False, 'runs': []}
    if parameters:
        manifest['parameters_sha256'] = digest(parameters)
        (output/'parameters.json').write_bytes(parameters.read_bytes())
    path = output/'manifest.json'
    path.write_text(json.dumps(manifest, indent=2)+'\n')
    for row in rows:
        if digest(binary) != manifest['binary_sha256']:
            raise ValueError('Benchmark executable changed during the matrix')
        name = row['name']
        result = output/f'{name}.json'
        command = [str(binary), '--scenario', row['scenario'], '--state', row['state'],
                   '--ddr-delay', str(row['delay']), '--quanta', str(quanta), '--output', str(result)]
        if parameters:
            command.extend(['--parameters', str(output/'parameters.json')])
        started = time.monotonic()
        with (output/f'{name}.log').open('w') as log:
            status = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT).returncode
        entry = row | {'command': command, 'exit_code': status,
                       'wall_seconds': time.monotonic()-started}
        if status == 0:
            entry['result'] = benchmark(result)
        manifest['runs'].append(entry)
        path.write_text(json.dumps(manifest, indent=2)+'\n')
        if status:
            raise RuntimeError(f'{name} failed; inspect {output/name}.log')
        print(name, entry['result']['cycles'], flush=True)
    manifest['complete'] = True
    path.write_text(json.dumps(manifest, indent=2)+'\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--parameters', type=Path)
    parser.add_argument('--case', action='append', choices=[row['name'] for row in cases()])
    parser.add_argument('--quanta', type=int, default=16)
    args = parser.parse_args()
    if args.quanta < 1:
        parser.error('quanta must be positive')
    run(args.binary, args.output.resolve(), args.parameters, args.case, args.quanta)
