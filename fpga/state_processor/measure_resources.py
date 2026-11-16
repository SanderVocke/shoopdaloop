#!/usr/bin/env python3
"""Preserve sources and synthesize a standalone SP into Xilinx 7-series cells."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rtl', type=Path, default=Path(__file__).parent / 'rtl')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--yosys', default='yosys')
    parser.add_argument('--program-depth', type=int, default=256)
    parser.add_argument('--state-depth', type=int, default=256)
    parser.add_argument('--job-depth', type=int, default=256)
    parser.add_argument('--loop-depth', type=int)
    args = parser.parse_args()
    rtl = args.rtl.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error('output must be empty to preserve measurement provenance')
    parameters = dict(PROGRAM_DEPTH=args.program_depth, STATE_DEPTH=args.state_depth,
                      JOB_DEPTH=args.job_depth)
    if args.loop_depth is not None:
        parameters['LOOP_DEPTH'] = args.loop_depth
    if any(value < 1 for value in parameters.values()):
        parser.error('capacities must be positive')
    source = output / 'source'
    source.mkdir()
    hashes = {}
    for path in sorted(rtl.iterdir()):
        if path.suffix in ('.sv', '.svh'):
            shutil.copyfile(path, source / path.name)
            hashes[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    def quote(path):
        return json.dumps(str(path))
    commands = [
        'read_verilog -sv -I' + quote(source) + ' ' +
        ' '.join(quote(path) for path in sorted(source.glob('*.sv'))),
        'chparam ' + ' '.join(f'-set {name} {value}' for name, value in parameters.items()) + ' state_processor',
        'synth_xilinx -family xc7 -flatten -noiopad -noclkbuf -top state_processor',
        'check -assert', 'stat -tech xilinx', 'write_json ' + quote(output / 'mapped.json'),
    ]
    script = output / 'synth.ys'
    script.write_text(';\n'.join(commands) + '\n')
    provenance = dict(
        tool=subprocess.check_output([args.yosys, '-V'], text=True).strip(),
        target='Xilinx 7-series cell mapping (xc7)', parameters=parameters, sources=hashes,
        synthesis=commands[2], physical_implementation=False,
    )
    (output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    with (output / 'console.log').open('w') as log:
        subprocess.run([args.yosys, '-Q', '-T', '-l', str(output / 'yosys.log'), '-s', str(script)],
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    netlist = json.loads((output / 'mapped.json').read_text())
    cells = {}
    for cell in netlist['modules']['state_processor']['cells'].values():
        cells[cell['type']] = cells.get(cell['type'], 0) + 1
    result = dict(
        cells=cells,
        lut=sum(count for kind, count in cells.items() if kind.startswith('LUT')),
        ff=sum(count for kind, count in cells.items() if kind.startswith('FD')),
        bram18=cells.get('RAMB18E1', 0), bram36=cells.get('RAMB36E1', 0),
        dsp=cells.get('DSP48E1', 0),
    )
    (output / 'resources.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
