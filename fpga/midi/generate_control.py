#!/usr/bin/env python3
"""Generate the MIDI execution wrapper around the shared control datapath."""

import argparse
from pathlib import Path
import re
from compile_control import combined, save_generated, OUTPUTS, INPUTS, IO_ID

ROOT = Path(__file__).resolve().parent


def generate(check=False):
    source = (ROOT / "program/midi_job_control.sv").read_text()
    text = source.split("    reg [4:0] activity_channel;")[0].replace(
        "module midi_job_control #(", "module midi_job_service #("
    )
    text += """
    localparam integer LOOP_BITS=LOOPS>1 ? $clog2(LOOPS) : 1;
    localparam integer INPUT_BITS=INPUTS>1 ? $clog2(INPUTS) : 1;
    localparam integer SNAP_BITS=SNAPSHOTS>1 ? $clog2(SNAPSHOTS) : 1;
    localparam integer DEST_BITS=DESTINATIONS>1 ? $clog2(DESTINATIONS) : 1;
    localparam integer HEAD_BITS=59+DEST_BITS;
    localparam integer CONTEXT_WORDS=LOOPS*32,CONTEXT_ADDRESS_BITS=$clog2(CONTEXT_WORDS);
    localparam integer VIEW_BITS=VIEWS>1 ? $clog2(VIEWS) : 1;
    localparam integer TABLE_BITS=TABLE_ENTRIES>1 ? $clog2(TABLE_ENTRIES) : 1;
    localparam [63:0] TABLE_BASE=64'(VIEWS)*64,
        REF_BASE=TABLE_BASE+64'(VIEWS)*64'(TABLE_ENTRIES)*4,
        FREE_BASE=REF_BASE+64'(BLOCKS)*4,
        SNAP_BASE=FREE_BASE+64'(BLOCKS)*4,
        EVENT_BASE=SNAP_BASE+64'(SNAPSHOTS)*16916,
        RETIRE_BASE=EVENT_BASE+64'(BLOCKS)*64'(BLOCK_EVENTS)*16;
    `include "midi_control_program.svh"
"""
    ports = set(re.findall(r"output reg(?:\s*\[[^]]+\])?\s+(\w+)", text))
    for name, width in OUTPUTS.items():
        if name not in ports:
            text += f"    reg [{width}-1:0] {name};\n"
    captured = {
        "op": 6,
        **{f"a{i}": 64 for i in range(5)},
        "data_word": 32,
        "last_error": 3,
        "head_word": 64,
    }
    for name, width in captured.items():
        text += f"    reg [{width-1}:0] {name};\n"
    for name in ["first_set_bit", "expensive"]:
        function = re.search(
            r"^    function automatic [^\n]*\b" + name + r"\(.*?^    endfunction",
            source,
            re.M | re.S,
        )
        assert function, name
        text += function.group(0) + "\n"
    wires = re.findall(r"^    wire\s.*?;", source, re.M | re.S)
    for wire in wires:
        if "state_" in wire and "=" not in wire:
            text += wire + "\n"
    text += """    (* ram_style="block" *) reg [31:0] context_words[0:CONTEXT_WORDS-1];
    (* ram_style="block" *) reg [HEAD_BITS-1:0] heads[0:LOOPS-1];
    reg [31:0] context_q;
    reg [HEAD_BITS-1:0] head_q;
    reg panic_refused;
    reg [63:0] io_read_data;
    `include "midi_control_core.svh"
    always @* begin
        io_read_data=0;
        case(target_pc)
"""
    for name in INPUTS:
        value = "panic_refused" if name == "packet_refused" else name
        text += f"            16'd{IO_ID[name]}: io_read_data=64'({value});\n"
    # Some output registers may be explicitly read through I/O in future programs.
    text += """            default: io_read_data=0;
        endcase
    end
"""
    text += """
    wire state_command_valid=phase==EXECUTE && operation==MC_STATE && !reset;
    wire state_response_ready=phase==STATE_WAIT && !reset;
    wire select_state=state_command_valid || phase==STATE_WAIT;
    assign command_ready=phase==EXECUTE && operation==MC_COMMAND && !reset;
    assign response_valid=phase==EXECUTE && operation==MC_REPLY && !reset;
    wire panic_valid=phase==EXECUTE && operation==MC_PACKET;
    assign packet_valid=(panic_valid || select_state && state_packet_valid) && !reset;
    assign packet_data=panic_valid ? recovery_packet : state_packet_data;
    assign packet_length=panic_valid ? 2'd3 : state_packet_length;
    assign memory_valid=select_state ? state_memory_valid :
        (bulk_request || phase==EXECUTE && (operation==MC_MEMREAD || operation==MC_MEMWRITE)) && !reset;
    assign memory_write=select_state ? state_memory_write : bulk_request || bulk_wait ? phase==BULK_WRITE_REQUEST || phase==BULK_WRITE_WAIT : operation==MC_MEMWRITE;
    assign memory_address=select_state ? state_memory_address : bulk_request || bulk_wait ?
        (phase==BULK_READ_REQUEST || phase==BULK_READ_WAIT ? bulk_source : bulk_target) : own_address;
    assign memory_write_data=select_state ? state_memory_write_data : bulk_request || bulk_wait ? bulk_write_data : own_data;
    assign memory_response_ready=select_state ? state_memory_response_ready :
        (phase==MEMORY_WAIT || bulk_wait) && !reset;
    assign state_memory_ready=select_state && memory_ready;
    assign state_memory_response_valid=select_state && memory_response_valid;
    wire [CONTEXT_ADDRESS_BITS-1:0] context_word_index={transfer_context,work_index[4:0]};
    always @(posedge clk) begin
        if(!reset) begin
            if(phase==EXECUTE && operation==MC_CLOAD) context_q<=context_words[{transfer_context,operand_a[4:0]}];
            if(phase==CONTEXT_STORE_WRITE || phase==CONTEXT_ZERO)
                context_words[context_word_index]<=phase==CONTEXT_ZERO ? 0 : operand_a[31:0];
            if(phase==EXECUTE && operation==MC_HEADREAD && operand_a<64'(LOOPS)) head_q<=heads[operand_a[LOOP_BITS-1:0]];
            if(phase==EXECUTE && operation==MC_HEADWRITE) heads[head_write_index]<=head_write_data;
        end
    end
    always @(posedge clk) begin
        if(reset) begin
"""
    for name in OUTPUTS:
        text += f"            {name}<=0;\n"
    for name in captured:
        text += f"            {name}<=0;\n"
    text += """            panic_refused<=0;
        end else begin
            if(phase==EXECUTE && operation==MC_IOWRITE) begin
                case(target_pc)
"""
    for name in OUTPUTS:
        text += f"                    16'd{IO_ID[name]}: {name}<= {OUTPUTS[name]}'(operand_a);\n"
    for name, width in captured.items():
        if name not in ["op", "a0", "a1", "a2", "a3", "a4"]:
            text += f"                    16'd{IO_ID[name]}: {name}<= {width}'(operand_a);\n"
    text += """                    default: begin end
                endcase
            end
            if(command_ready && command_valid) begin
                op<=command_op;a0<=command_arg0;a1<=command_arg1;a2<=command_arg2;a3<=command_arg3;a4<=command_arg4;
                last_error<=0;
            end
            if((phase==MEMORY_WAIT || phase==BULK_READ_WAIT) && memory_response_valid) data_word<=memory_read_data;
            if(phase==HEAD_WAIT) head_word<=64'(head_q);
            if(phase==EXECUTE && operation==MC_PACKET && packet_ready) panic_refused<=packet_refused;
            if(phase==STATE_WAIT && state_valid) last_error<=state_error==5 && (state_op==3 || state_op==4 || state_op==5) ? 3 : state_error;
        end
    end
"""
    instances = source[
        source.index("    midi_state_service states") : source.index(
            "    reg [15:0] checkpoint_return"
        )
    ]
    instances = instances.replace("pc==STATE_START", "state_command_valid").replace(
        "pc==STATE_WAIT", "state_response_ready"
    )
    text += instances + "endmodule\n"
    save_generated(ROOT / "rtl/midi_job_service.sv", text, check)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="check checked-in RTL without changing files",
    )
    args = parser.parse_args()
    combined().write_files(args.check)
    generate(args.check)
