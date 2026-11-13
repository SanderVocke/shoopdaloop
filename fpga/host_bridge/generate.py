#!/usr/bin/env python3
"""Generate the CPU host bridge top and its register map from the canonical AXI pipeline."""
import argparse
import importlib.util
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location('axi', ROOT/'fpga/axi_memory/generate_tops.py')
axi = importlib.util.module_from_spec(spec)
spec.loader.exec_module(axi)

IDENTITY = 0x53484f50
VERSION = 1
ADDRESS_BITS = 12
DEFINES = {name: int(value) for name, value in re.findall(
    r'`define (\w+) (\d+)', (ROOT/'fpga/state_processor/rtl/state_processor_defs.svh').read_text())}

CAPACITIES = ['PROGRAM_DEPTH', 'STATE_DEPTH', 'BUFFERS', 'MAX_FRAMES', 'INPUT_PORTS', 'OUTPUT_PORTS',
              'LOOP_DEPTH', 'REQUEST_DEPTH', 'EVENT_DEPTH', 'VIEWS', 'SLOTS', 'BLOCK_CHUNKS',
              'CHUNK_FRAMES', 'EXTERNAL_IO', 'AXI_DATA_WIDTH', 'PHYSICAL_BASE']

STATUS = [('FAULTED', 0, 1), ('ERROR_STAGE', 1, 2), ('ERROR_CODE', 8, 8), ('RUN_BUSY', 16, 1),
          ('TP_BUSY', 17, 1), ('CFG_BUSY', 18, 1), ('AUDIO_BUSY', 19, 1), ('STORE_CFG_BUSY', 20, 1),
          ('STORE_BUSY', 21, 1), ('RESET_BUSY', 22, 1), ('QUIESCENT', 23, 1), ('DONE', 24, 1),
          ('MEMORY_FAULT', 25, 1)]


def words(ports, name):
    declaration = dict((n, d) for d, n in ports)[name]
    match = re.search(r'\[\s*(`?\w+)\s*(?:-\s*1)?\s*:\s*0\s*\]', declaration)
    bound = match[1]
    width = DEFINES[bound[1:]] if bound.startswith('`') else int(bound) + 1
    return (width + 31)//32


def registers(ports):
    tp_data, tp_response = words(ports, 'command_data'), words(ports, 'response_data')
    cfg, store_cfg = words(ports, 'cfg_write_data'), words(ports, 'store_cfg_write_data')
    assert words(ports, 'cfg_read_data') == cfg and words(ports, 'store_cfg_read_data') == store_cfg
    table = [
        ('ID', 0x000, 1), ('VERSION', 0x004, 1), ('CONTROL', 0x008, 1), ('STATUS', 0x00c, 1),
        ('QUANTA', 0x010, 2), ('LAST_CYCLES', 0x018, 1), ('OUTPUT_WORDS', 0x01c, 1), ('DIGEST', 0x020, 2),
        ('CAPACITY', 0x040, len(CAPACITIES)),
        ('RUN', 0x080, 8), ('RUN_GO', 0x0a0, 1),
        ('TP_OPCODE', 0x0c0, 1), ('TP_INDEX', 0x0c4, 1), ('TP_GO', 0x0c8, 1), ('TP_ERROR', 0x0cc, 1),
        ('TP_DATA', 0x100, tp_data), ('TP_RESPONSE', 0x180, tp_response),
        ('CFG_CONTROL', 0x200, 1), ('CFG_ADDRESS', 0x204, 1), ('CFG_GO', 0x208, 1), ('CFG_ERROR', 0x20c, 1),
        ('CFG_WRITE', 0x210, cfg), ('CFG_READ', 0x230, cfg),
        ('AUDIO_CONTROL', 0x280, 1), ('AUDIO_ADDRESS', 0x284, 1), ('AUDIO_WRITE', 0x288, 1),
        ('AUDIO_GO', 0x28c, 1), ('AUDIO_READ', 0x290, 1), ('AUDIO_ERROR', 0x294, 1),
        ('STORE_CFG_CONTROL', 0x300, 1), ('STORE_CFG_ADDRESS', 0x304, 1), ('STORE_CFG_GO', 0x308, 1),
        ('STORE_CFG_ERROR', 0x30c, 1), ('STORE_CFG_WRITE', 0x310, store_cfg), ('STORE_CFG_READ', 0x330, store_cfg),
        ('STORE_OPCODE', 0x380, 1), ('STORE_VIEW', 0x384, 1), ('STORE_START', 0x388, 2),
        ('STORE_LENGTH', 0x390, 2), ('STORE_COUNT', 0x398, 1), ('STORE_GO', 0x39c, 1),
        ('STORE_RESULT', 0x3a0, 1), ('STORE_RESULT_VIEW', 0x3a4, 1), ('STORE_TX', 0x3a8, 1),
        ('STORE_TX_SPACE', 0x3ac, 1), ('STORE_RX', 0x3b0, 1), ('STORE_RX_LEVEL', 0x3b4, 1),
        ('STORE_WRITTEN', 0x3b8, 1),
    ]
    used = []
    for name, offset, count in table:
        span = range(offset, offset + 4*count)
        assert offset % 4 == 0 and span[-1] < 1 << ADDRESS_BITS, name
        assert not any(set(span) & set(other) for other in used), name
        used.append(span)
    return table


