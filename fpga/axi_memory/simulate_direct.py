#!/usr/bin/env python3
"""Check the standalone native functional AXI RAM."""
import argparse
import os
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=HERE.parents[1]/'work/direct-memory')
    args = parser.parse_args()
    sys.path[:0] = [str(HERE/'sim'),str(HERE/'tests')]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault('VIRTUAL_ENV',sys.prefix)
    for width in (32,64):
        output = args.output.resolve()/str(width)
        runner = get_runner('verilator')
        runner.build(verilog_sources=[HERE/'tests/axi_direct_memory.sv'],hdl_toplevel='axi_direct_memory',
            parameters={'DATA_WIDTH':width,'WORDS':2048},build_args=['--Wall','--timescale','1ns/1ps'],
            build_dir=output/'build',log_file=output/'build.log',always=True)
        result = runner.test(hdl_toplevel='axi_direct_memory',test_module='test_direct_memory',
            test_dir=output,log_file=output/'simulation.log')
        report = ET.parse(result).getroot()
        if len(list(report.iter('testcase'))) != 1 or any(list(report.iter(t)) for t in ['failure','error','skipped']):
            raise SystemExit(f'Direct memory failed: {output}')
        print(width,'native memory passed',flush=True)


if __name__ == '__main__':
    main()
