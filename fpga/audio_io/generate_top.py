#!/usr/bin/env python3
"""Generate the continuous-audio AXI top from the canonical pipeline ports."""
import argparse
import importlib.util
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location('axi_tops', ROOT/'fpga/axi_memory/generate_tops.py')
axi = importlib.util.module_from_spec(spec)
spec.loader.exec_module(axi)


def generate():
    source = (ROOT/'fpga/axi_memory/rtl/loop_pipeline_axi.sv').read_text()
    match, ports = axi.header(source)
    params = match[2].strip().replace('    parameter integer EXTERNAL_IO = 0,\n', '')
    params = params.replace('parameter integer EXTERNAL_IO = 0,\n', '')
    for field, value in [('LOOP_DEPTH', 16), ('REQUEST_DEPTH', 128), ('EVENT_DEPTH', 256),
                         ('PROGRAM_DEPTH', 256), ('STATE_DEPTH', 128), ('JOB_DEPTH', 256),
                         ('BUFFERS', 128), ('INPUT_PORTS', 16), ('OUTPUT_PORTS', 16),
                         ('VIEWS', 32), ('SLOTS', 1024)]:
        params = re.sub(rf'(parameter integer {field} = )\d+', rf'\g<1>{value}', params)
    params += ',\n    parameter integer P = MAX_FRAMES, RX_BANKS = 3, TX_BANKS = 3,\n    parameter integer WATCHDOG_CYCLES = 1000000, CONTROL_DEPTH = 128'
    removed = {'maintenance_enable', 'run_valid', 'sample_input_read', 'sample_input_address', 'sample_input_data',
               'sample_output_write', 'sample_output_address', 'sample_output_data'}
    public = [(p, n) for p, n in ports if n not in removed]
    extra = '''input wire audio_clk, input wire session_reset, input wire enable,
    input wire quiesce, output wire memory_quiescent, output wire session_reset_ready,
    input wire [31:0] epoch, input wire frame_tick, input wire frame_prepare,
    input wire [INPUT_PORTS*32-1:0] rx_samples,
    output wire [OUTPUT_PORTS*32-1:0] tx_samples, output wire tx_valid,
    output wire [63:0] audio_frame, output wire [63:0] session_frame,
    input wire control_valid, output wire control_ready, input wire [745:0] control_data,
    output wire control_result_valid, input wire control_result_ready, output wire [1087:0] control_result_data,
    output wire recovery_requested, output wire [3:0] recovery_reason,
    output wire [63:0] failure_sequence, output wire [63:0] process_sequence,
    output wire [31:0] processed_quanta, output wire [31:0] played_quanta,
    output wire [31:0] missed_deadlines, output wire [31:0] discarded_quanta,
    output wire [31:0] input_losses, output wire [31:0] rx_high_water, output wire [31:0] tx_high_water'''
    text = '// Generated continuous-audio AXI wrapper; do not edit by hand.\n`include "state_processor_defs.svh"\n\n'
    text += 'module audio_loop_pipeline #(\n    '+params+'\n) (\n'
    text += ',\n'.join('    '+p+n for p, n in public)+',\n    '+extra+'\n);\n'
    internal = {'maintenance_enable', 'reset', 'run_valid', 'command_valid', 'command_ready', 'command_opcode', 'command_index',
                'command_data', 'response_valid', 'response_ready', 'response_error', 'response_data',
                'cfg_valid', 'cfg_ready', 'cfg_state', 'cfg_write', 'cfg_address', 'cfg_write_data',
                'cfg_response_valid', 'cfg_response_error', 'cfg_read_data',
                'audio_cfg_valid', 'audio_cfg_ready', 'audio_cfg_space', 'audio_cfg_write', 'audio_cfg_address', 'audio_cfg_write_data', 'audio_cfg_response_valid', 'audio_cfg_response_error', 'audio_cfg_read_data', 'store_cmd_valid', 'store_cmd_ready', 'store_cmd_opcode', 'store_cmd_view', 'store_cmd_start', 'store_cmd_length', 'store_cmd_count', 'store_write_valid', 'store_write_ready', 'store_write_data', 'store_read_valid', 'store_read_ready', 'store_read_data', 'store_cfg_valid', 'store_cfg_ready', 'store_cfg_write', 'store_cfg_space', 'store_cfg_address', 'store_cfg_write_data', 'store_cfg_response_valid', 'store_cfg_response_error', 'store_cfg_read_data'}
    for p, n in ports:
        if n in internal or n in removed or n.startswith('m_axi_'):
            prefix = re.sub(r'(?:input|output)\s+(?:wire|reg)', 'wire', p)
            text += '    '+prefix+('core_'+n if n in internal or n.startswith('m_axi_') else n)+';\n'
    names = re.findall(r'parameter\s+integer\s+(\w+)\s*=', match[2])
    text += '    loop_pipeline_axi #(\n        '+',\n        '.join('.'+n+'('+('1' if n == 'EXTERNAL_IO' else n)+')' for n in names)+',\n        .PHYSICAL_BASE(PHYSICAL_BASE)) core (\n'
    text += ',\n'.join('        .'+n+'('+('core_'+n if n in internal or n.startswith('m_axi_') else n)+')' for _, n in ports)+'\n    );\n'
    text += (HERE/'rtl/audio_loop_body.svh').read_text()
    text += '\nendmodule\n'
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    path = HERE/'rtl/audio_loop_pipeline.sv'
    text = generate()
    if args.check:
        if not path.exists() or path.read_text() != text:
            raise SystemExit('stale audio top; run fpga/audio_io/generate_top.py')
    else:
        path.write_text(text)


if __name__ == '__main__':
    main()
