#!/usr/bin/env python3
"""Generate the MIDI execution wrapper around the shared control datapath."""

import argparse
from pathlib import Path
import re
from compile_control import combined, save_generated, OUTPUTS, INPUTS, IO_ID

ROOT = Path(__file__).resolve().parent


def generate(check=False):
    source = (ROOT / "program/midi_job_control.sv").read_text()
    text = source.split("    reg [15:0] pc,")[0].replace(
        "module midi_job_control #(", "module midi_job_service #("
    )
    text += """
    localparam integer LOOP_BITS=LOOPS>1 ? $clog2(LOOPS) : 1;
    localparam integer INPUT_BITS=INPUTS>1 ? $clog2(INPUTS) : 1;
    localparam integer DEST_BITS=DESTINATIONS>1 ? $clog2(DESTINATIONS) : 1;
    localparam integer HEAD_BITS=59+DEST_BITS;
    localparam integer CONTEXT_WORDS=LOOPS*32,CONTEXT_ADDRESS_BITS=$clog2(CONTEXT_WORDS);
    localparam integer VIEW_BITS=VIEWS>1 ? $clog2(VIEWS) : 1;
    localparam integer TABLE_BITS=TABLE_ENTRIES>1 ? $clog2(TABLE_ENTRIES) : 1;
    localparam [63:0] TABLE_BASE=64'(VIEWS)*64,
        REF_BASE=TABLE_BASE+64'(VIEWS)*64'(TABLE_ENTRIES)*4,
        FREE_BASE=REF_BASE+64'(BLOCKS)*4,
        EVENT_BASE=FREE_BASE+64'(BLOCKS)*4,
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
    for name in ["first_set_bit"]:
        function = re.search(
            r"^    function automatic [^\n]*\b" + name + r"\(.*?^    endfunction",
            source,
            re.M | re.S,
        )
        assert function, name
        text += function.group(0) + "\n"
    text += """    (* ram_style="block" *) reg [31:0] context_words[0:CONTEXT_WORDS-1];
    (* ram_style="block" *) reg [HEAD_BITS-1:0] heads[0:LOOPS-1];
    reg [31:0] context_q;
    reg [HEAD_BITS-1:0] head_q;
    (* ram_style="block" *) reg [74:0] summaries[0:LOOPS-1];
    reg [74:0] summary_q,summary_work;
    reg accepted_refused;
    reg [63:0] io_read_data;
    `include "midi_control_core.svh"
    `include "midi_control_rom.svh"
    always @* begin
        io_read_data=0;
        case(target_pc)
"""
    for name in INPUTS:
        value = "accepted_refused" if name == "packet_refused" else name
        text += f"            16'd{IO_ID[name]}: io_read_data=64'({value});\n"
    # Some output registers may be explicitly read through I/O in future programs.
    text += """            default: io_read_data=0;
        endcase
    end
