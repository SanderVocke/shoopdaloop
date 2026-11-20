module midi_job_service #(
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8,
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, SNAPSHOTS=768, RETIREMENTS=256,
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
    output reg [63:0] response_state, output reg [31:0] response_count,
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
    localparam [63:0] SNAP_OFFSET=64'(VIEWS)*64+64'(VIEWS)*64'(TABLE_ENTRIES)*4+64'(BLOCKS)*8;
    localparam [63:0] CONTENT_BYTES=SNAP_OFFSET+64'(SNAPSHOTS)*16916+
        64'(BLOCKS)*64'(BLOCK_EVENTS)*16+64'(RETIREMENTS)*8;
    localparam [63:0] CONTEXT_OFFSET=CONTENT_BYTES,
        INPUT_OFFSET=CONTEXT_OFFSET+64'(LOOPS)*128,
        OUTPUT_OFFSET=INPUT_OFFSET+64'(INPUTS)*16900,
        ACTIVITY_OFFSET=OUTPUT_OFFSET+64'(DESTINATIONS)*16900,
        HISTORY_OFFSET=ACTIVITY_OFFSET+64'(LOOPS)*16900,
        CHECKPOINT_OFFSET=HISTORY_OFFSET+64'(INPUTS)*96,
        CHANGES_OFFSET=CHECKPOINT_OFFSET+64'(INPUTS)*64'(SNAPSHOTS)*16,
        MAP_OFFSET=CHANGES_OFFSET+64'(INPUTS)*532,
        DIRECTORY_OFFSET=MAP_OFFSET+64'(SNAPSHOTS)*64'(DESTINATIONS)*532;
    localparam [63:0] SCRATCH_CHANGES_OFFSET=DIRECTORY_OFFSET+64'(SNAPSHOTS)*64'(DESTINATIONS)*48;
    assign arena_bytes=SCRATCH_CHANGES_OFFSET+532;
    initial if(SCRATCH_CHANGES_OFFSET+532 > 64'h100000000) $fatal(1,"MIDI arena offsets exceed 32 bits");

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
    reg [64-1:0] own_address;
    reg [32-1:0] own_data;
    reg [3-1:0] state_op;
    reg [64-1:0] state_current;
    reg [64-1:0] state_target;
    reg [64-1:0] state_map;
    reg [64-1:0] state_activity;
    reg [1-1:0] state_track_activity;
    reg [1-1:0] state_track_changes;
    reg [64-1:0] state_changes;
    reg [16-1:0] state_channels;
    reg [24-1:0] state_packet;
    reg [2-1:0] state_length;
    reg [13-1:0] state_first;
    reg [13-1:0] state_count;
    reg [32-1:0] state_pairs;
    reg [24-1:0] recovery_packet;
    reg [LOOP_BITS-1:0] head_write_index;
    reg [HEAD_BITS-1:0] head_write_data;
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
    function automatic bit expensive(input [23:0] p);
        expensive=p[7:0]==8'hff || (p[7:4]==11 &&
            (p[15:8]==64 || p[15:8]==66 || p[15:8]==69 || p[15:8]==120 || p[15:8]==121 || p[15:8]>=123));
    endfunction
    wire state_ready,state_valid,state_packet_valid;
    wire [23:0] state_packet_data;
    wire [1:0] state_packet_length;
    wire [2:0] state_error;
    wire state_memory_valid,state_memory_ready,state_memory_write;
    wire [63:0] state_memory_address;
    wire [31:0] state_memory_write_data;
    wire state_memory_response_valid,state_memory_response_ready;
    (* ram_style="block" *) reg [31:0] context_words[0:CONTEXT_WORDS-1];
    (* ram_style="block" *) reg [HEAD_BITS-1:0] heads[0:LOOPS-1];
    reg [31:0] context_q;
    reg [HEAD_BITS-1:0] head_q;
    reg panic_refused;
    reg [63:0] io_read_data;
    `include "midi_control_core.svh"
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
            16'd11: io_read_data=64'(panic_refused);
            default: io_read_data=0;
        endcase
    end

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
            response_error<=0;
            response_handle<=0;
            response_frame<=0;
            response_state<=0;
            response_count<=0;
            response_packet<=0;
            response_length<=0;
            packet_destination<=0;
            packet_offset<=0;
            own_address<=0;
            own_data<=0;
            state_op<=0;
            state_current<=0;
            state_target<=0;
            state_map<=0;
            state_activity<=0;
            state_track_activity<=0;
            state_track_changes<=0;
            state_changes<=0;
            state_channels<=0;
            state_packet<=0;
            state_length<=0;
            state_first<=0;
            state_count<=0;
            state_pairs<=0;
            recovery_packet<=0;
            head_write_index<=0;
            head_write_data<=0;
            op<=0;
            a0<=0;
            a1<=0;
            a2<=0;
            a3<=0;
            a4<=0;
            data_word<=0;
            last_error<=0;
            head_word<=0;
            panic_refused<=0;
        end else begin
            if(phase==EXECUTE && operation==MC_IOWRITE) begin
                case(target_pc)
                    16'd12: response_error<= 4'(operand_a);
                    16'd13: response_handle<= 64'(operand_a);
                    16'd14: response_frame<= 64'(operand_a);
                    16'd15: response_state<= 64'(operand_a);
                    16'd16: response_count<= 32'(operand_a);
                    16'd17: response_packet<= 24'(operand_a);
                    16'd18: response_length<= 2'(operand_a);
                    16'd19: packet_destination<= 32'(operand_a);
                    16'd20: packet_offset<= 32'(operand_a);
                    16'd21: own_address<= 64'(operand_a);
                    16'd22: own_data<= 32'(operand_a);
                    16'd23: state_op<= 3'(operand_a);
                    16'd24: state_current<= 64'(operand_a);
                    16'd25: state_target<= 64'(operand_a);
                    16'd26: state_map<= 64'(operand_a);
                    16'd27: state_activity<= 64'(operand_a);
                    16'd28: state_track_activity<= 1'(operand_a);
                    16'd29: state_track_changes<= 1'(operand_a);
                    16'd30: state_changes<= 64'(operand_a);
                    16'd31: state_channels<= 16'(operand_a);
                    16'd32: state_packet<= 24'(operand_a);
                    16'd33: state_length<= 2'(operand_a);
                    16'd34: state_first<= 13'(operand_a);
                    16'd35: state_count<= 13'(operand_a);
                    16'd36: state_pairs<= 32'(operand_a);
                    16'd37: recovery_packet<= 24'(operand_a);
                    16'd38: head_write_index<= LOOP_BITS'(operand_a);
                    16'd39: head_write_data<= HEAD_BITS'(operand_a);
                    16'd8: data_word<= 32'(operand_a);
                    16'd9: last_error<= 3'(operand_a);
                    16'd10: head_word<= 64'(operand_a);
                    default: begin end
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
    midi_state_service states (
        .clk(clk),.reset(reset),.command_valid(state_command_valid),.command_ready(state_ready),.command_op(state_op),
        .command_current(state_current),.command_target(state_target),.command_map(state_map),
        .command_directory(arena_base+DIRECTORY_OFFSET),.command_pairs(state_pairs),
        .command_first(state_first),.command_count(state_count),.command_channels(state_channels),
        .command_track_activity(state_track_activity),.command_activity(state_activity),
        .command_track_changes(state_track_changes),.command_changes(state_changes),
        .command_packet(state_packet),.command_length(state_length),
        .response_valid(state_valid),.response_ready(state_response_ready),.response_error(state_error),
        .packet_valid(state_packet_valid),.packet_ready(packet_ready),.packet_refused(packet_refused),
        .packet_data(state_packet_data),.packet_length(state_packet_length),
        .memory_valid(state_memory_valid),
        .memory_ready(state_memory_ready),
        .memory_write(state_memory_write),
        .memory_address(state_memory_address),
        .memory_write_data(state_memory_write_data),
        .memory_response_valid(state_memory_response_valid),
        .memory_response_ready(state_memory_response_ready),
        .memory_response_error(memory_response_error),.memory_read_data(memory_read_data)
    );

endmodule