def generate():
    match, ports = axi.header((ROOT/'fpga/axi_memory/rtl/loop_pipeline_axi.sv').read_text())
    params = match[2].strip()
    external = [(p, n) for p, n in ports if n.startswith('m_axi_')]
    text = '''// Generated CPU host bridge top; do not edit by hand.
`include "state_processor_defs.svh"
module host_core #(
    '''+params+'''
) (
    input wire clk, input wire reset, output wire irq,
    input wire [ADDRESS_BITS-1:0] s_axi_awaddr, input wire s_axi_awvalid, output wire s_axi_awready,
    input wire [31:0] s_axi_wdata, input wire [3:0] s_axi_wstrb, input wire s_axi_wvalid, output wire s_axi_wready,
    output wire [1:0] s_axi_bresp, output wire s_axi_bvalid, input wire s_axi_bready,
    input wire [ADDRESS_BITS-1:0] s_axi_araddr, input wire s_axi_arvalid, output wire s_axi_arready,
    output wire [31:0] s_axi_rdata, output wire [1:0] s_axi_rresp, output wire s_axi_rvalid, input wire s_axi_rready,
'''.replace('ADDRESS_BITS', str(ADDRESS_BITS))
    text += ',\n'.join('    '+p+n for p, n in external)+'\n);\n'
    text += '    `include "host_map.svh"\n    `include "host_capacity.svh"\n'
    for p, n in ports:
        if n == 'clk':
            continue
        prefix = re.sub(r'(?:input|output)\s+(?:wire|reg)', 'wire', p)
        name = 'core_'+n if n == 'reset' or n.startswith('m_axi_') else n
        text += '    '+prefix+name+';\n'
    names = re.findall(r'parameter\s+integer\s+(\w+)\s*=', match[2])
    text += '    loop_pipeline_axi #(\n        '+',\n        '.join('.'+n+'('+n+')' for n in names)
    text += ',\n        .PHYSICAL_BASE(PHYSICAL_BASE)) core (\n'
    text += ',\n'.join('        .'+n+'('+('core_'+n if n == 'reset' or n.startswith('m_axi_') else n)+')'
                       for _, n in ports)+'\n    );\n'
    text += '    `include "host_body.svh"\nendmodule\n'
    table = registers(ports)
    sv = '// Generated host bridge register map; do not edit by hand.\n'
    sv += f"localparam [31:0] HOST_IDENTITY = 32'h{IDENTITY:08x};\nlocalparam [31:0] HOST_VERSION_NUMBER = {VERSION};\n"
    for name, offset, count in table:
        sv += f"localparam [{ADDRESS_BITS-1}:0] HOST_{name} = {ADDRESS_BITS}'h{offset:03x};\n"
        if count > 1:
            sv += f'localparam integer HOST_{name}_WORDS = {count};\n'
    for name, lsb, width in STATUS:
        sv += f'localparam integer HOST_STATUS_{name} = {lsb};\n'
    rust = '//! Generated host bridge register map; do not edit by hand.\n'
    rust += f'pub const IDENTITY: u32 = 0x{IDENTITY:08x};\npub const VERSION_NUMBER: u32 = {VERSION};\n'
    for name, offset, count in table:
        rust += f'pub const {name}: u32 = 0x{offset:03x};\n'
        if count > 1:
            rust += f'pub const {name}_WORDS: usize = {count};\n'
    for name, lsb, width in STATUS:
        rust += f'pub const STATUS_{name}: (u32, u32) = ({lsb}, {width});\n'
    rust += 'pub const CAPACITIES: [&str; %d] = [\n%s];\n' % (
        len(CAPACITIES), ''.join(f'    "{c}",\n' for c in CAPACITIES))
    capacity = '// Generated capacity readback; do not edit by hand.\n'
    capacity += f'wire [31:0] host_capacity [0:{len(CAPACITIES)-1}];\n'
    capacity += ''.join(f"assign host_capacity[{i}] = 32'({c});\n" for i, c in enumerate(CAPACITIES))
    return {HERE/'rtl/host_core.sv': text,
            HERE/'rtl/host_map.svh': sv,
            HERE/'rtl/host_capacity.svh': capacity,
            ROOT/'src/rust/rt_fpga_adapter/src/bridge/map.rs': rust}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for path, text in generate().items():
        if args.check:
            if not path.exists() or path.read_text() != text:
                raise SystemExit(f'stale {path.name}; run fpga/host_bridge/generate.py')
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, newline='\n')


if __name__ == '__main__':
    main()
