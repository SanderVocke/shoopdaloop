#!/usr/bin/env python3
"""Check restart histories and incremental smoothing against integer arithmetic."""
from pathlib import Path
import sys
from cocotb.runner import get_runner

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
if __name__ == '__main__':
    output = ROOT/'work/smoothing-sim'
    output.mkdir(parents=True, exist_ok=True)
    runner = get_runner('verilator')
    runner.build(verilog_sources=[ROOT/'fpga/state_processor/rtl/sp_ram.sv'] +
                 [HERE/'rtl'/f'{name}.sv' for name in ('jp_sample_ram', 'jp_divider', 'job_processor')],
                 includes=[ROOT/'fpga/state_processor/rtl'], hdl_toplevel='job_processor',
                 parameters=dict(MAX_FRAMES=32, BUFFERS=8, INPUT_PORTS=1, OUTPUT_PORTS=1),
                 build_args=['--Wall', '--timescale', '1ns/1ps'], build_dir=output/'build', always=True)
    sys.path.insert(0, str(HERE/'tests'))
    result = runner.test(hdl_toplevel='job_processor', test_module='test_smoothing', test_dir=output)
    import xml.etree.ElementTree as ET
    if list(ET.parse(result).getroot().iter('failure')) or list(ET.parse(result).getroot().iter('error')):
        raise SystemExit('smoothing comparison failed')
