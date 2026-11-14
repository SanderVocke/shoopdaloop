#!/usr/bin/env python3
"""Measure the complete loop core in a synchronous registered fabric context."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT/'fpga/axi_memory'))
from generate_tops import header


def context(source, module_name='loop_pipeline_axi', clocks=('clk',), audio_ports=()):
    match, ports = header(source)
    parameters = match[2]
    import re
    names = re.findall(r'parameter\s+(?:integer|\[[^]]+\])\s+(\w+)\s*=', parameters)
    lines = ['`include "state_processor_defs.svh"',
             'module registered_core_context #(', parameters,
             ') ('+', '.join('input wire '+name for name in clocks)+');', 'reg [7:0] startup = 0;',
             "always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;",
             "reg reset = 1;", 'always @(posedge clk) reset <= !(&startup);']
    for ordinal, (declaration, name) in enumerate(ports):
        if name in clocks or name == 'reset':
            continue
        bounds = re.search(r'\[([^:]+):0\]', declaration)
        bits = bounds[0] if bounds else ''
        width = f'({bounds[1]})+1' if bounds else '1'
        clock = 'audio_clk' if name in audio_ports else 'clk'
        if declaration.startswith('input'):
            lines += [f'localparam integer PORT_BITS_{name} = {width};',
                      f'localparam integer SOURCE_BITS_{name} = PORT_BITS_{name} < 32 ? 32 : PORT_BITS_{name};',
                      f'(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_{name}-1:0] source_{name} = SOURCE_BITS_{name}\'({ordinal+1});',
                      f'always @(posedge {clock}) source_{name} <= {{source_{name}[SOURCE_BITS_{name}-2:0],source_{name}[SOURCE_BITS_{name}-1]^source_{name}[SOURCE_BITS_{name}-2]}};',
                      f"wire {bits} {name} = PORT_BITS_{name}'(source_{name});"]
        else:
            lines += [f'wire {bits} {name};',
                      f'(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg {bits} unused_capture_{name};',
                      f'always @(posedge {clock}) unused_capture_{name} <= {name};']
    lines += [f'(* KEEP_HIERARCHY = "yes" *) {module_name} #(',
              ',\n'.join(f'.{name}({name})' for name in names), ') core_under_test (',
              ',\n'.join(f'.{name}({name})' for _, name in ports), ');', 'endmodule']
    return '\n'.join(lines)+'\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--vivado', default='vivado')
    parser.add_argument('--output', type=Path, default=ROOT/'work/vivado-current')
    parser.add_argument('--threads', type=int, default=4)
    parser.add_argument('--audio', action='store_true', help='measure the autonomous continuous wrapper, including both clock domains')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error('output must be empty to preserve run provenance')
    if args.threads < 1:
        parser.error('threads must be positive')
    sources = []
    directories=['state_processor','job_processor','transition_processor',
                 'chunk_memory','media_store','loop_pipeline','axi_memory']
    if args.audio:
        directories.append('audio_io')
    for directory in directories:
        sources.extend(sorted((ROOT/'fpga'/directory/'rtl').glob('*.sv')))
    sources.extend(sorted((ROOT/'fpga/state_processor/rtl').glob('*.svh')))
    hashes = {}
    for path in sources:
        relative = path.relative_to(ROOT)
        destination = output/'source'/relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, destination)
        hashes[str(relative)] = hashlib.sha256(path.read_bytes()).hexdigest()
    source=ROOT/('fpga/audio_io/rtl/audio_loop_pipeline.sv' if args.audio else 'fpga/axi_memory/rtl/loop_pipeline_axi.sv')
    (output/'registered_core_context.sv').write_text(context(source.read_text(),
        'audio_loop_pipeline' if args.audio else 'loop_pipeline_axi',
        ('clk','audio_clk') if args.audio else ('clk',),
        ('frame_tick','frame_prepare','rx_samples','tx_samples','tx_valid','audio_frame','session_frame') if args.audio else ()))
    for name in ('run.tcl','core.xdc'):
        shutil.copyfile(HERE/name, output/name)
    if args.audio:
        script=(output/'run.tcl').read_text().replace('loop_pipeline axi_memory}', 'loop_pipeline axi_memory audio_io}')
        script += '\nreport_cdc -details -file post_route_cdc.rpt\nreport_clock_interaction -file post_route_clock_interaction.rpt\nreport_clocks -file post_route_clocks.rpt\n'
        (output/'run.tcl').write_text(script)
        with (output/'core.xdc').open('a') as constraints:
            constraints.write('\ncreate_clock -name audio_clk -period 325.520833 [get_ports audio_clk]\n'
                              'set_clock_uncertainty 0.200 [get_clocks audio_clk]\n'
                              'set_clock_groups -asynchronous -group [get_clocks core_clk] -group [get_clocks audio_clk]\n')
    (output/'source-manifest.json').write_text(json.dumps(hashes, indent=2)+'\n')
    (output/'provenance.json').write_text(json.dumps(dict(
        revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        working_tree_dirty=bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True)),
        command=sys.argv, part='xc7z020clg400-1', clock_mhz=100,
        top='audio_loop_pipeline' if args.audio else 'loop_pipeline_axi',
        audio_clock_mhz=3.072 if args.audio else None,
    ), indent=2)+'\n')
    command = [args.vivado,'-mode','batch','-nojournal','-log','vivado.log',
               '-source','run.tcl','-tclargs',str(args.threads)]
    # Reap Vivado's detached synthesis workers in containers whose PID 1 does not.
    if sys.platform == 'linux':
        import ctypes
        if ctypes.CDLL(None).prctl(36, 1, 0, 0, 0) != 0:
            raise RuntimeError('cannot enable child subreaper')
    with (output/'console.log').open('w') as log:
        process = subprocess.Popen(command,cwd=output,stdout=log,stderr=subprocess.STDOUT)
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
    result = json.loads((output/'timing.json').read_text())
    print(json.dumps(result,indent=2))
    if not result['met_100mhz']:
        raise SystemExit('Routed design does not meet 100 MHz; see timing reports')


if __name__ == '__main__':
    main()
