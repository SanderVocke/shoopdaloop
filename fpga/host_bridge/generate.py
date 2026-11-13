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

# Core configuration shared by the Marlin bridge top and the ZedBoard host bitstream.
PROFILE = dict(REUSE_VIEWS=1, LOOP_DEPTH=4, REQUEST_DEPTH=32, EVENT_DEPTH=64, PROGRAM_DEPTH=256,
               STATE_DEPTH=256, JOB_DEPTH=512, MAX_FRAMES=32, BUFFERS=128, INPUT_PORTS=2,
               OUTPUT_PORTS=2, VIEWS=32, SLOTS=32768, CHUNK_FRAMES=32, BLOCK_CHUNKS=1,
               RETIRED_TABLES=256, AXI_DATA_WIDTH=64)
PROFILE['TABLE_PAGES'] = (2*PROFILE['SLOTS']//PROFILE['BLOCK_CHUNKS']+14)//15 + 8*PROFILE['VIEWS'] + 16
SIMULATION_BASE = 0x01000000


def arena_bytes():
    p = PROFILE
    words = (((p['SLOTS']*p['CHUNK_FRAMES']+15)//16)*16 + 2*((p['SLOTS']//p['BLOCK_CHUNKS']+15)//16)*16
             + p['TABLE_PAGES']*16 + p['RETIRED_TABLES']*16)
    return 4*words


def profile_parameters(external_io, physical_base):
    values = dict(PROFILE, EXTERNAL_IO=external_io)
    text = ', '.join(f'.{name}({value})' for name, value in values.items())
    return text + f", .PHYSICAL_BASE(32'h{physical_base:08x})"


AXI_LITE_PORTS = '''input wire [11:0] s_axi_awaddr, input wire s_axi_awvalid, output wire s_axi_awready,
    input wire [31:0] s_axi_wdata, input wire s_axi_wvalid, output wire s_axi_wready,
    output wire s_axi_bvalid, input wire s_axi_bready,
    input wire [11:0] s_axi_araddr, input wire s_axi_arvalid, output wire s_axi_arready,
    output wire [31:0] s_axi_rdata, output wire s_axi_rvalid, input wire s_axi_rready'''


def marlin_top():
    words = arena_bytes()//4
    return f'''// Generated Marlin top for the CPU host bridge; do not edit by hand.
// The shared core profile behind the AXI-lite register map, with a functional DDR model.
module marlin_bridge #(parameter integer EXTERNAL_IO = 0) (
    input wire clk, input wire reset, output wire irq,
    {AXI_LITE_PORTS}
);
    wire [1:0] s_axi_bresp, s_axi_rresp;
    wire m_axi_awid, m_axi_wid, m_axi_bid, m_axi_arid, m_axi_rid;
    wire [31:0] m_axi_awaddr, m_axi_araddr;
    wire [3:0] m_axi_awlen, m_axi_arlen;
    wire [2:0] m_axi_awsize, m_axi_arsize;
    wire [1:0] m_axi_awburst, m_axi_arburst, m_axi_bresp, m_axi_rresp;
    wire m_axi_awvalid, m_axi_awready, m_axi_wlast, m_axi_wvalid, m_axi_wready, m_axi_bvalid, m_axi_bready;
    wire m_axi_arvalid, m_axi_arready, m_axi_rlast, m_axi_rvalid, m_axi_rready;
    wire [{PROFILE['AXI_DATA_WIDTH']-1}:0] m_axi_wdata, m_axi_rdata;
    wire [{PROFILE['AXI_DATA_WIDTH']//8-1}:0] m_axi_wstrb;
    host_core #({profile_parameters(0, SIMULATION_BASE).replace('.EXTERNAL_IO(0)', '.EXTERNAL_IO(EXTERNAL_IO)')}) bridge (
        .clk, .reset, .irq, .s_axi_awaddr, .s_axi_awvalid, .s_axi_awready, .s_axi_wdata,
        .s_axi_wstrb(4'hf), .s_axi_wvalid, .s_axi_wready, .s_axi_bresp, .s_axi_bvalid, .s_axi_bready,
        .s_axi_araddr, .s_axi_arvalid, .s_axi_arready, .s_axi_rdata, .s_axi_rresp, .s_axi_rvalid,
        .s_axi_rready, .m_axi_awid, .m_axi_awaddr, .m_axi_awlen, .m_axi_awsize, .m_axi_awburst,
        .m_axi_awvalid, .m_axi_awready, .m_axi_wdata, .m_axi_wstrb, .m_axi_wid, .m_axi_wlast,
        .m_axi_wvalid, .m_axi_wready, .m_axi_bid, .m_axi_bresp, .m_axi_bvalid, .m_axi_bready,
        .m_axi_arid, .m_axi_araddr, .m_axi_arlen, .m_axi_arsize, .m_axi_arburst, .m_axi_arvalid,
        .m_axi_arready, .m_axi_rid, .m_axi_rdata, .m_axi_rresp, .m_axi_rlast, .m_axi_rvalid,
        .m_axi_rready);
    axi_direct_memory #(.DATA_WIDTH({PROFILE['AXI_DATA_WIDTH']}), .WORDS({words}), .BASE(32'h{SIMULATION_BASE:08x})) memory (
        .clk, .reset, .sim_image_request(2'b0), .sim_image_done(), .sim_control(5'b0),
        .sim_fail_token(32'b0), .sim_fail_seen(), .sim_busy(), .sim_active(), .sim_error(), .sim_write(),
        .sim_index(), .sim_reads(), .sim_writes(), .sim_capacity(), .m_axi_awid, .m_axi_awaddr,
        .m_axi_awlen, .m_axi_awsize, .m_axi_awburst, .m_axi_awvalid, .m_axi_awready, .m_axi_wdata,
        .m_axi_wstrb, .m_axi_wid, .m_axi_wlast, .m_axi_wvalid, .m_axi_wready, .m_axi_bid, .m_axi_bresp,
        .m_axi_bvalid, .m_axi_bready, .m_axi_arid, .m_axi_araddr, .m_axi_arlen, .m_axi_arsize,
        .m_axi_arburst, .m_axi_arvalid, .m_axi_arready, .m_axi_rid, .m_axi_rdata, .m_axi_rresp,
        .m_axi_rlast, .m_axi_rvalid, .m_axi_rready);
endmodule

module marlin_bridge_external (
    input wire clk, input wire reset, output wire irq,
    {AXI_LITE_PORTS}
);
    marlin_bridge #(.EXTERNAL_IO(1)) top (.*);
endmodule
'''


def board_top(physical_base):
    """The ZedBoard host top: the shared profile with core-generated audio, its register map on
    GP0 and its DDR master adapted to S_AXI_HP0."""
    width = PROFILE['AXI_DATA_WIDTH']
    return f'''// Generated ZedBoard host top; do not edit by hand.
module zedboard_host (
    (* X_INTERFACE_INFO = "xilinx.com:signal:clock:1.0 clk CLK" *)
    (* X_INTERFACE_PARAMETER = "ASSOCIATED_BUSIF s_axi:m_axi, ASSOCIATED_RESET aresetn" *)
    input wire clk,
    (* X_INTERFACE_INFO = "xilinx.com:signal:reset:1.0 aresetn RST" *)
    (* X_INTERFACE_PARAMETER = "POLARITY ACTIVE_LOW" *)
    input wire aresetn,
    output wire irq,
    input wire [11:0] s_axi_awaddr, input wire [2:0] s_axi_awprot, input wire s_axi_awvalid,
    output wire s_axi_awready, input wire [31:0] s_axi_wdata, input wire [3:0] s_axi_wstrb,
    input wire s_axi_wvalid, output wire s_axi_wready, output wire [1:0] s_axi_bresp,
    output wire s_axi_bvalid, input wire s_axi_bready, input wire [11:0] s_axi_araddr,
    input wire [2:0] s_axi_arprot, input wire s_axi_arvalid, output wire s_axi_arready,
    output wire [31:0] s_axi_rdata, output wire [1:0] s_axi_rresp, output wire s_axi_rvalid,
    input wire s_axi_rready,
    output wire [5:0] m_axi_awid, output wire [31:0] m_axi_awaddr, output wire [3:0] m_axi_awlen,
    output wire [2:0] m_axi_awsize, output wire [1:0] m_axi_awburst, output wire [1:0] m_axi_awlock,
    output wire [3:0] m_axi_awcache, output wire [2:0] m_axi_awprot, output wire [3:0] m_axi_awqos,
    output wire m_axi_awvalid, input wire m_axi_awready,
    output wire [5:0] m_axi_wid, output wire [{width-1}:0] m_axi_wdata, output wire [{width//8-1}:0] m_axi_wstrb,
    output wire m_axi_wlast, output wire m_axi_wvalid, input wire m_axi_wready,
    input wire [5:0] m_axi_bid, input wire [1:0] m_axi_bresp, input wire m_axi_bvalid, output wire m_axi_bready,
    output wire [5:0] m_axi_arid, output wire [31:0] m_axi_araddr, output wire [3:0] m_axi_arlen,
    output wire [2:0] m_axi_arsize, output wire [1:0] m_axi_arburst, output wire [1:0] m_axi_arlock,
    output wire [3:0] m_axi_arcache, output wire [2:0] m_axi_arprot, output wire [3:0] m_axi_arqos,
    output wire m_axi_arvalid, input wire m_axi_arready,
    input wire [5:0] m_axi_rid, input wire [{width-1}:0] m_axi_rdata, input wire [1:0] m_axi_rresp,
    input wire m_axi_rlast, input wire m_axi_rvalid, output wire m_axi_rready
);
    wire inner_awid, inner_wid, inner_arid;
    assign m_axi_awid = {{5'b0, inner_awid}};
    assign m_axi_wid = {{5'b0, inner_wid}};
    assign m_axi_arid = {{5'b0, inner_arid}};
    assign m_axi_awlock = 0;
    assign m_axi_arlock = 0;
    assign m_axi_awcache = 3;
    assign m_axi_arcache = 3;
    assign m_axi_awprot = 0;
    assign m_axi_arprot = 0;
    assign m_axi_awqos = 0;
    assign m_axi_arqos = 0;
    host_core #({profile_parameters(1, physical_base)}) core (
        .clk(clk), .reset(!aresetn), .irq(irq),
        .s_axi_awaddr(s_axi_awaddr), .s_axi_awvalid(s_axi_awvalid), .s_axi_awready(s_axi_awready),
        .s_axi_wdata(s_axi_wdata), .s_axi_wstrb(s_axi_wstrb), .s_axi_wvalid(s_axi_wvalid),
        .s_axi_wready(s_axi_wready), .s_axi_bresp(s_axi_bresp), .s_axi_bvalid(s_axi_bvalid),
        .s_axi_bready(s_axi_bready), .s_axi_araddr(s_axi_araddr), .s_axi_arvalid(s_axi_arvalid),
        .s_axi_arready(s_axi_arready), .s_axi_rdata(s_axi_rdata), .s_axi_rresp(s_axi_rresp),
        .s_axi_rvalid(s_axi_rvalid), .s_axi_rready(s_axi_rready),
        .m_axi_awid(inner_awid), .m_axi_awaddr(m_axi_awaddr), .m_axi_awlen(m_axi_awlen),
        .m_axi_awsize(m_axi_awsize), .m_axi_awburst(m_axi_awburst), .m_axi_awvalid(m_axi_awvalid),
        .m_axi_awready(m_axi_awready), .m_axi_wdata(m_axi_wdata), .m_axi_wstrb(m_axi_wstrb),
        .m_axi_wid(inner_wid), .m_axi_wlast(m_axi_wlast), .m_axi_wvalid(m_axi_wvalid),
        .m_axi_wready(m_axi_wready), .m_axi_bid(|m_axi_bid), .m_axi_bresp(m_axi_bresp),
        .m_axi_bvalid(m_axi_bvalid), .m_axi_bready(m_axi_bready), .m_axi_arid(inner_arid),
        .m_axi_araddr(m_axi_araddr), .m_axi_arlen(m_axi_arlen), .m_axi_arsize(m_axi_arsize),
        .m_axi_arburst(m_axi_arburst), .m_axi_arvalid(m_axi_arvalid), .m_axi_arready(m_axi_arready),
        .m_axi_rid(|m_axi_rid), .m_axi_rdata(m_axi_rdata), .m_axi_rresp(m_axi_rresp),
        .m_axi_rlast(m_axi_rlast), .m_axi_rvalid(m_axi_rvalid), .m_axi_rready(m_axi_rready));
endmodule
'''


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
            ROOT/'src/rust/rt_fpga_adapter/src/bridge/map.rs': rust,
            ROOT/'src/rust/rt_fpga_sim/rtl/marlin_bridge.sv': marlin_top()}


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
