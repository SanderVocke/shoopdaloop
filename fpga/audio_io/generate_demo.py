#!/usr/bin/env python3
"""Generate a synthesizable prepared-program source/core/sink demonstration."""
import argparse
import re
from pathlib import Path
import generate_top

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def generate():
    source = (HERE/'rtl/audio_loop_pipeline.sv').read_text()
    _, ports = generate_top.axi.header(source)
    axi_ports = [(p, n) for p, n in ports if n.startswith('m_axi_')]
    observe = {'tx_samples', 'tx_valid', 'audio_frame', 'session_frame', 'process_sequence', 'processed_quanta',
               'played_quanta', 'missed_deadlines', 'discarded_quanta', 'input_losses', 'rx_high_water',
               'tx_high_water', 'recovery_requested', 'recovery_reason', 'failure_sequence', 'memory_quiescent',
               'faulted', 'done', 'error_code', 'error_stage', 'monitor_valid', 'monitor_ready', 'monitor_data'}
    extra = [(p, n) for p, n in ports if n in observe]
    text = '''// Generated prepared audio demonstration; do not edit by hand.
`include "state_processor_defs.svh"
module audio_demo #(
    parameter integer Q = 32, P = Q, CHANNELS = 16, RX_BANKS = 3, TX_BANKS = 3,
    parameter integer AXI_DATA_WIDTH = 32, CHUNK_FRAMES = Q,
    parameter integer BOOT_WORDS = 166, EVENT_WORDS = 74,
    parameter integer INSTRUCTIONS = 64, STATES = 32, BUFFERS = 32, JOB_BUDGET = 80,
    parameter integer RESET_CYCLES = 64, WATCHDOG_CYCLES = 1000000,
    parameter integer VIEWS = 32, SLOTS = 256,
    parameter integer TABLE_PAGES = (SLOTS+14)/15+8*VIEWS+16,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000,
    parameter BOOT_FILE = "fpga/audio_io/examples/default/boot.hex",
    parameter EVENT_FILE = "fpga/audio_io/examples/default/events.hex"
) (
    input wire clk, input wire audio_clk, input wire reset,
    input wire frame_tick, input wire frame_prepare, input wire job_enable,
    input wire restart, output wire boot_done, output wire boot_fault,
    output reg [31:0] epoch,
    output reg [31:0] recoveries,
    output reg [3:0] last_recovery_reason,
    output reg [63:0] last_failure_sequence, output reg [63:0] last_failure_logical_frame,
    output reg [7:0] last_error_code, output reg [1:0] last_error_stage,
'''
    text += ',\n'.join('    '+p.replace('INPUT_PORTS', 'CHANNELS').replace('OUTPUT_PORTS', 'CHANNELS')+n for p, n in extra+axi_ports)+'\n);\n'
    for p, n in ports:
        if n in observe or n.startswith('m_axi_') or n in {'clk', 'audio_clk', 'reset', 'frame_tick', 'frame_prepare', 'job_enable', 'epoch'}:
            continue
        text += '    '+re.sub(r'(?:input|output)\s+(?:wire|reg)', 'wire', p).replace('INPUT_PORTS', 'CHANNELS').replace('OUTPUT_PORTS', 'CHANNELS')+n+';\n'
    text += '''    audio_loop_pipeline #(.MAX_FRAMES(Q),.CHUNK_FRAMES(CHUNK_FRAMES),.P(P),
        .BUFFERS(BUFFERS),.INPUT_PORTS(CHANNELS),.OUTPUT_PORTS(CHANNELS),.RX_BANKS(RX_BANKS),.TX_BANKS(TX_BANKS),
        .VIEWS(VIEWS),.SLOTS(SLOTS),.TABLE_PAGES(TABLE_PAGES),.PHYSICAL_BASE(PHYSICAL_BASE),
        .REQUEST_DEPTH(128),.EVENT_DEPTH(256),.AXI_DATA_WIDTH(AXI_DATA_WIDTH),.WATCHDOG_CYCLES(WATCHDOG_CYCLES)) engine (.*);
'''
    text += (HERE/'rtl/audio_demo_body.svh').read_text()
    unused = [n for p, n in ports if p.startswith('output') and n not in observe and not n.startswith('m_axi_')]
    text += "    wire unused_status = &{1'b0,"+','.join(unused)+"};\nendmodule\n"
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    path = HERE/'rtl/audio_demo.sv'
    text = generate()
    if args.check:
        if not path.exists() or path.read_text() != text:
            raise SystemExit('stale demo top; run fpga/audio_io/generate_demo.py')
    else:
        path.write_text(text)


if __name__ == '__main__':
    main()
