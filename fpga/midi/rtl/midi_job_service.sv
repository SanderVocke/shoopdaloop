module midi_job_service #(
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8,
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, RETIREMENTS=256,
    parameter integer INPUT_EVENTS=256, MAINTENANCE=64,
    parameter integer TABLE_ENTRIES=(MAX_EVENTS+2*BLOCK_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1
) (
    input wire clk, input wire reset, input wire [63:0] arena_base, input wire [31:0] quantum_frames,
    output wire [63:0] arena_bytes,
    input wire command_valid, output wire command_ready, input wire [5:0] command_op,
    input wire [63:0] command_arg0, input wire [63:0] command_arg1, input wire [63:0] command_arg2,
    input wire [63:0] command_arg3, input wire [63:0] command_arg4,
    output wire response_valid, input wire response_ready, output reg [3:0] response_error,
    output reg [63:0] response_handle, output reg [63:0] response_frame,
    output reg [31:0] response_count,
    output reg [23:0] response_packet, output reg [1:0] response_length,
    output wire packet_valid, input wire packet_ready, input wire packet_refused,
    output wire [23:0] packet_data, output wire [1:0] packet_length,
    output reg [31:0] packet_destination, output reg [31:0] packet_offset,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    `include "midi_functions.svh"
    initial begin
        if(LOOPS<1 || LOOPS>128 || INPUTS<1 || DESTINATIONS<1 || INPUT_EVENTS<1 || MAINTENANCE<0)
            $fatal(1,"Invalid MIDI runtime capacities");
    end
    localparam [63:0] CONTENT_BYTES=64'(VIEWS)*64+64'(VIEWS)*64'(TABLE_ENTRIES)*4+64'(BLOCKS)*8+
        64'(BLOCKS)*64'(BLOCK_EVENTS)*16+64'(RETIREMENTS)*8;
    localparam [63:0] CONTEXT_OFFSET=CONTENT_BYTES, HISTORY_OFFSET=CONTEXT_OFFSET+64'(LOOPS)*128;
    assign arena_bytes=HISTORY_OFFSET+64'(INPUTS)*96;
    initial if(HISTORY_OFFSET+64'(INPUTS)*96>64'h100000000) $fatal(1,"MIDI arena offsets exceed 32 bits");

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
    reg [64-1:0] own_address;
    reg [32-1:0] own_data;
    reg [24-1:0] send_packet;
    reg [2-1:0] send_length;
    reg [LOOP_BITS-1:0] head_write_index;
    reg [HEAD_BITS-1:0] head_write_data;
    reg [1-1:0] prepared;
    reg [1-1:0] running;
    reg [5:0] op;
    reg [63:0] a0;
    reg [63:0] a1;
    reg [63:0] a2;
    reg [63:0] a3;
    reg [63:0] a4;
    reg [31:0] data_word;
    reg [2:0] last_error;
    reg [63:0] head_word;
    function automatic [31:0] first_set_bit(input [31:0] bits);
        begin
            first_set_bit=0;
            for(integer n=31;n>=0;n=n-1) if(bits[n]) first_set_bit=32'(n);
        end
    endfunction
    (* ram_style="block" *) reg [31:0] context_words[0:CONTEXT_WORDS-1];
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
            16'd0: io_read_data=64'(arena_base);
            16'd1: io_read_data=64'(quantum_frames);
            16'd2: io_read_data=64'(op);
            16'd3: io_read_data=64'(a0);
            16'd4: io_read_data=64'(a1);
            16'd5: io_read_data=64'(a2);
            16'd6: io_read_data=64'(a3);
            16'd7: io_read_data=64'(a4);
            16'd8: io_read_data=64'(data_word);
            16'd9: io_read_data=64'(last_error);
            16'd10: io_read_data=64'(head_word);
            16'd11: io_read_data=64'(accepted_refused);
            default: io_read_data=0;
        endcase
    end

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
            response_error<=0;
            response_handle<=0;
            response_frame<=0;
            response_count<=0;
            response_packet<=0;
            response_length<=0;
            packet_destination<=0;
            packet_offset<=0;
            own_address<=0;
            own_data<=0;
            send_packet<=0;
            send_length<=0;
            head_write_index<=0;
            head_write_data<=0;
            prepared<=0;
            running<=0;
            op<=0;
            a0<=0;
            a1<=0;
            a2<=0;
            a3<=0;
            a4<=0;
            data_word<=0;
            last_error<=0;
            head_word<=0;
            accepted_refused<=0;
        end else begin
            if(phase==EXECUTE && operation==MC_IOWRITE) begin
                case(target_pc)
                    16'd12: response_error<= 4'(operand_a);
                    16'd13: response_handle<= 64'(operand_a);
                    16'd14: response_frame<= 64'(operand_a);
                    16'd15: response_count<= 32'(operand_a);
                    16'd16: response_packet<= 24'(operand_a);
                    16'd17: response_length<= 2'(operand_a);
                    16'd18: packet_destination<= 32'(operand_a);
                    16'd19: packet_offset<= 32'(operand_a);
                    16'd20: own_address<= 64'(operand_a);
                    16'd21: own_data<= 32'(operand_a);
                    16'd22: send_packet<= 24'(operand_a);
                    16'd23: send_length<= 2'(operand_a);
                    16'd24: head_write_index<= LOOP_BITS'(operand_a);
                    16'd25: head_write_data<= HEAD_BITS'(operand_a);
                    16'd26: prepared<= 1'(operand_a);
                    16'd27: running<= 1'(operand_a);
                    16'd8: data_word<= 32'(operand_a);
                    16'd9: last_error<= 3'(operand_a);
                    16'd10: head_word<= 64'(operand_a);
                    default: begin end
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
endmodule
