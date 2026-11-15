#!/usr/bin/env python3
"""Registered fabric context generator used by subsystem measurements."""
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT/'fpga/axi_memory'))
from generate_tops import header


def context(source, module_name):
    match, ports = header(source)
    parameters = match[2]
    import re
    names = re.findall(r'parameter\s+(?:integer|\[[^]]+\])\s+(\w+)\s*=', parameters)
    lines = ['`include "state_processor_defs.svh"',
             'module registered_core_context #(', parameters,
             ') (input wire clk);', 'reg [7:0] startup = 0;',
             "always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;",
             "reg reset = 1;", 'always @(posedge clk) reset <= !(&startup);']
    for ordinal, (declaration, name) in enumerate(ports):
        if name in ('clk', 'reset'):
            continue
        bounds = re.search(r'\[([^:]+):0\]', declaration)
        bits = bounds[0] if bounds else ''
        width = f'({bounds[1]})+1' if bounds else '1'
        if declaration.startswith('input'):
            lines += [f'localparam integer PORT_BITS_{name} = {width};',
                      f'localparam integer SOURCE_BITS_{name} = PORT_BITS_{name} < 32 ? 32 : PORT_BITS_{name};',
                      f'(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_{name}-1:0] source_{name} = SOURCE_BITS_{name}\'({ordinal+1});',
                      f'always @(posedge clk) source_{name} <= {{source_{name}[SOURCE_BITS_{name}-2:0],source_{name}[SOURCE_BITS_{name}-1]^source_{name}[SOURCE_BITS_{name}-2]}};',
                      f"wire {bits} {name} = PORT_BITS_{name}'(source_{name});"]
        else:
            lines += [f'wire {bits} {name};',
                      f'(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg {bits} unused_capture_{name};',
                      f'always @(posedge clk) unused_capture_{name} <= {name};']
    lines += [f'(* KEEP_HIERARCHY = "yes" *) {module_name} #(',
              ',\n'.join(f'.{name}({name})' for name in names), ') core_under_test (',
              ',\n'.join(f'.{name}({name})' for _, name in ports), ');', 'endmodule']
    return '\n'.join(lines)+'\n'
