#!/usr/bin/env python3
"""Compare the connected shared-loop RTL pipeline with Rust quantum traces."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
CONFIGS={"default":(8,16,32,64,32,64,32,16,2,2), "small":(3,5,11,16,8,16,16,4,1,1),
         "odd":(7,11,25,32,16,32,16,8,2,2)}
MEDIA_CONFIGS={"default":(32,32,32),"small":(16,8,4),"odd":(7,11,8)}
PARAMS=("LOOP_DEPTH","REQUEST_DEPTH","EVENT_DEPTH","PROGRAM_DEPTH","STATE_DEPTH","JOB_DEPTH","MAX_FRAMES","BUFFERS","INPUT_PORTS","OUTPUT_PORTS")

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',choices=['all',*CONFIGS],default='all')
    parser.add_argument('--random-cases',type=int,default=30)
    parser.add_argument('--output',type=Path,default=ROOT/'work/fpga-loop-pipeline')
    parser.add_argument('--waves',action='store_true')
    parser.add_argument('--case',type=Path)
    parser.add_argument('--media',action='store_true',help='compare actual recording/playback samples and media')
    parser.add_argument('--response-cycles',type=int,default=1)
    args=parser.parse_args()
    if args.random_cases<0:parser.error('random-cases must be nonnegative')
    if args.case and args.config=='all':parser.error('--case requires one --config')
    if args.response_cycles<1:parser.error('response-cycles must be positive')
    subprocess.run(['cargo','build','--locked','-p','rt_fpga_testbench'],cwd=ROOT,check=True)
    sys.path[:0]=[str(HERE/'tests'),str(ROOT/'fpga/transition_processor/tests'),str(ROOT/'scripts'),
                  str(ROOT/'fpga/media_store/tests')]
    if sys.prefix!=sys.base_prefix:os.environ.setdefault('VIRTUAL_ENV',sys.prefix)
    sources=[ROOT/'fpga'/p/'rtl'/s for p,names in [
        ('state_processor',['sp_ram.sv','sp_job_fifo.sv','sp_engine.sv','state_processor.sv']),
        ('job_processor',['jp_sample_ram.sv','jp_divider.sv','job_processor.sv','audio_pipeline.sv']),
        ('transition_processor',['tp_ram.sv','tp_priority.sv','transition_processor.sv']),
        ('media_store',['media_ram.sv','media_store.sv','memory_delay.sv','media_audio_pipeline.sv']),
        ('loop_pipeline',['loop_pipeline.sv'])] for s in names]
    for name,depths in (CONFIGS if args.config=='all' else {args.config:CONFIGS[args.config]}).items():
        output=args.output.resolve()/name;output.mkdir(parents=True,exist_ok=True)
        runner=get_runner('verilator')
        parameters=dict(zip(PARAMS,depths))
        parameters.update(zip(('VIEWS','SLOTS','CHUNK_FRAMES'),MEDIA_CONFIGS[name]))
        parameters['MEMORY_RESPONSE_CYCLES']=args.response_cycles
        runner.build(verilog_sources=[*sources,HERE/'tests/loop_pipeline_tb.sv'],includes=[ROOT/'fpga/state_processor/rtl'],
            hdl_toplevel='loop_pipeline_tb',parameters=parameters,build_args=['--Wall','--timing','--timescale','1ns/1ps'],
            build_dir=output/'build',always=True,waves=args.waves,log_file=output/'build.log')
        results=runner.test(hdl_toplevel='loop_pipeline_tb',test_module='test_media_loop_pipeline' if args.media else 'test_loop_pipeline',seed=20261110,
            testcase='directed_against_rust' if args.case else None,
            test_dir=output,waves=args.waves,log_file=output/'simulation.log',extra_env={
                'LP_DEPTHS':','.join(map(str,depths)),'LP_RANDOM_CASES':str(args.random_cases),
                'LP_OUTPUT':str(output),'LP_REFERENCE':str(ROOT/'target/debug/fpga-reference'),
                'LP_MEDIA_CAPACITIES':','.join(map(str,MEDIA_CONFIGS[name])),
                'LP_CASE':str(args.case.resolve()) if args.case else ''})
        report=ET.parse(results).getroot()
        if len(list(report.iter('testcase')))!=(1 if args.case else 3) or any(list(report.iter(t)) for t in ['failure','error','skipped']):
            raise SystemExit(f'{name}: failed, see {output}')
        print(f'{name}: connected pipeline comparisons passed')

if __name__=='__main__':main()
