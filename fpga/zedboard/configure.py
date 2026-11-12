#!/usr/bin/env python3
"""Bind a prepared workload to a directly connected Zynq AXI3 master."""
import argparse
import json
from pathlib import Path
import sys
import generate

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT/'fpga/audio_io'))
from sources import sources

STATISTICS = ('status', 'completed_quanta', 'measured_quanta', 'total_cycles', 'last_cycles',
              'min_cycles', 'max_cycles', 'wall_cycles', 'read_bursts', 'write_bursts',
              'read_bytes', 'write_bytes', 'address_stall_cycles', 'response_stall_cycles',
              'output_words', 'output_digest')


def tcl(value):
    value = str(value)
    for char in ('\\', '"', '$', '[', ']'):
        value = value.replace(char, '\\'+char)
    return '"'+value+'"'


def configure(workload, output, fabric_mhz=100, physical_base=0x01000000,
              read_buffers=4, write_buffers=4, metadata_lines=8, reference_lines=4):
    workload, output = Path(workload).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    config = json.loads((workload/'config.json').read_text())
    params = dict(config['parameters'], PHYSICAL_BASE=physical_base, READ_BUFFERS=read_buffers,
                  WRITE_BUFFERS=write_buffers, TABLE_LINES=metadata_lines, REF_LINES=reference_lines)
    blocks = params['SLOTS']//params['BLOCK_CHUNKS']
    pages = (blocks+14)//15+8*params['VIEWS']+16
    arena_bytes = 4*(((params['SLOTS']*params['CHUNK_FRAMES']+15)//16)*16+2*((blocks+15)//16)*16+pages*16)
    if physical_base < 0 or physical_base % 8 or physical_base+arena_bytes > 512*1024*1024:
        raise ValueError('eight-byte aligned benchmark arena must fit inside ZedBoard DDR')
    if fabric_mhz <= 0 or min(read_buffers, write_buffers, metadata_lines, reference_lines) < 1:
        raise ValueError('positive clock and buffer/cache counts required')
    if params['AXI_DATA_WIDTH'] != 64:
        raise ValueError('board deployment uses the 64-bit HP0 port')
    params.update(BOOT_FILE=(workload/'boot.hex').as_posix(), EVENT_FILE=(workload/'events.hex').as_posix())
    _, ports = generate.axi.header(generate.generate())
    axi_ports = [(p.replace('AXI_DATA_WIDTH', '64'), n) for p, n in ports if n.startswith('m_axi_')]
    for i, (p, n) in enumerate(axi_ports):
        if n in ('m_axi_awid', 'm_axi_arid', 'm_axi_wid', 'm_axi_bid', 'm_axi_rid'):
            p = p.replace('wire ', 'wire [5:0] ')
            axi_ports[i] = p, n
    text = '''module zedboard_bench (
    input wire clk, input wire aresetn,
    input wire start, input wire stop, input wire snapshot,
    input wire [31:0] quanta_limit, input wire [31:0] warmup_quanta,
    input wire [31:0] gap_cycles, input wire [31:0] watchdog_cycles,
    output wire [31:0] status,
    output wire [127:0] debug_state, output wire [95:0] debug_axi,
'''
    text += ',\n'.join(f'    output wire [63:0] stat{i}' for i in range(16))+',\n'
    text += ',\n'.join('    '+p+n for p, n in axi_ports)+''',
    output wire [1:0] m_axi_awlock, output wire [1:0] m_axi_arlock,
    output wire [3:0] m_axi_awcache, output wire [3:0] m_axi_arcache,
    output wire [2:0] m_axi_awprot, output wire [2:0] m_axi_arprot,
    output wire [3:0] m_axi_awqos, output wire [3:0] m_axi_arqos
);
    wire reset = !aresetn;
    wire inner_awid, inner_arid, inner_wid;
    wire [1023:0] statistics;
    wire run_pulse, done_pulse;
    wire [63:0] debug_quantum;
    wire [31:0] debug_cycles;
    assign m_axi_awid = {5'b0,inner_awid};
    assign m_axi_arid = {5'b0,inner_arid};
    assign m_axi_wid = {5'b0,inner_wid};
    assign m_axi_awlock = 0;
    assign m_axi_arlock = 0;
    assign m_axi_awcache = 3;
    assign m_axi_arcache = 3;
    assign m_axi_awprot = 0;
    assign m_axi_arprot = 0;
    assign m_axi_awqos = 0;
    assign m_axi_arqos = 0;
    assign debug_state = {debug_quantum,debug_cycles,status};
    assign debug_axi = {m_axi_awaddr,m_axi_araddr,m_axi_awlen,m_axi_arlen,m_axi_awsize,m_axi_arsize,
                       m_axi_bresp,m_axi_rresp,m_axi_awvalid,m_axi_awready,m_axi_wvalid,m_axi_wready,
                       m_axi_wlast,m_axi_bvalid,m_axi_bready,m_axi_arvalid,m_axi_arready,
                       m_axi_rvalid,m_axi_rready,m_axi_rlast,run_pulse,done_pulse};
'''
    for i in range(16):
        text += f'    assign stat{i} = statistics[{i*64}+:64];\n'
    text += '    timing_bench #(\n'+',\n'.join('        .'+k+'('+ (json.dumps(v) if isinstance(v, str) else str(v))+')' for k, v in params.items())+') bench (\n'
    connections = []
    for _, name in ports:
        target = {'m_axi_awid': 'inner_awid', 'm_axi_arid': 'inner_arid', 'm_axi_wid': 'inner_wid',
                  'm_axi_rid': '|m_axi_rid', 'm_axi_bid': '|m_axi_bid'}.get(name, name)
        connections.append('        .'+name+'('+target+')')
    text += ',\n'.join(connections)+'\n    );\nendmodule\n'
    top = output/'zedboard_bench.v'
    top.write_text(text)
    manifest = sources(True)+[HERE/'rtl/timing_bench.sv', top]
    variables = dict(bench_root=ROOT.as_posix(), bench_output=output.as_posix(),
                     bench_fabric_mhz=fabric_mhz, bench_warmup=config['warmup_quanta'])
    script = ''.join('set '+k+' '+tcl(v)+'\n' for k, v in variables.items())
    script += 'set bench_sources [list '+ ' '.join(tcl(p.as_posix()) for p in manifest)+']\n'
    script += 'set bench_axi_signals [list '+ ' '.join(n for _, n in axi_ports)+ ' m_axi_awlock m_axi_arlock m_axi_awcache m_axi_arcache m_axi_awprot m_axi_arprot m_axi_awqos m_axi_arqos]\n'
    script += 'set bench_statistics [list '+' '.join(STATISTICS)+']\n'
    (output/'manifest.tcl').write_text(script)
    (output/'deployment.json').write_text(json.dumps(dict(workload=config, parameters=params,
        fabric_mhz=fabric_mhz, arena_bytes=arena_bytes, statistics=STATISTICS), indent=2)+'\n')
    return top


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workload', type=Path, default=ROOT/'work/zedboard/workload')
    parser.add_argument('--output', type=Path, default=ROOT/'work/zedboard/build')
    parser.add_argument('--fabric-mhz', type=float, default=100)
    parser.add_argument('--physical-base', type=lambda v: int(v, 0), default=0x01000000)
    parser.add_argument('--read-buffers', type=int, default=4)
    parser.add_argument('--write-buffers', type=int, default=4)
    parser.add_argument('--metadata-lines', type=int, default=8)
    parser.add_argument('--reference-lines', type=int, default=4)
    args = parser.parse_args()
    print(configure(args.workload, args.output, args.fabric_mhz, args.physical_base,
                    args.read_buffers, args.write_buffers, args.metadata_lines, args.reference_lines))


if __name__ == '__main__':
    main()
