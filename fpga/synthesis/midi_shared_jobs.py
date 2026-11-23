#!/usr/bin/env python3
"""Prepare a committed 64-stereo-plus-MIDI shared-SP/JP Vivado experiment."""
import argparse
import hashlib
import json
import math
import re
from pathlib import Path
import shutil
import subprocess

from midi_session import AUDIO, MIDI, LIBRARIES, component_context
from run import ROOT, HERE


def prepare(output, revision, mhz, profile, stage='route', overrides=None, working_tree=False):
    if stage not in ('synth', 'opt', 'place', 'route') or not math.isfinite(mhz) or mhz <= 0:
        raise ValueError('Invalid implementation stage or clock frequency')
    revision = subprocess.check_output(['git', 'rev-parse', revision], cwd=ROOT, text=True).strip()
    config = json.loads(profile.read_text())
    assert (config['loops'], config['audio_channels_per_loop'], config['quantum_frames'], config['chunk_frames']) == (64, 2, 32, 32)
    audio = dict(AUDIO, PROGRAM_DEPTH=2048, JOB_DEPTH=2048, READ_HINT_DEPTH=256)
    parameters = audio | {'MIDI_'+k: v for k, v in MIDI.items() if k != 'DATA_WIDTH'}
    parameters |= dict(MIDI_STATE_SUPPORTED=int(config.get('midi_state_hardware',False)),MIDI_AXI_DATA_WIDTH=64, MIDI_PHYSICAL_BASE=0x30000000,
                       MIDI_PHYSICAL_BYTES_LIMIT=64*1024*1024)
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise SystemExit('Use an empty output directory to preserve evidence')
    hashes = {}
    for library in LIBRARIES + ('midi',):
        command = ['git', 'ls-files', '--cached', '--others', '--exclude-standard', f'fpga/{library}/rtl'] if working_tree else ['git', 'ls-tree', '-r', '--name-only', revision, f'fpga/{library}/rtl']
        paths = subprocess.check_output(command, cwd=ROOT, text=True).splitlines()
        for path in paths:
            if Path(path).suffix not in ('.sv', '.svh'):
                continue
            data = (ROOT/path).read_bytes() if working_tree else subprocess.check_output(['git', 'show', f'{revision}:{path}'], cwd=ROOT)
            target = output/'source'/path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            hashes[path] = hashlib.sha256(data).hexdigest()
    source = (output/'source/fpga/midi/rtl/midi_audio_pipeline_axi.sv').read_text()
    declared=set(re.findall(r'parameter\s+(?:integer|\[[^]]+\])\s+(\w+)\s*=',source))
    parameters={k:v for k,v in parameters.items() if k in declared}
    for key,value in dict(MIDI_BUFFERS=256,MIDI_CURSORS=64,MIDI_SCRATCH_EVENTS=131072).items():
        if key in declared: parameters[key]=value
    overrides = overrides or {}
    if set(overrides)-declared:
        raise ValueError(f'Unknown top parameters: {sorted(set(overrides)-declared)}')
    parameters.update(overrides)
    for used, capacity in [('compiled_instructions', 'PROGRAM_DEPTH'), ('jobs_per_quantum', 'JOB_DEPTH'),
                           ('audio_buffers', 'BUFFERS'), ('state_values', 'STATE_DEPTH')]:
        if config[used] > parameters[capacity]:
            raise ValueError(f'{used} exceeds {capacity}')
    text = '`include "state_processor_defs.svh"\n'
    text += component_context(source, 'midi_audio_pipeline_axi', 'pipeline_context', dict(
        run_quantum="32'd32", run_chunk_frames="32'd32", arena_base="64'd0",
        midi_arena_base="64'd0", midi_memory_arena_bytes="64'd67108864"))
    args = ','.join(f'.{k}({v})' for k, v in parameters.items())
    text += f'module session_context(input wire clk);\npipeline_context #({args}) pipeline (.clk(clk));\nendmodule\n'
    (output/'context.sv').write_text(text)
    (output/'context.xdc').write_text(
        f'create_clock -name core -period {1000/mhz:.6f} [get_ports clk]\n'
        'set_clock_uncertainty 0.200 [get_clocks core]\n'
        'set_property HD.CLK_SRC BUFGCTRL_X0Y0 [get_ports clk]\n')
    (output/'run.tcl').write_text(f'set stop_after {stage}\n'+(HERE/'midi_shared_jobs.tcl').read_text())
    shutil.copyfile(HERE/'run_midi_container.sh', output/'run-container.sh')
    shutil.copyfile(profile, output/'compiled-profile.json')
    (output/'provenance.json').write_text(json.dumps(dict(
        revision=revision, working_tree=working_tree, stage=stage, parameter_overrides=overrides,
        clock_mhz=mhz, part='xc7z020clg484-1', vivado='2021.1',
        uncertainty_ns=0.2, loops=64, audio_channels_per_loop=2, sample_rate=48000,
        storage_seconds=30, chunk_frames=32, quantum_frames=32, parameters=parameters,
        compiled_instructions=config['compiled_instructions'], jobs_per_quantum=config['jobs_per_quantum'],
        build='connected shared SP/JP pipeline, registered OOC public boundaries',
        memory='two AXI3 DDR ports; DDR controller arbitration and PS7 outside this fabric build',
        threads=2), indent=2)+'\n')
    (output/'source-manifest.json').write_text(json.dumps(hashes, indent=2)+'\n')
    artifacts = ['context.sv', 'context.xdc', 'run.tcl', 'run-container.sh', 'compiled-profile.json']
    (output/'artifact-manifest.json').write_text(json.dumps({p: hashlib.sha256((output/p).read_bytes()).hexdigest() for p in artifacts}, indent=2)+'\n')
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision', default='HEAD')
    parser.add_argument('--mhz', required=True, type=float)
    parser.add_argument('--stage', default='route', choices=('synth', 'opt', 'place', 'route'))
    parser.add_argument('--working-tree', action='store_true')
    parser.add_argument('--param', action='append', default=[], metavar='NAME=INTEGER')
    parser.add_argument('--profile', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    overrides = {}
    for item in args.param:
        try:
            name, value = item.split('=', 1)
            value = int(value, 0)
        except ValueError:
            parser.error(f'Expected NAME=INTEGER: {item}')
        if name in overrides:
            parser.error(f'Duplicate parameter: {name}')
        overrides[name] = value
    prepare(args.output.resolve(), args.revision, args.mhz, args.profile,
            args.stage, overrides, args.working_tree)
