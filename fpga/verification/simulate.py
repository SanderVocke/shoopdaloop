#!/usr/bin/env python3
"""Run common logical sessions through production Rust and the full AXI RTL pipeline."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
sys.path[:0]=[str(ROOT/'scripts'),str(ROOT/'fpga/axi_memory/sim')]
import rt_backend_testbench as wire
from options import add_options,environment,direct_sources

CONFIGS={'default':(8,16,32,64,32,64,32,32,2,2,32,128,32),
         '8x8':(64,128,256,512,512,512,32,512,8,8,256,1024,32)}
PARAMS=('LOOP_DEPTH','REQUEST_DEPTH','EVENT_DEPTH','PROGRAM_DEPTH','STATE_DEPTH','JOB_DEPTH','MAX_FRAMES','BUFFERS','INPUT_PORTS','OUTPUT_PORTS','VIEWS','SLOTS','CHUNK_FRAMES')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case',type=Path,action='append')
    parser.add_argument('--config',choices=CONFIGS,default='default')
    parser.add_argument('--output',type=Path,default=ROOT/'work/production-rtl')
    parser.add_argument('--read-buffers',type=int,default=4)
    parser.add_argument('--write-buffers',type=int,default=4)
    parser.add_argument('--waves',action='store_true')
    parser.add_argument('--stalls',action='store_true')
    add_options(parser)
    args=parser.parse_args()
    try:env=environment(args)
    except ValueError as error:parser.error(str(error))
    if min(args.read_buffers,args.write_buffers)<1:parser.error('buffer capacities must be positive')
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    subprocess.run(['cargo','build','-p','rt_backend_testbench'],cwd=ROOT,check=True)
    executable=ROOT/'target/debug/rt-backend-testbench'
    scenarios=[wire.load(p) for p in (args.case or [HERE/'scenarios/mono-record.json'])]
    images=wire.run_batch(scenarios,executable=executable,command='compile')
    expected=wire.run_batch(scenarios,executable=executable)
    floating=wire.run_batch(scenarios,executable=executable,numeric='float')
    for i,(s,image,result,f) in enumerate(zip(scenarios,images,expected,floating)):
        wire.dump(s,output/f'{i}.scenario.json')
        wire.dump(image,output/f'{i}.image.json','image')
        wire.dump(result,output/f'{i}.expected.json','result')
        wire.dump(f,output/f'{i}.float.json','result')
    sources=[ROOT/'fpga'/directory/'rtl'/module for directory,names in [
        ('state_processor',['sp_ram.sv','sp_job_fifo.sv','sp_engine.sv','state_processor.sv']),
        ('job_processor',['jp_sample_ram.sv','jp_divider.sv','job_processor.sv','audio_pipeline.sv']),
        ('transition_processor',['tp_ram.sv','tp_priority.sv','transition_processor.sv']),
        ('chunk_memory',['chunk_ram.sv','chunk_memory.sv']),
        ('media_store',['media_ram.sv','media_prefetch_resolver.sv','media_store.sv','memory_delay.sv','media_audio_pipeline.sv']),
        ('loop_pipeline',['loop_pipeline.sv']),
        ('axi_memory',['axi_master.sv','word_axi.sv','chunk_axi.sv','loop_pipeline_axi.sv'])] for module in names]
    top='loop_pipeline_axi_tb';sources.append(ROOT/'fpga/axi_memory/tests/loop_pipeline_axi_tb.sv')
    depths=CONFIGS[args.config]
    parameters=dict(zip(PARAMS,depths));parameters.update(READ_BUFFERS=args.read_buffers,WRITE_BUFFERS=args.write_buffers,
        AXI_DATA_WIDTH=args.axi_data_width,PHYSICAL_BASE=args.physical_base,BURST_MEMORY=int(not args.word_transfers))
    for image in images:
        needs={'LOOP_DEPTH':image['transition']['limits']['loop_slots'],'REQUEST_DEPTH':image['transition']['limits']['pending_requests'],
            'EVENT_DEPTH':image['transition']['limits']['events'],'PROGRAM_DEPTH':len(image['program']['instructions']),
            'STATE_DEPTH':len(image['initial_state']['memory']),'JOB_DEPTH':image['program']['jobs_per_quantum'],
            'MAX_FRAMES':image['program']['quantum'],'BUFFERS':image['buffers'],'INPUT_PORTS':image['input_ports'],
            'OUTPUT_PORTS':image['output_ports'],'VIEWS':len(image['media']['views']),
            'SLOTS':len(image['media']['samples'])//image['media']['chunk_frames']}
        for field,value in needs.items():
            if value>parameters[field]:parser.error(f'{image["name"]}: {field} needs {value}, has {parameters[field]}')
        if image['media']['chunk_frames']!=parameters['CHUNK_FRAMES']:parser.error('scenario chunk size differs from physical configuration')
    runner=get_runner('verilator')
    runner.build(verilog_sources=direct_sources(sources,top,args.memory_model),hdl_toplevel=top,
        includes=[ROOT/'fpga/state_processor/rtl'],parameters=parameters,
        build_args=['--Wall','--timing','--timescale','1ns/1ps'],build_dir=output/'build',always=True,waves=args.waves,log_file=output/'build.log')
    sys.path[:0]=[str(HERE/'tests'),str(ROOT/'fpga/loop_pipeline/tests'),str(ROOT/'fpga/transition_processor/tests'),str(ROOT/'fpga/media_store/tests')]
    result=runner.test(hdl_toplevel=top,test_module='test_production_pipeline',seed=20261112,
        test_dir=output,log_file=output/'simulation.log',waves=args.waves,extra_env={**env,
            'UNIFY_OUTPUT':str(output),'UNIFY_CASES':str(len(scenarios)),'LP_DEPTHS':','.join(map(str,depths[:10])),
            'LP_MEDIA_CAPACITIES':','.join(map(str,depths[10:])),'UNIFY_STALLS':str(int(args.stalls)),
            'LP_OUTPUT':str(output)})
    report=ET.parse(result).getroot()
    if any(list(report.iter(key)) for key in ('failure','error','skipped')) or len(list(report.iter('testcase')))!=1:
        raise SystemExit(f'production/RTL comparison failed: {output}')
    print(f'{len(scenarios)} common production/RTL scenarios passed: {output}')

if __name__=='__main__':main()
