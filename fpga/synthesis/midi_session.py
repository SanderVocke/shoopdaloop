#!/usr/bin/env python3
"""Prepare reproducible before/after Vivado builds for a 64-stereo-loop session."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

from run import ROOT, HERE
sys.path.insert(0, str(ROOT / 'fpga/axi_memory'))
from generate_tops import header

BEFORE = '147c2457f46ec921f5dad5c99b5f673eda13ef99'
AFTER = 'f24cc89b57e2c82d4d342e43ff686a6d389eff90'
LIBRARIES = ('state_processor', 'job_processor', 'chunk_memory', 'media_store', 'axi_memory')
AUDIO = dict(EXTERNAL_IO=1, LIVE_LOADING=1, EXPORT_SYNC_EVENTS=1,
             PROGRAM_DEPTH=1024, STATE_DEPTH=256, JOB_DEPTH=1024, MAX_FRAMES=32,
             BUFFERS=512, INPUT_PORTS=2, OUTPUT_PORTS=2, REUSE_VIEWS=1,
             VIEWS=512, SLOTS=5763072, CHUNK_FRAMES=32, BLOCK_CHUNKS=32,
             RETIRED_TABLES=256, TABLE_LINES=128, REF_LINES=4, STACK_LINES=2,
             READ_BUFFERS=4, WRITE_BUFFERS=4, BURST_MEMORY=1, AXI_DATA_WIDTH=64,
             PHYSICAL_BASE=0x01000000, PHYSICAL_BYTES_LIMIT=1024**3)
MIDI = dict(DATA_WIDTH=64, LOOPS=64, INPUTS=1, DESTINATIONS=8,
            BLOCK_EVENTS=256, BLOCKS=2432, MAX_EVENTS=8192,
            VIEWS=256, SNAPSHOTS=384, RETIREMENTS=128,
            INPUT_EVENTS=256, MAINTENANCE=64)


def snapshot(revision, path):
    return subprocess.check_output(['git', 'show', f'{revision}:{path}'], cwd=ROOT)


def component_context(source, top, name, constants):
    match, ports = header(source)
    parameters = match[2]
    names = []
    for declaration in parameters.split(','):
        parameter = re.fullmatch(
            r'\s*(?:parameter\s+(?:integer|\[[^]]+\])\s+)?(\w+)\s*=.*',
            declaration, re.S)
        if parameter is None:
            raise ValueError(f'Cannot parse parameter: {declaration}')
        names.append(parameter[1])
    lines = [f'module {name} #(', parameters, ') (input wire clk);',
             'reg [7:0] startup = 0;',
             "always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;",
             'reg reset = 1;', 'always @(posedge clk) reset <= !(&startup);']
    for ordinal, (declaration, port) in enumerate(ports):
        if port in ('clk', 'reset'):
            continue
        bounds = re.search(r'\[([^:]+):0\]', declaration)
        bits = bounds[0] if bounds else ''
        width = f'({bounds[1]})+1' if bounds else '1'
        if declaration.startswith('input'):
            if port in constants:
                lines.append(f'wire {bits} {port} = {constants[port]};')
                continue
            lines += [f'localparam integer PORT_BITS_{port} = {width};',
                      f'localparam integer SOURCE_BITS_{port} = PORT_BITS_{port} < 32 ? 32 : PORT_BITS_{port};',
                      f'(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_{port}-1:0] source_{port} = SOURCE_BITS_{port}\'({ordinal+1});',
                      f'always @(posedge clk) source_{port} <= {{source_{port}[SOURCE_BITS_{port}-2:0],source_{port}[SOURCE_BITS_{port}-1]^source_{port}[SOURCE_BITS_{port}-2]}};',
                      f"wire {bits} {port} = PORT_BITS_{port}'(source_{port});"]
        else:
            lines += [f'wire {bits} {port};',
                      f'(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg {bits} capture_{port};',
                      f'always @(posedge clk) capture_{port} <= {port};']
    lines += [f'(* KEEP_HIERARCHY = "yes" *) {top} #(',
              ',\n'.join(f'.{p}({p})' for p in names), ') core_under_test (',
              ',\n'.join(f'.{p}({p})' for _, p in ports), ');', 'endmodule']
    return '\n'.join(lines)+'\n'


def prepare(output, version, mhz):
    revision = BEFORE if version == 'before' else AFTER
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise SystemExit('Output must be empty to preserve provenance')
    hashes = {}
    libraries = LIBRARIES + (('midi',) if version == 'after' else ())
    for library in libraries:
        paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', revision,
                                         f'fpga/{library}/rtl'], cwd=ROOT, text=True).splitlines()
        for path in paths:
            if Path(path).suffix not in ('.sv', '.svh'):
                continue
            data = snapshot(revision, path)
            dest = output / 'source' / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
            hashes[path] = hashlib.sha256(data).hexdigest()
    original_hashes = hashes.copy()
    patches = []
    patch_names = ['vivado_2021_audio.patch']
    if version == 'after':
        patch_names.append('vivado_2021_midi.patch')
    for patch_name in patch_names:
        patch = HERE/patch_name
        subprocess.run(['patch', '--batch', '-p1', '-i', str(patch)],
                       cwd=output/'source', check=True)
        shutil.copyfile(patch, output/patch.name)
        patches.append(dict(file=patch.name,
                            sha256=hashlib.sha256(patch.read_bytes()).hexdigest(),
                            reason='Syntax compatibility with Vivado 2021.1'))
    for path in hashes:
        hashes[path] = hashlib.sha256((output/'source'/path).read_bytes()).hexdigest()
    source = (output/'source/fpga/axi_memory/rtl/media_audio_pipeline_axi.sv').read_text()
    text = '`include "state_processor_defs.svh"\n'
    text += component_context(source, 'media_audio_pipeline_axi', 'audio_context', dict(
        run_quantum="32'd32", run_chunk_frames="32'd32", arena_base="64'd0"))
    components = [('audio_context', 'audio', AUDIO)]
    if version == 'after':
        source = (output/'source/fpga/midi/rtl/midi_runtime_axi.sv').read_text()
        text += component_context(source, 'midi_runtime_axi', 'midi_context', dict(
            quantum_frames="32'd32", arena_base="64'd0", physical_base="32'h30000000",
            memory_arena_bytes="64'd67108864"))
        components.append(('midi_context', 'midi', MIDI))
    text += 'module session_context(input wire clk);\n'
    for name, instance, params in components:
        text += f'(* KEEP_HIERARCHY = "yes" *) {name} #(\n'
        text += ',\n'.join(f'.{p}({v})' for p, v in params.items())
        text += f') {instance} (.clk(clk));\n'
    text += 'endmodule\n'
    (output/'context.sv').write_text(text)
    (output/'context.xdc').write_text(
        f'create_clock -name core_clk -period {1000/mhz:.9f} [get_ports clk]\n'
        'set_clock_uncertainty 0.200 [get_clocks core_clk]\n'
        'set_property HD.CLK_SRC BUFGCTRL_X0Y0 [get_ports clk]\n')
    (output/'run.tcl').write_bytes((HERE/'midi_session.tcl').read_bytes())
    provenance = dict(revision=revision, version=version, clock_mhz=mhz,
                      part='xc7z020clg484-1', uncertainty_ns=0.2,
                      sample_rate=48000, loops=64, audio_channels_per_loop=2,
                      storage_seconds=30, chunk_frames=32, quantum_frames=32,
                      audio=AUDIO, midi=MIDI if version == 'after' else None,
                      libraries=libraries, build='out-of-context registered component composition',
                      memory='two independent AXI3 HP interfaces into external shared DDR; PS7 excluded',
                      threads=2, strategy='default placement and routing; post-place phys_opt',
                      compatibility_patches=patches)
    (output/'provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')
    (output/'source-manifest.json').write_text(json.dumps(hashes, indent=2)+'\n')
    (output/'original-source-manifest.json').write_text(json.dumps(original_hashes, indent=2)+'\n')
    (output/'artifact-manifest.json').write_text(json.dumps({
        p: hashlib.sha256((output/p).read_bytes()).hexdigest()
        for p in ('context.sv', 'context.xdc', 'run.tcl')}, indent=2)+'\n')
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('version', choices=('before', 'after'))
    parser.add_argument('--mhz', required=True, type=int, choices=(100, 150))
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    prepare(args.output.resolve(), args.version, args.mhz)