"""
    text += """
    assign command_ready=phase==EXECUTE && operation==MC_COMMAND && !reset;
    assign response_valid=(phase==FAST_REPLY || phase==EXECUTE && operation==MC_REPLY) && !reset;
    assign packet_valid=phase==EXECUTE && operation==MC_PACKET && !reset;
    assign packet_data=send_packet;
    assign packet_length=send_length;
    assign memory_valid=(bulk_request || phase==EXECUTE && (operation==MC_MEMREAD || operation==MC_MEMWRITE)) && !reset;
    assign memory_write=bulk_request || bulk_wait ? phase==BULK_WRITE_REQUEST || phase==BULK_WRITE_WAIT : operation==MC_MEMWRITE;
    assign memory_address=bulk_request || bulk_wait ?
        (phase==BULK_READ_REQUEST || phase==BULK_READ_WAIT ? bulk_source : bulk_target) : own_address;
    assign memory_write_data=bulk_request || bulk_wait ? bulk_write_data : own_data;
    assign memory_response_ready=(phase==MEMORY_WAIT || bulk_wait) && !reset;
    wire [CONTEXT_ADDRESS_BITS-1:0] context_word_index=CONTEXT_ADDRESS_BITS'({transfer_context,
        phase==FAST_METADATA ? 5'd18+{3'b0,work_index[1:0]} : work_index[4:0]});
    wire [31:0] fast_metadata_word=work_index[1:0]==0 ? a1[31:0] : work_index[1:0]==1 ? a1[63:32] :
        work_index[1:0]==2 ? a2[31:0] : a2[63:32];
    wire summary_read=phase==EXECUTE && operation==MC_CTXREAD || command_ready && command_valid && command_arg0<64'(LOOPS);
    wire [LOOP_BITS-1:0] summary_index=command_ready ? command_arg0[LOOP_BITS-1:0] : operand_a[LOOP_BITS-1:0];
    wire head_read=phase==MERGE_READ || phase==EXECUTE && operation==MC_HEADREAD && operand_a<64'(LOOPS);
    wire [LOOP_BITS-1:0] head_index=phase==MERGE_READ ? merge_index : operand_a[LOOP_BITS-1:0];
    wire context_committed=phase==CONTEXT_STORE_WRITE && (context_dirty & ~(32'b1<<work_index[4:0]))==0;
    always @(posedge clk) begin
        if(!reset) begin
            if(phase==EXECUTE && operation==MC_CLOAD) context_q<=context_words[CONTEXT_ADDRESS_BITS'({transfer_context,operand_a[4:0]})];
            if(phase==CONTEXT_STORE_WRITE || phase==CONTEXT_ZERO || phase==FAST_METADATA)
                context_words[context_word_index]<=phase==CONTEXT_ZERO ? 0 : phase==FAST_METADATA ? fast_metadata_word : operand_a[31:0];
            if(head_read) head_q<=heads[head_index];
            if(summary_read) summary_q<=summaries[summary_index];
            if(phase==CONTEXT_ZERO && work_index==31 || context_committed)
                summaries[transfer_context]<=phase==CONTEXT_ZERO ? 0 : summary_work;
            if(phase==EXECUTE && operation==MC_HEADWRITE) heads[head_write_index]<=head_write_data;
        end
    end
    always @(posedge clk) begin
        if(reset) summary_work<=0;
        else begin
            if(phase==META_LATCH || phase==FAST_CONTEXT) summary_work<=summary_q;
            if(phase==FAST_CAPTURE) summary_work[68:67]<=0;
            if(phase==EXECUTE && operation==MC_FILL && operand_a==64'(R_c) && operand_b==32) summary_work<=0;
            if(phase==EXECUTE && operation==MC_CSTORE) begin
                case(operand_a[4:0])
                    0: summary_work[64]<=operand_b!=0;
                    4: summary_work[31:0]<=operand_b[31:0];
                    5: summary_work[63:32]<=operand_b[31:0];
                    8: summary_work[72]<=operand_b!=0;
                    9: summary_work[73]<=operand_b!=0;
                    10: summary_work[70]<=operand_b!=0;
                    11: summary_work[71]<=operand_b!=0;
                    16: summary_work[66:65]<=operand_b[1:0];
                    17: summary_work[68:67]<=operand_b[1:0];
                    22: summary_work[69]<=operand_b[0];
                    23: summary_work[74]<=operand_b!=0;
                    default: begin end
                endcase
            end
        end
    end
    always @(posedge clk) begin
        if(reset) begin
"""
    for name in OUTPUTS:
        text += f"            {name}<=0;\n"
    for name in captured:
        text += f"            {name}<=0;\n"
    text += """            accepted_refused<=0;
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
            if(phase==COMMAND_CHECK && fast_command) begin response_error<=0;response_count<=0;response_length<=0;end
            if(command_ready && command_valid) begin
                op<=command_op;a0<=command_arg0;a1<=command_arg1;a2<=command_arg2;a3<=command_arg3;a4<=command_arg4;
                last_error<=0;
            end
            if((phase==MEMORY_WAIT || phase==BULK_READ_WAIT) && memory_response_valid) data_word<=memory_read_data;
            if(phase==HEAD_WAIT) head_word<=64'(head_q);
            if(phase==EXECUTE && operation==MC_PACKET && packet_ready) accepted_refused<=packet_refused;
        end
    end
"""
    text += "endmodule\n"
    save_generated(ROOT / "rtl/midi_job_service.sv", text, check)
    content = (ROOT / "program/midi_content_control.sv").read_text().replace(
        "module midi_content_control #(", "module midi_store #(" )
    save_generated(ROOT / "rtl/midi_store.sv", content, check)



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
