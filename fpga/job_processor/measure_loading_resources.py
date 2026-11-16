#!/usr/bin/env python3
"""Compare one-program and live-loading SP/JP cell mappings at identical capacities."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--yosys', default='yosys')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error('output must be empty to preserve measurement provenance')
    source = output / 'source'
    sources = []
    hashes = {}
    for component in ('state_processor', 'job_processor'):
        destination = source
        destination.mkdir(parents=True, exist_ok=True)
        for path in sorted((root / 'fpga' / component / 'rtl').iterdir()):
            if path.suffix in ('.sv', '.svh'):
                copied = destination / path.name
                if copied.exists():
                    parser.error('source file names must be unique')
                shutil.copyfile(path, copied)
                hashes[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
                if path.suffix == '.sv':
                    sources.append(copied)
    tool = subprocess.check_output([args.yosys, '-V'], text=True).strip()
    results = {}
    for live, name in ((0, 'fixed'), (1, 'live')):
        directory = output / name
        directory.mkdir()
        parameters = dict(PROGRAM_DEPTH=64, PROGRAM_REGIONS=1+live, LIVE_LOADING=live,
                          STATE_DEPTH=32, LOOP_DEPTH=32, JOB_DEPTH=64, MAX_FRAMES=16,
                          BUFFERS=16, INPUT_PORTS=1, OUTPUT_PORTS=2)
        commands = [
            'read_verilog -sv ' +
            ' '.join(json.dumps(str(path)) for path in sources),
            'chparam ' + ' '.join(f'-set {key} {value}' for key, value in parameters.items()) + ' audio_pipeline',
            'synth_xilinx -family xc7 -flatten -noiopad -noclkbuf -top audio_pipeline',
            'check -assert', 'stat -tech xilinx',
            'write_json ' + json.dumps(str(directory / 'mapped.json')),
        ]
        script = directory / 'synth.ys'
        script.write_text(';\n'.join(commands) + '\n')
        provenance = dict(tool=tool, parameters=parameters, sources=hashes,
                          commands=commands, physical_implementation=False)
        (directory / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
        with (directory / 'console.log').open('w') as log:
            subprocess.run([args.yosys, '-Q', '-T', '-s', str(script)],
                           stdout=log, stderr=subprocess.STDOUT, check=True)
        netlist = json.loads((directory / 'mapped.json').read_text())
        cells = {}
        for cell in netlist['modules']['audio_pipeline']['cells'].values():
            cells[cell['type']] = cells.get(cell['type'], 0) + 1
        result = dict(lut=sum(v for k, v in cells.items() if k.startswith('LUT')),
                      ff=sum(v for k, v in cells.items() if k.startswith('FD')),
                      bram18=cells.get('RAMB18E1', 0), bram36=cells.get('RAMB36E1', 0),
                      dsp=cells.get('DSP48E1', 0), cells=cells)
        (directory / 'resources.json').write_text(json.dumps(result, indent=2) + '\n')
        results[name] = result
        print(name, json.dumps(result), flush=True)
    (output / 'comparison.json').write_text(json.dumps(results, indent=2) + '\n')


if __name__ == '__main__':
    main()
