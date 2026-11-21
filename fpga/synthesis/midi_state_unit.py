#!/usr/bin/env python3
"""Prepare a reproducible expression-unit area experiment with live public ports."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from midi_session import component_context
from run import ROOT, HERE


def prepare(output, revision, mhz, stage, working_tree=False):
    revision = subprocess.check_output(['git', 'rev-parse', revision], cwd=ROOT, text=True).strip()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise SystemExit('Use an empty output directory')
    path = 'fpga/midi/rtl/midi_expression_state.sv'
    data = (ROOT/path).read_bytes() if working_tree else subprocess.check_output(['git', 'show', f'{revision}:{path}'], cwd=ROOT)
    target = output/'source'/path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    text = component_context(data.decode(), 'midi_expression_state', 'expression_context', {'arena_base': "64'd0"})
    params = dict(LOOPS=64, INPUTS=1, DESTINATIONS=8, VIEWS=256)
    text += 'module session_context(input wire clk);\nexpression_context #('
    text += ','.join(f'.{k}({v})' for k, v in params.items())
    text += ') expression (.clk(clk));\nendmodule\n'
    (output/'context.sv').write_text(text)
    (output/'context.xdc').write_text(f'create_clock -name core -period {1000/mhz:.6f} [get_ports clk]\nset_clock_uncertainty 0.200 [get_clocks core]\nset_property HD.CLK_SRC BUFGCTRL_X0Y0 [get_ports clk]\n')
    shutil.copyfile(HERE/'midi_state_unit.tcl', output/'run.tcl')
    shutil.copyfile(HERE/'run_midi_container.sh', output/'run-container.sh')
    (output/'source-manifest.json').write_text(json.dumps({path: hashlib.sha256(data).hexdigest()}, indent=2)+'\n')
    (output/'provenance.json').write_text(json.dumps(dict(revision=revision, working_tree=working_tree, part='xc7z020clg484-1', vivado='2021.1', clock_mhz=mhz, threads=1, stage=stage, parameters=params, scope='isolated expression functional unit; registered live public boundaries; no pruning of commands or outputs'), indent=2)+'\n')
    tcl = (output/'run.tcl').read_text().replace('@STAGE@', stage)
    (output/'run.tcl').write_text(tcl)
    (output/'artifact-manifest.json').write_text(json.dumps({p: hashlib.sha256((output/p).read_bytes()).hexdigest() for p in ['context.sv','context.xdc','run.tcl','run-container.sh']}, indent=2)+'\n')
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision', default='HEAD')
    parser.add_argument('--working-tree', action='store_true', help='Snapshot the working source and record its SHA-256 for an uncommitted area experiment')
    parser.add_argument('--mhz', default=100, type=int)
    parser.add_argument('--stage', default='synth', choices=('synth','route'))
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.output.resolve(), args.revision, args.mhz, args.stage, args.working_tree)
