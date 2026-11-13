#!/usr/bin/env python3
"""Strict lint, generic inference and ECP5 mapping for complete audio tops."""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
from sources import ROOT, sources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT/'work/audio-synthesis')
    parser.add_argument('--generic-only', action='store_true')
    parser.add_argument('--mapping-only', action='store_true')
    parser.add_argument('--demo', action='store_true')
    parser.add_argument('--quantum', type=int, choices=(16,32,64), help='select one continuous-audio lint/inference variant')
    args = parser.parse_args()
    if args.mapping_only and (args.generic_only or args.quantum is not None):parser.error('--mapping-only selects the default-top mapping alone')
    if args.demo and args.quantum is not None:parser.error('--quantum selects a continuous-audio variant')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    top = 'audio_demo' if args.demo else 'audio_loop_pipeline'
    manifest = sources(args.demo)
    include = ROOT/'fpga/state_processor/rtl'
    subprocess.run(['python3', str(ROOT/'fpga/axi_memory/generate_tops.py'), '--check'], cwd=ROOT, check=True)
    subprocess.run(['python3', str(ROOT/'fpga/audio_io/generate_top.py'), '--check'], cwd=ROOT, check=True)
    subprocess.run(['python3', str(ROOT/'fpga/audio_io/generate_demo.py'), '--check'], cwd=ROOT, check=True)
    variants=[(16,2),(32,3),(64,3)]
    if args.quantum is not None:variants=[variant for variant in variants if variant[0]==args.quantum]
    for q, banks in ([] if args.mapping_only else [(32, 3)] if args.demo else variants):
        name = f'{top}-q{q}-b{banks}'
        print(f'{name}: strict lint and generic synthesis', flush=True)
        quantum = 'Q' if args.demo else 'MAX_FRAMES'
        params = dict(**{quantum: q}, CHUNK_FRAMES=q//2, RX_BANKS=banks, TX_BANKS=banks, P=q//2)
        if args.demo:
            params['CHUNK_FRAMES'] = q
        command = ['verilator', '--lint-only', '--Wall', '--top-module', top, '-I'+str(include)]
        command += [f'-G{k}={v}' for k, v in params.items()]+[str(p) for p in manifest]
        with (output/f'{name}-lint.log').open('w') as log:
            subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)
        script = 'read_verilog -sv -I'+str(include)+' '+' '.join(str(p) for p in manifest)+'\n'
        script += 'chparam '+' '.join(f'-set {k} {v}' for k, v in params.items())+' '+top+'\n'
        script += 'hierarchy -check -top '+top+'\nproc\nopt\nmemory -nomap\nopt\ncheck -assert\n'
        script += 'select -assert-none t:$dlatch t:$adlatch\n'
        script += 'tee -o '+str(output/f'{name}-generic.json')+' stat -json\n'
        path = output/f'{name}.ys'
        path.write_text(script)
        subprocess.run(['yosys', '-Q', '-T', '-q', '-l', str(output/f'{name}-generic.log'), '-s', str(path)], cwd=ROOT, check=True)
    if not args.generic_only:
        print(f'{top}: ECP5 mapping', flush=True)
        path = output/'map.ys'
        netlist = output/'mapped.json'
        path.write_text('read_verilog -sv -I'+str(include)+' '+' '.join(str(p) for p in manifest)+'\n'
                        'synth_ecp5 -top '+top+' -json '+str(netlist)+'\ncheck -assert\nstat\n')
        subprocess.run(['yosys', '-Q', '-T', '-q', '-l', str(output/'mapping.log'), '-s', str(path)], cwd=ROOT, check=True)
        cells = Counter(c['type'] for c in json.loads(netlist.read_text())['modules'][top]['cells'].values())
        (output/'resources.json').write_text(json.dumps(dict(top=top, q=32, channels=16, rx_banks=3, tx_banks=3,
            logical_io_bytes=12288, block_chunks=32, media_slots=1024, cells=dict(sorted(cells.items())), routed_timing=False), indent=2)+'\n')
    print(f'{top}: lint and synthesis passed: {output}')


if __name__ == '__main__':
    main()
