#!/usr/bin/env python3
"""Run common logical sessions through production Rust and the full AXI RTL pipeline."""
import argparse
import json
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
from rt_backend_testbench.accuracy import compare as compare_float
from rt_backend_testbench.cases import directed,seeded,benchmark64,long_sessions,scale_workload
from options import add_options,environment,direct_sources

CONFIGS={'default':(8,16,32,64,32,64,32,32,4,4,32,128,32),
         'odd':(7,11,25,128,128,128,32,61,4,4,7,11,8),
         'wide':(8,16,32,128,64,128,128,32,4,4,32,128,32),
         '8x8':(64,128,256,512,256,512,32,256,8,8,64,1024,32)}
PARAMS=('LOOP_DEPTH','REQUEST_DEPTH','EVENT_DEPTH','PROGRAM_DEPTH','STATE_DEPTH','JOB_DEPTH','MAX_FRAMES','BUFFERS','INPUT_PORTS','OUTPUT_PORTS','VIEWS','SLOTS','CHUNK_FRAMES')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case',type=Path,action='append')
    parser.add_argument('--directed',action='store_true')
    parser.add_argument('--random-cases',type=int,default=0)
    parser.add_argument('--benchmark64',action='store_true')
    parser.add_argument('--scale-workload',choices=('original','streaming','mixed'),default='original')
    parser.add_argument('--long-sessions',action='store_true')
    parser.add_argument('--config',choices=CONFIGS,default='default')
    parser.add_argument('--output',type=Path,default=ROOT/'work/production-rtl')
    parser.add_argument('--read-buffers',type=int,default=4)
    parser.add_argument('--write-buffers',type=int,default=4)
    parser.add_argument('--waves',action='store_true')
    parser.add_argument('--stalls',action='store_true')
    parser.add_argument('--faults',action='store_true')
    add_options(parser)
    args=parser.parse_args()
    try:env=environment(args)
    except ValueError as error:parser.error(str(error))
    if min(args.read_buffers,args.write_buffers)<1:parser.error('buffer capacities must be positive')
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    subprocess.run(['cargo','build','-p','rt_backend_testbench'],cwd=ROOT,check=True)
    executable=ROOT/'target/debug/rt-backend-testbench'
    scenarios=[wire.load(p) for p in (args.case or ([] if args.benchmark64 else [HERE/'scenarios/mono-record.json']))]
    if args.directed:scenarios.extend(directed())
    scenarios.extend(seeded(args.random_cases))
    if args.benchmark64:scenarios.append(scale_workload(args.scale_workload))
    if args.long_sessions:scenarios.extend(long_sessions())
    images=wire.run_batch(scenarios,executable=executable,command='compile')
    expected=wire.run_batch(scenarios,executable=executable)
    floating=wire.run_batch(scenarios,executable=executable,numeric='float')
    audits=wire.run_batch(scenarios,executable=executable,command='audit')
    accuracy=[compare_float(s,image,q,f,audit) for s,image,q,f,audit in zip(scenarios,images,expected,floating,audits)]
    (output/'float-accuracy.json').write_text(json.dumps(accuracy,indent=2)+'\n')
    for i,(s,image,result,f) in enumerate(zip(scenarios,images,expected,floating)):
        wire.dump(s,output/f'{i}.scenario.json')
        wire.dump(image,output/f'{i}.image.json','image')
        wire.dump(result,output/f'{i}.expected.json','result')
        wire.dump(f,output/f'{i}.float.json','result')
        wire.dump(audits[i],output/f'{i}.audit.json','audit')
    sources=[ROOT/'fpga'/directory/'rtl'/module for directory,names in [
        ('state_processor',['sp_ram.sv','sp_job_fifo.sv','sp_engine.sv','state_processor.sv']),
        ('job_processor',['jp_sample_ram.sv','jp_divider.sv','job_processor.sv','audio_pipeline.sv']),
        ('transition_processor',['tp_ram.sv','tp_priority.sv','transition_processor.sv']),
        ('chunk_memory',['chunk_ram.sv','chunk_memory.sv']),
        ('media_store',['media_ram.sv','metadata_cache.sv','media_metadata.sv','media_prefetch_resolver.sv','media_store.sv','memory_delay.sv','media_audio_pipeline.sv']),
        ('loop_pipeline',['loop_pipeline.sv']),
        ('axi_memory',['axi_master.sv','axi_burst_mux.sv','shared_axi.sv','chunk_dma.sv','word_dma.sv','word_axi.sv','chunk_axi.sv','loop_pipeline_axi.sv'])] for module in names]
    top='loop_pipeline_axi_tb';sources.append(ROOT/'fpga/axi_memory/tests/loop_pipeline_axi_tb.sv')
    depths=list(CONFIGS[args.config])
    if args.benchmark64 and args.scale_workload=='streaming':depths[11]=4096
    if args.benchmark64 and args.scale_workload=='mixed':depths[11]=2048
    depths[12]=scenarios[0]['chunk_frames']
    parameters=dict(zip(PARAMS,depths));parameters.update(READ_BUFFERS=args.read_buffers,WRITE_BUFFERS=args.write_buffers,
        AXI_DATA_WIDTH=args.axi_data_width,PHYSICAL_BASE=args.physical_base,BURST_MEMORY=int(not args.word_transfers))
    parameters.update(TABLE_LINES=args.metadata_lines,REF_LINES=args.reference_lines,STACK_LINES=args.stack_lines)
    if args.metadata_pages is not None:parameters['TABLE_PAGES']=args.metadata_pages
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
            'UNIFY_FAULTS':str(int(args.faults)),'UNIFY_OUTPUT':str(output),'UNIFY_CASES':str(len(scenarios)),'LP_DEPTHS':','.join(map(str,depths[:10])),
            'LP_MEDIA_CAPACITIES':','.join(map(str,depths[10:])),'UNIFY_STALLS':str(int(args.stalls)),
            'LP_OUTPUT':str(output)})
    report=ET.parse(result).getroot()
    if any(list(report.iter(key)) for key in ('failure','error','skipped')) or len(list(report.iter('testcase')))!=(2 if args.faults else 1):
        raise SystemExit(f'production/RTL comparison failed: {output}')
    print(f'{len(scenarios)} common production/RTL scenarios passed: {output}')

if __name__=='__main__':main()
