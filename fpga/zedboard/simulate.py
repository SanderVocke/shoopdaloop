#!/usr/bin/env python3
"""Exercise the deployed runner and its counters with the shared AXI models."""
import argparse
import json
import os
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path[:0] = [str(ROOT/'fpga/audio_io'), str(ROOT/'fpga/axi_memory/sim')]
from sources import sources
from options import add_options, environment
from generate import axi


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workload', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT/'work/zedboard/simulation')
    parser.add_argument('--quanta', type=int, default=12)
    parser.add_argument('--control-tests', action='store_true')
    parser.add_argument('--read-buffers', type=int, default=4)
    parser.add_argument('--write-buffers', type=int, default=4)
    add_options(parser)
    parser.set_defaults(axi_data_width=64, block_chunks=32)
    args = parser.parse_args()
    workload = args.workload.resolve()
    config = json.loads((workload/'config.json').read_text())
    if args.quanta <= config['warmup_quanta']:
        parser.error('quanta must exceed workload warmup_quanta')
    if args.axi_data_width != 64 or min(args.read_buffers, args.write_buffers) < 1:
        parser.error('benchmark requires 64-bit AXI and positive buffer counts')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    env = environment(args)
    parameters = dict(config['parameters'], PHYSICAL_BASE=args.physical_base,
                      READ_BUFFERS=args.read_buffers, WRITE_BUFFERS=args.write_buffers, TABLE_LINES=args.metadata_lines,
                      REF_LINES=args.reference_lines, STACK_LINES=args.stack_lines,
                      BOOT_FILE='"'+str(workload/'boot.hex')+'"', EVENT_FILE='"'+str(workload/'events.hex')+'"')
    parameters.update(RETIRED_TABLES=args.retired_tables,RECLAIM_BLOCKS=args.reclaim_blocks,
        RECLAIM_CYCLES=args.reclaim_cycles,RECLAIM_GUARD=args.reclaim_guard)
    if args.metadata_pages is not None:parameters['TABLE_PAGES']=args.metadata_pages
    runtime_config={**config,'parameters':parameters}
    runtime_path=output/'config.json'
    runtime_path.write_text(json.dumps(runtime_config,indent=2)+'\n')
    manifest = sources(True)
    top = HERE/'rtl/timing_bench.sv'
    if args.memory_model == 'instant':
        direct = output/'timing_bench.sv'
        direct.write_text(axi.direct_top(top.read_text()))
        manifest += [direct, ROOT/'fpga/axi_memory/tests/axi_direct_memory.sv']
    else:
        manifest += [top]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault('VIRTUAL_ENV', sys.prefix)
    runner = get_runner('verilator')
    runner.build(verilog_sources=manifest, hdl_toplevel='timing_bench', parameters=parameters,
                 includes=[ROOT/'fpga/state_processor/rtl', HERE/'rtl'],
                 build_args=['--Wall', '--timescale', '1ns/1ps'],
                 build_dir=output/'build', log_file=output/'build.log', always=True)
    sys.path.insert(0, str(HERE/'tests'))
    result = runner.test(hdl_toplevel='timing_bench', test_module='test_timing_bench', test_dir=output,
                         log_file=output/'simulation.log', extra_env={**env,
                             'BENCH_CONFIG': str(runtime_path), 'BENCH_QUANTA': str(args.quanta),
                             'BENCH_CONTROLS': str(int(args.control_tests)), 'BENCH_OUTPUT': str(output)})
    report = ET.parse(result).getroot()
    if len(list(report.iter('testcase'))) != 1 or any(list(report.iter(k)) for k in ('failure', 'error', 'skipped')):
        raise SystemExit(f'benchmark runner failed: {output}')
    print(f'benchmark runner passed: {output}')


if __name__ == '__main__':
    main()
