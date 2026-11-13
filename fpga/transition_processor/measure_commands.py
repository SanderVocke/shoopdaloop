#!/usr/bin/env python3
"""Measure identical control workloads against a frozen or current TP tree."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path,default=HERE)
    parser.add_argument('--requests',type=int,choices=(1,11,64,272),default=272)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    source=args.source.resolve();output=args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    executable=Path(os.environ.get('SHOOP_PRODUCTION_ORACLE',ROOT/'target/debug/rt-backend-testbench')).resolve()
    if 'SHOOP_PRODUCTION_ORACLE' not in os.environ:
        subprocess.run(['cargo','build','--locked','-p','rt_backend_testbench'],cwd=ROOT,check=True)
    sys.path[:0]=[str(source/'tests'),str(HERE/'tests'),str(ROOT/'scripts')]
    depths=(64,args.requests,400)
    sources=[source/'rtl'/name for name in ('tp_ram.sv','tp_priority.sv','transition_processor.sv')]
    manifest={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    (output/'provenance.json').write_text(json.dumps(dict(source_manifest=manifest,depths=depths,
        revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        verilator=subprocess.check_output(['verilator','--version'],text=True).strip(),
        rustc=subprocess.check_output(['rustc','--version'],text=True).strip(),
        command=sys.argv,service_profile='dev; native host; JSON transport excluded from service_ns'),indent=2)+'\n')
    runner=get_runner('verilator')
    runner.build(verilog_sources=[*sources,source/'tests/transition_processor_tb.sv'],hdl_toplevel='transition_processor_tb',
        parameters=dict(zip(('LOOP_DEPTH','REQUEST_DEPTH','EVENT_DEPTH'),depths)),
        build_args=['--Wall','--timing','--timescale','1ns/1ps'],build_dir=output/'build',log_file=output/'build.log')
    results=runner.test(hdl_toplevel='transition_processor_tb',test_module='test_command_measurements',seed=20261113,
        test_dir=output,log_file=output/'simulation.log',extra_env=dict(TP_DEPTHS=','.join(map(str,depths)),
        TP_REFERENCE=str(executable),TP_COMMAND_OUTPUT=str(output/'commands.json')))
    report=ET.parse(results).getroot()
    if len(list(report.iter('testcase')))!=1 or any(list(report.iter(tag)) for tag in ('failure','error','skipped')):
        raise SystemExit(f'command measurement failed: {output}')
    print(f'Command measurements: {output}',flush=True)

if __name__=='__main__':
    main()
