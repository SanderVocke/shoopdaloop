#!/usr/bin/env python3
"""Generate the benchmark interfaces from the canonical AXI pipeline."""
import argparse
import importlib.util
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location('axi', ROOT/'fpga/axi_memory/generate_tops.py')
axi = importlib.util.module_from_spec(spec)
spec.loader.exec_module(axi)


def generate():
    match, ports = axi.header((ROOT/'fpga/axi_memory/rtl/loop_pipeline_axi.sv').read_text())
    params = match[2].strip()
    defaults = dict(LOOP_DEPTH=16, REQUEST_DEPTH=128, EVENT_DEPTH=256, PROGRAM_DEPTH=64,
                    STATE_DEPTH=32, JOB_DEPTH=80, EXTERNAL_IO=1, BUFFERS=32,
                    INPUT_PORTS=16, OUTPUT_PORTS=16, AXI_DATA_WIDTH=64)
    for name, value in defaults.items():
        params = re.sub(rf'(parameter integer {name} = )\d+', rf'\g<1>{value}', params)
    params += ''',
    parameter integer INSTRUCTION_COUNT = 64, STATE_COUNT = 32, BUFFER_COUNT = 32, JOB_BUDGET = 80,
    parameter integer BOOT_WORDS = 166, EVENT_WORDS = 74,
    parameter BOOT_FILE = "fpga/audio_io/examples/default/boot.hex",
    parameter EVENT_FILE = "fpga/audio_io/examples/default/events.hex"'''
    external = [(p, n) for p, n in ports if n.startswith('m_axi_')]
    text = '''// Generated benchmark interface; do not edit by hand.
`include "state_processor_defs.svh"
module timing_bench #(
    '''+params+'''
) (
    input wire clk, input wire reset, input wire start, input wire stop, input wire snapshot,
    input wire [31:0] quanta_limit, input wire [31:0] warmup_quanta,
    input wire [31:0] gap_cycles, input wire [31:0] watchdog_cycles,
    output wire [31:0] status, output reg [1023:0] statistics,
    output wire run_pulse, output wire done_pulse,
    output wire [63:0] debug_quantum, output wire [31:0] debug_cycles,
'''
    text += ',\n'.join('    '+p+n for p, n in external)+'\n);\n'
    for p, n in ports:
        if n == 'clk':
            continue
        prefix = re.sub(r'(?:input|output)\s+(?:wire|reg)', 'wire', p)
        name = 'core_'+n if n == 'reset' or n.startswith('m_axi_') else n
        text += '    '+prefix+name+';\n'
    names = re.findall(r'parameter\s+integer\s+(\w+)\s*=', match[2])
    text += '    loop_pipeline_axi #(\n        '+',\n        '.join('.'+n+'('+n+')' for n in names)
    text += ',\n        .PHYSICAL_BASE(PHYSICAL_BASE)) core (\n'
    text += ',\n'.join('        .'+n+'('+('core_'+n if n == 'reset' or n.startswith('m_axi_') else n)+')' for _, n in ports)+'\n    );\n'
    text += '    `include "timing_body.svh"\nendmodule\n'
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    path = HERE/'rtl/timing_bench.sv'
    text = generate()
    if args.check:
        if not path.exists() or path.read_text() != text:
            raise SystemExit('stale benchmark interface; run fpga/zedboard/generate.py')
    else:
        path.write_text(text)


if __name__ == '__main__':
    main()
