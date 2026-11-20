module midi_runtime #(
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8,
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, SNAPSHOTS=768, RETIREMENTS=256,
    parameter integer INPUT_EVENTS=256, MAINTENANCE=64,
    parameter integer TABLE_ENTRIES=(MAX_EVENTS+2*BLOCK_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1
) (
    input wire clk, input wire reset, input wire [63:0] arena_base, input wire [31:0] quantum_frames,
    output wire [63:0] arena_bytes,
    input wire command_valid, output wire command_ready, input wire [4:0] command_op,
    input wire [63:0] command_arg0, input wire [63:0] command_arg1, input wire [63:0] command_arg2,
    input wire [63:0] command_arg3, input wire [63:0] command_arg4,
    output wire response_valid, input wire response_ready, output reg [2:0] response_error,
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
    reg [15:0] pc, service_return, memory_return, context_return, history_return, copy_return, prepare_return;
    reg [4:0] op;
    reg prepared, running, seen_quantum;
    reg [63:0] a0,a1,a2,a3,a4,next_frame,start_frame,frames,event_base;
    reg [31:0] context_slot,input_slot,destination,scan,cursor,index,event_count,event_index;
    reg [31:0] data_word, c[0:31], h[0:23];
    reg [63:0] own_address;
    reg [31:0] own_data;
    reg [31:0] high_snapshot;
    reg [15:0] owners [0:DESTINATIONS-1];
    reg [31:0] density [0:INPUTS-1];
    reg [5:0] recovery [0:DESTINATIONS-1];
    reg [31:0] transfer_events,transfer_cells,transfer_metadata,publication_bindings;
    reg [63:0] copy_source,copy_target;
    reg [31:0] copy_count,copy_index;
    reg [63:0] snapshot_id,snapshot_base,prepare_map,prepare_entry;
    reg [31:0] prepare_destination,prepare_cursor,prepare_budget,prepare_amount;
    reg prepare_ready;
    reg [31:0] event_input,event_offset;
    reg [23:0] event_packet;
    reg [1:0] event_length;
    reg [63:0] checkpoint_snapshot;
    reg [31:0] checkpoint_order,checkpoint_index,checkpoint_word;
    reg [31:0] low,high,middle,range_index,range_order,replay_index;
    reg [63:0] query_frame,range_start,range_end,range_view;
    reg [31:0] range_input;
    reg [15:0] range_return;
    reg [63:0] best_frame;
    reg [31:0] best_slot;
    reg [23:0] best_packet;
    reg [1:0] best_length;
    reg found, established;
    reg [31:0] target_count,target_index;
    reg [63:0] target_list;
    reg [31:0] recovery_destination,recovery_offset;
    reg [15:0] recovery_return;
    reg [63:0] initial_snapshot;
    reg [3:0] store_op;
    reg [63:0] store_view,store_snapshot,store_arg0,store_arg1;
    reg [23:0] store_packet;
    reg [1:0] store_length;
    wire store_ready,store_valid;
    wire [2:0] store_error;
    wire [63:0] store_handle,store_state,store_frame,store_initial;
    wire [31:0] store_events,store_status,store_order,store_work,store_blocks,store_owner,store_references;
    wire [31:0] free_blocks,free_snapshots,free_views,free_retirements;
    wire [23:0] store_record_packet;
    wire [1:0] store_record_length;
    reg [2:0] last_error;
    reg [63:0] last_handle,last_state,last_frame,last_initial;
    reg [31:0] last_events,last_status,last_order,last_blocks,last_owner,last_references;
    reg [23:0] last_packet;
    reg [1:0] last_length;
    reg [2:0] state_op;
    reg [63:0] state_current,state_target,state_map,state_activity;
    reg state_track_activity,state_track_changes;
    reg [63:0] state_changes;
    reg [15:0] state_channels;
    reg [23:0] state_packet;
    reg [1:0] state_length;
    reg [12:0] state_first,state_count;
    reg [31:0] state_pairs;
    wire state_ready,state_valid,state_packet_valid;
    wire [23:0] state_packet_data;
    wire [1:0] state_packet_length;
    wire [2:0] state_error;
    wire store_memory_valid,store_memory_ready,store_memory_write;
    wire [63:0] store_memory_address;
    wire [31:0] store_memory_write_data;
    wire store_memory_response_valid,store_memory_response_ready;
    wire state_memory_valid,state_memory_ready,state_memory_write;
    wire [63:0] state_memory_address;
    wire [31:0] state_memory_write_data;
    wire state_memory_response_valid,state_memory_response_ready;
    localparam integer CONTEXT_WORDS=LOOPS*32, CONTEXT_ADDRESS_BITS=$clog2(CONTEXT_WORDS);
    (* ram_style="block" *) reg [31:0] context_words [0:CONTEXT_WORDS-1];
    reg [31:0] context_read_data;
    wire own_context=own_address>=arena_base+CONTEXT_OFFSET && own_address<arena_base+INPUT_OFFSET;
    wire [CONTEXT_ADDRESS_BITS-1:0] context_index=CONTEXT_ADDRESS_BITS'((own_address-(arena_base+CONTEXT_OFFSET))>>2);
    localparam [15:0] STORE_START=16'd6, STORE_WAIT=16'd7,
        STATE_START=16'd8, STATE_WAIT=16'd9;
    wire select_store=pc==STORE_START || pc==STORE_WAIT;
    wire select_state=pc==STATE_START || pc==STATE_WAIT;
    assign memory_valid=select_store ? store_memory_valid : select_state ? state_memory_valid :
        (pc==READ_REQUEST || pc==WRITE_REQUEST) && !own_context && !reset;
    assign memory_write=select_store ? store_memory_write : select_state ? state_memory_write : pc==WRITE_REQUEST;
    assign memory_address=select_store ? store_memory_address : select_state ? state_memory_address : own_address;
    assign memory_write_data=select_store ? store_memory_write_data : select_state ? state_memory_write_data : own_data;
    assign memory_response_ready=select_store ? store_memory_response_ready : select_state ? state_memory_response_ready :
        (pc==READ_WAIT || pc==WRITE_WAIT) && !own_context;
    assign store_memory_ready=select_store && memory_ready;
    assign store_memory_response_valid=select_store && memory_response_valid;
    assign state_memory_ready=select_state && memory_ready;
    assign state_memory_response_valid=select_state && memory_response_valid;
    assign packet_valid=(pc==PANIC_EMIT || state_packet_valid) && !reset;
    assign packet_data=pc==PANIC_EMIT ? recovery_packet : state_packet_data;
    assign packet_length=pc==PANIC_EMIT ? 2'd3 : state_packet_length;
    assign command_ready=pc==IDLE && !reset;
    assign response_valid=pc==RESPONSE && !reset;
    midi_store #(.BLOCK_EVENTS(BLOCK_EVENTS),.BLOCKS(BLOCKS),.MAX_EVENTS(MAX_EVENTS),
        .VIEWS(VIEWS),.SNAPSHOTS(SNAPSHOTS),.RETIREMENTS(RETIREMENTS),.TABLE_ENTRIES(TABLE_ENTRIES)) content (
        .clk(clk),.reset(reset),.arena_base(arena_base),.arena_bytes(),
        .command_valid(pc==STORE_START),.command_ready(store_ready),.command_op(store_op),
        .command_view(store_view),.command_snapshot(store_snapshot),.command_arg0(store_arg0),.command_arg1(store_arg1),
        .command_packet(store_packet),.command_length(store_length),
        .response_valid(store_valid),.response_ready(pc==STORE_WAIT),.response_error(store_error),
        .response_handle(store_handle),.response_state(store_state),.response_frame(store_frame),
        .response_order(store_order),.response_packet(store_record_packet),.response_length(store_record_length),
        .response_events(store_events),.response_status(store_status),.response_snapshot(store_initial),.response_work(store_work),
        .response_blocks(store_blocks),.response_owner(store_owner),.response_references(store_references),.free_block_slots(free_blocks),
        .free_snapshot_slots(free_snapshots),.free_view_slots(free_views),.free_retirement_slots(free_retirements),
        .memory_valid(store_memory_valid),
        .memory_ready(store_memory_ready),
        .memory_write(store_memory_write),
        .memory_address(store_memory_address),
        .memory_write_data(store_memory_write_data),
        .memory_response_valid(store_memory_response_valid),
        .memory_response_ready(store_memory_response_ready),
        .memory_response_error(memory_response_error),.memory_read_data(memory_read_data)
    );
    midi_state_service states (
        .clk(clk),.reset(reset),.command_valid(pc==STATE_START),.command_ready(state_ready),.command_op(state_op),
        .command_current(state_current),.command_target(state_target),.command_map(state_map),
        .command_directory(arena_base+DIRECTORY_OFFSET),.command_pairs(state_pairs),
        .command_first(state_first),.command_count(state_count),.command_channels(state_channels),
        .command_track_activity(state_track_activity),.command_activity(state_activity),
        .command_track_changes(state_track_changes),.command_changes(state_changes),
        .command_packet(state_packet),.command_length(state_length),
        .response_valid(state_valid),.response_ready(pc==STATE_WAIT),.response_error(state_error),
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

    reg [15:0] checkpoint_return,search_return,state_at_return,event_return;
    reg [15:0] failure_return;
    reg [31:0] last_input_offset,trim_order,failed_destination,failure_saved_scan;
    reg [63:0] pending_writer;
    reg [127:0] target_seen;
    reg [2:0] range_failure;
    reg [23:0] recovery_packet;
    reg [31:0] reserved_blocks,reserved_snapshots,reservation_scan,reservation_saved_scan,reservation_context,import_needed;
    reg [15:0] reservation_return,derive_return;
    reg prepare_is_derive;
    reg [63:0] derive_source_snapshot,derive_base_snapshot,derive_changes,next_checkpoint;
    reg [31:0] changed_bits,change_word_index;
    reg [63:0] alignment_value,alignment_remainder;
    reg [5:0] alignment_bit;
    reg [15:0] alignment_return;


    localparam [15:0] IDLE=16'd0,
        RESPONSE=16'd1,
        READ_REQUEST=16'd2,
        READ_WAIT=16'd3,
        WRITE_REQUEST=16'd4,
        WRITE_WAIT=16'd5,
        CONTEXT_READ=16'd10,
        CONTEXT_READ_NEXT=16'd11,
        CONTEXT_WRITE=16'd12,
        CONTEXT_WRITE_NEXT=16'd13,
        HISTORY_READ=16'd14,
        HISTORY_READ_NEXT=16'd15,
        HISTORY_WRITE=16'd16,
        HISTORY_WRITE_NEXT=16'd17,
        COPY_READ=16'd18,
        COPY_WRITE=16'd19,
        COPY_NEXT=16'd20,
        PREPARE_ENTRY=16'd21,
        PREPARE_VALID=16'd22,
        PREPARE_GENERATION=16'd23,
        PREPARE_POSITION=16'd24,
        PREPARE_CREATE=16'd25,
        PREPARE_CREATE_NEXT=16'd26,
        PREPARE_COMPARE=16'd27,
        PREPARE_COMPARED=16'd28,
        CHECKPOINT_READ=16'd29,
        CHECKPOINT_READ_NEXT=16'd30,
        CHECKPOINT_WRITE=16'd31,
        CHECKPOINT_WRITE_NEXT=16'd32,
        SEARCH=16'd33,
        SEARCH_COMPARE=16'd34,
        DISPATCH=16'd35,
        BOOT_CONTEXTS=16'd36,
        BOOT_CONTEXT=16'd37,
        BOOT_CONTEXT_NEXT=16'd38,
        BOOT_DIRECTORY=16'd39,
        BOOT_DIRECTORY_NEXT=16'd40,
        BOOT_OUTPUT=16'd41,
        BOOT_OUTPUT_NEXT=16'd42,
        BOOT_INPUT=16'd43,
        BOOT_INPUT_NEXT=16'd44,
        BOOT_ACTIVITY=16'd45,
        BOOT_ACTIVITY_NEXT=16'd46,
        BOOT_SNAPSHOT=16'd47,
        BOOT_HISTORIES=16'd48,
        BOOT_HISTORY=16'd49,
        BOOT_HISTORY_WRITER=16'd50,
        BOOT_HISTORY_OWNED=16'd51,
        BOOT_HISTORY_CHECKPOINT=16'd52,
        BOOT_HISTORY_SAVED=16'd53,
        BOOT_HISTORY_MASK_ZERO=16'd54,
        BOOT_HISTORY_NEXT=16'd55,
        CONTROL_CONTEXT=16'd56,
        CONTROL_DUPLICATE=16'd57,
        CONTROL_EXPORTED=16'd58,
        CONTROL_EXPORT_DESCRIPTION=16'd59,
        CONTROL_RELEASED=16'd60,
        EXPORT_READ=16'd61,
        CONTROL_SENT=16'd62,
        CONTROL_RETENTION=16'd63,
        CONTROL_FAULT=16'd64,
        IMPORT_VALIDATE=16'd65,
        IMPORT_VALIDATED=16'd66,
        IMPORT_SNAPSHOT=16'd67,
        IMPORT_WRITER=16'd68,
        IMPORT_FAILED=16'd69,
        IMPORT_BEGUN=16'd70,
        IMPORT_APPEND=16'd71,
        IMPORT_APPENDED=16'd72,
        IMPORT_FINISH=16'd73,
        IMPORT_FINISHED=16'd74,
        IMPORT_PREPARED=16'd75,
        CONTROL_REPLACE=16'd76,
        CONTROL_REPLACE_DESCRIPTION=16'd77,
        CONTROL_REPLACE_ALIGNED=16'd78,
        CONTROL_REPLACE_GENERATION=16'd79,
        CONTROL_REPLACE_READY=16'd80,
        CONTROL_REPLACE_RETAINED=16'd81,
        CONTROL_REPLACE_OLD=16'd82,
        CONTROL_REPLACE_WRITER=16'd83,
        CONTROL_RANGE=16'd84,
        GRAB_ALIGNED=16'd85,
        GRAB_VALIDATE=16'd86,
        GRAB_VALIDATE_SLOT=16'd87,
        GRAB_VALIDATE_CONTEXT=16'd88,
        RANGE_STATE=16'd89,
        RANGE_CREATED=16'd90,
        RANGE_FAILED=16'd91,
        RANGE_OWNED=16'd92,
        GRAB_RETAIN=16'd93,
        GRAB_RETAINED=16'd94,
        GRAB_PUBLISH=16'd95,
        GRAB_PUBLISH_SLOT=16'd96,
        GRAB_PUBLISH_CONTEXT=16'd97,
        GRAB_PUBLISH_OLD=16'd98,
        GRAB_PUBLISH_WRITER=16'd99,
        GRAB_PUBLISH_NEXT=16'd100,
        STATE_AT=16'd101,
        STATE_AT_DESCRIPTION=16'd102,
        STATE_AT_INDEX=16'd103,
        STATE_AT_ORDER=16'd104,
        STATE_AT_CHECKPOINT=16'd105,
        STATE_AT_RETAINED=16'd106,
        STATE_AT_COPIED=16'd107,
        STATE_AT_REPLAY=16'd108,
        STATE_AT_EVENT=16'd109,
        STATE_AT_APPLIED=16'd110,
        STATE_AT_PREPARED=16'd111,
        VALIDATE_INPUTS_CLEAR=16'd112,
        VALIDATE_INPUT=16'd113,
        EVENT_READ=16'd114,
        EVENT_READ_NEXT=16'd115,
        VALIDATE_EVENT=16'd116,
        VALIDATE_CONTEXT=16'd117,
        VALIDATE_CONTEXT_NEXT=16'd118,
        PROCESS_RECORDS=16'd119,
        PROCESS_RECORD=16'd120,
        PROCESS_RECORD_CONTEXT=16'd121,
        PROCESS_RECORD_FINISHED=16'd122,
        PROCESS_RECORD_OWNER_RELEASED=16'd123,
        PROCESS_RECORD_REPLACED=16'd124,
        PROCESS_RECORD_PREPARED=16'd125,
        PROCESS_RECORD_BEGIN=16'd126,
        PROCESS_RECORD_SEED=16'd127,
        PROCESS_RECORD_SNAPSHOT=16'd128,
        PROCESS_RECORD_SEED_PREPARED=16'd129,
        PROCESS_RECORD_SEED_SAVED=16'd130,
        PROCESS_RECORD_WRITER=16'd131,
        PROCESS_RECORD_WRITER_OWNED=16'd132,
        PROCESS_RECORD_WRITER_OLD=16'd133,
        PROCESS_RECORD_NEXT=16'd134,
        PROCESS_RELEASE_SEED=16'd135,
        PROCESS_RELEASE_SEED_LOADED=16'd136,
        PROCESS_RELEASE_SEED_RELEASED=16'd137,
        PROCESS_RELEASE_SEED_NEXT=16'd138,
        PROCESS_INPUT=16'd139,
        PROCESS_INPUT_EVENT=16'd140,
        PROCESS_INPUT_HISTORY=16'd141,
        PROCESS_INPUT_CAPTURED=16'd142,
        PROCESS_INPUT_STATE=16'd143,
        PROCESS_INPUT_OBSERVED=16'd144,
        PROCESS_INPUT_CHECKPOINT_BASE=16'd145,
        PROCESS_INPUT_CHECKPOINT=16'd146,
        PROCESS_INPUT_CHECKPOINT_PREPARED=16'd147,
        PROCESS_INPUT_CHECKPOINT_WRITTEN=16'd148,
        PROCESS_INPUT_HISTORY_SAVED=16'd149,
        PROCESS_INPUT_RECORDS_START=16'd150,
        PROCESS_INPUT_RECORD=16'd151,
        PROCESS_INPUT_RECORD_LOADED=16'd152,
        PROCESS_INPUT_RECORDED=16'd153,
        PROCESS_INPUT_RECORD_NEXT=16'd154,
        PROCESS_ADVANCE_HISTORY=16'd155,
        PROCESS_ADVANCE_HISTORY_LOADED=16'd156,
        PROCESS_HISTORY_ADVANCED=16'd157,
        PROCESS_HISTORY_TRIM=16'd158,
        PROCESS_HISTORY_TRIM_INDEX=16'd159,
        PROCESS_HISTORY_TRIM_ORDER=16'd160,
        PROCESS_HISTORY_TRIM_CHECKPOINT=16'd161,
        PROCESS_HISTORY_TRIM_SECOND=16'd162,
        PROCESS_HISTORY_TRIM_OLD=16'd163,
        PROCESS_HISTORY_TRIM_RELEASED=16'd164,
        PROCESS_HISTORY_DISCARD=16'd165,
        PROCESS_HISTORY_DISCARDED=16'd166,
        PROCESS_ADVANCE_HISTORY_NEXT=16'd167,
        PROCESS_ADVANCE_RECORD=16'd168,
        PROCESS_ADVANCE_RECORD_LOADED=16'd169,
        PROCESS_RECORD_ADVANCED=16'd170,
        PROCESS_ADVANCE_RECORD_NEXT=16'd171,
        PROCESS_OWNERS_CLEAR=16'd172,
        PROCESS_CLEANUP=16'd173,
        PROCESS_CLEANUP_LOADED=16'd174,
        PROCESS_CLEANED=16'd175,
        PROCESS_CLEANUP_RELEASED=16'd176,
        PROCESS_CLEANUP_RESET=16'd177,
        PROCESS_CLEANUP_NEXT=16'd178,
        PROCESS_OWNERS=16'd179,
        PROCESS_OWNER_LOADED=16'd180,
        PROCESS_OWNER_NEXT=16'd181,
        PROCESS_PLAY=16'd182,
        PROCESS_PLAY_LOADED=16'd183,
        PROCESS_PLAY_DESCRIPTION=16'd184,
        PROCESS_PLAY_GENERATION=16'd185,
        PROCESS_PLAY_READY=16'd186,
        PROCESS_PLAY_PINNED=16'd187,
        PROCESS_PLAY_ACTIVITY=16'd188,
        PROCESS_PLAY_RESTORED=16'd189,
        PROCESS_PLAY_FAILED=16'd190,
        PROCESS_PLAY_NEXT=16'd191,
        PROCESS_HEAD=16'd192,
        PROCESS_HEAD_LOADED=16'd193,
        PROCESS_HEAD_RECORD=16'd194,
        PROCESS_HEAD_NEXT=16'd195,
        PROCESS_EMIT=16'd196,
        PROCESS_EMITTED=16'd197,
        PROCESS_RESCAN=16'd198,
        PROCESS_FINISH_CONTEXT=16'd199,
        PROCESS_FINISH_LOADED=16'd200,
        PROCESS_FINISH_NEXT=16'd201,
        PROCESS_FINISHED=16'd202,
        FAIL_DESTINATION=16'd203,
        FAIL_CONTEXT=16'd204,
        FAIL_CONTEXT_LOADED=16'd205,
        FAIL_CONTEXT_NEXT=16'd206,
        RECOVERY=16'd207,
        RECOVERY_SEND=16'd208,
        RECOVERY_SENT=16'd209,
        RECOVERY_ACTIVITY=16'd210,
        RECOVERY_ACTIVITY_LOADED=16'd211,
        RECOVERY_ACTIVITY_NEXT=16'd212,
        BIND_UNIQUE=16'd213,
        BIND_UNIQUE_CHECK=16'd214,
        BIND_COMMIT=16'd215,
        RESERVE_HISTORY=16'd216,
        RESERVE_HISTORY_LOADED=16'd217,
        RESERVE_HISTORY_VIEW=16'd218,
        RESERVE_RECORD=16'd219,
        RESERVE_RECORD_LOADED=16'd220,
        RESERVE_RECORD_VIEW=16'd221,
        IMPORT_RESERVED=16'd222,
        IMPORT_APPEND_CHECK=16'd223,
        IMPORT_APPEND_RESERVED=16'd224,
        PROCESS_RECORD_RESERVED=16'd225,
        PROCESS_RECORD_RESERVATION=16'd226,
        QUERY_OUTPUT=16'd227,
        QUERY_OUTPUT_LOADED=16'd228,
        CONTROL_FAULT_CONTEXT=16'd229,
        CONTROL_FAULT_LOADED=16'd230,
        CONTROL_FAULT_NEXT=16'd231,
        CONTROL_MAINTAINED=16'd232,
        ZERO_WRITE=16'd233,
        ZERO_NEXT=16'd234,
        DERIVE_ENTRY=16'd235,
        DERIVE_GENERATION=16'd236,
        DERIVE_READY=16'd237,
        DERIVE_MAP_COPY=16'd238,
        DERIVE_COPIED=16'd239,
        DERIVE_CHANGED_WORD=16'd240,
        DERIVE_CHANGED_READ=16'd241,
        DERIVE_CHANGED_BITS=16'd242,
        DERIVE_CHANGED_DONE=16'd243,
        DERIVE_NEXT=16'd244,
        ALIGN_DURATION=16'd245,
        GROUP_DESCRIPTION=16'd246,
        GROUP_MAP_GENERATION=16'd247,
        GROUP_MAP_READY=16'd248,
        GROUP_START=16'd249,
        PANIC_POLL=16'd250,
        PANIC_EMIT=16'd251;
    function automatic [63:0] context_address(input [31:0] slot);
        context_address=arena_base+CONTEXT_OFFSET+64'(slot)*128;
    endfunction
    function automatic [63:0] input_address(input [31:0] slot);
        input_address=arena_base+INPUT_OFFSET+64'(slot)*16900;
    endfunction
    function automatic [63:0] output_address(input [31:0] slot);
        output_address=arena_base+OUTPUT_OFFSET+64'(slot)*16900;
    endfunction
    function automatic [63:0] activity_address(input [31:0] slot);
        activity_address=arena_base+ACTIVITY_OFFSET+64'(slot)*16900;
    endfunction
    function automatic [63:0] history_address(input [31:0] slot);
        history_address=arena_base+HISTORY_OFFSET+64'(slot)*96;
    endfunction
    function automatic [63:0] checkpoint_address(input [31:0] slot,input [31:0] ordinal);
        checkpoint_address=arena_base+CHECKPOINT_OFFSET+(64'(slot)*64'(SNAPSHOTS)+64'(ordinal))*16;
    endfunction
    function automatic [63:0] snapshot_address(input [63:0] id);
        snapshot_address=arena_base+SNAP_OFFSET+64'(id[31:0])*16916+16;
    endfunction
    function automatic [63:0] changes_address(input [31:0] slot);
        changes_address=arena_base+CHANGES_OFFSET+64'(slot)*532;
    endfunction
    function automatic [63:0] map_address(input [63:0] id,input [31:0] dst);
        map_address=arena_base+MAP_OFFSET+(64'(id[31:0])*64'(DESTINATIONS)+64'(dst))*532;
    endfunction
    function automatic [63:0] directory_address(input [63:0] id,input [31:0] dst);
        directory_address=arena_base+DIRECTORY_OFFSET+(64'(id[31:0])*64'(DESTINATIONS)+64'(dst))*48;
    endfunction
    function automatic [31:0] checkpoint_slot(input [31:0] head,input [31:0] distance);
        reg [31:0] total;
        begin total=head+distance;checkpoint_slot=total>=32'(SNAPSHOTS) ? total-32'(SNAPSHOTS) : total;end
    endfunction
    function automatic bit expensive(input [23:0] p);
        expensive=p[7:0]==8'hff || (p[7:4]==11 &&
            (p[15:8]==64 || p[15:8]==66 || p[15:8]==69 || p[15:8]==120 || p[15:8]==121 || p[15:8]>=123));
    endfunction
    task automatic fail(input [2:0] error_code);
        begin if(response_error==0) response_error<=error_code;pc<=RESPONSE;end
    endtask
    task automatic memory_fault;
        begin
            prepared<=0;for(integer d=0;d<DESTINATIONS;d=d+1) recovery[d]<=0;
            fail(7);response_error<=7;
        end
    endtask
    task automatic read_word(input [63:0] address,input [15:0] next);
        begin own_address<=address;memory_return<=next;pc<=READ_REQUEST;end
    endtask
    task automatic write_word(input [63:0] address,input [31:0] value,input [15:0] next);
        begin own_address<=address;own_data<=value;memory_return<=next;pc<=WRITE_REQUEST;end
    endtask
    task automatic call_store(input [3:0] code,input [63:0] view,input [63:0] snapshot,
        input [63:0] x,input [63:0] y,input [15:0] next);
        begin store_op<=code;store_view<=view;store_snapshot<=snapshot;store_arg0<=x;store_arg1<=y;
            store_packet<=event_packet;store_length<=event_length;service_return<=next;pc<=STORE_START;end
    endtask
    task automatic call_state(input [2:0] code,input [63:0] current,input [63:0] target,
        input [63:0] map,input [23:0] packet,input [1:0] length,input bit activity,input [15:0] next);
        begin state_op<=code;state_current<=current;state_target<=target;state_map<=map;state_packet<=packet;state_length<=length;
            state_track_changes<=0;state_changes<=0;state_track_activity<=activity;state_activity<=activity_address(context_slot);state_channels<=c[3][15:0];
            state_pairs<=current>=arena_base+OUTPUT_OFFSET && current<arena_base+ACTIVITY_OFFSET ? high_snapshot*32'(DESTINATIONS) : 0;
            state_first<=0;state_count<=4225;service_return<=next;pc<=STATE_START;end
    endtask
    task automatic load_context(input [31:0] slot,input [15:0] next);
        begin context_slot<=slot;cursor<=0;context_return<=next;pc<=CONTEXT_READ;end
    endtask
    task automatic save_context(input [15:0] next);
        begin cursor<=0;context_return<=next;pc<=CONTEXT_WRITE;end
    endtask
    task automatic load_history(input [31:0] slot,input [15:0] next);
        begin input_slot<=slot;cursor<=0;history_return<=next;pc<=HISTORY_READ;end
    endtask
    task automatic save_history(input [15:0] next);
        begin cursor<=0;history_return<=next;pc<=HISTORY_WRITE;end
    endtask
    task automatic copy_words(input [63:0] source,input [63:0] target,input [31:0] count,input [15:0] next);
        begin copy_source<=source;copy_target<=target;copy_count<=count;copy_index<=0;copy_return<=next;pc<=COPY_READ;end
    endtask
    task automatic prepare_snapshot(input [63:0] id,input [31:0] budget,input [15:0] next);
        begin snapshot_id<=id;snapshot_base<=snapshot_address(id);prepare_budget<=budget;
            prepare_destination<=0;prepare_ready<=1;prepare_is_derive<=0;prepare_return<=next;pc<=PREPARE_ENTRY;end
    endtask
    task automatic derive_snapshot(input [63:0] target,input [63:0] source,input [63:0] changes,input [15:0] next);
        begin snapshot_id<=target;snapshot_base<=snapshot_address(target);derive_source_snapshot<=source;derive_changes<=changes;
            prepare_destination<=0;prepare_is_derive<=1;derive_return<=next;pc<=DERIVE_ENTRY;end
    endtask
    task automatic clear_words(input [63:0] target,input [31:0] count,input [15:0] next);
        begin copy_target<=target;copy_count<=count;copy_index<=0;copy_return<=next;pc<=ZERO_WRITE;end
    endtask
    task automatic state_at_fail(input [2:0] error_code);
        begin if(running) begin last_error<=error_code;pc<=state_at_return;end else fail(error_code);end
    endtask
    task automatic aligned_duration(input [63:0] duration,input [15:0] next);
        begin alignment_value<=duration;alignment_remainder<=0;alignment_bit<=63;alignment_return<=next;pc<=ALIGN_DURATION;end
    endtask
    task automatic checkpoint_read(input [31:0] ordinal,input [15:0] next);
        begin checkpoint_index<=ordinal;checkpoint_word<=0;checkpoint_return<=next;pc<=CHECKPOINT_READ;end
    endtask
    task automatic checkpoint_write(input [31:0] ordinal,input [31:0] order,input [63:0] id,input [15:0] next);
        begin checkpoint_index<=ordinal;checkpoint_order<=order;checkpoint_snapshot<=id;
            checkpoint_word<=0;checkpoint_return<=next;pc<=CHECKPOINT_WRITE;end
    endtask
    task automatic lower_bound(input [63:0] frame,input [15:0] next);
        begin query_frame<=frame;low<=0;high<=last_events;search_return<=next;pc<=SEARCH;end
    endtask
    task automatic state_at(input [63:0] frame,input [15:0] next);
        begin query_frame<=frame;state_at_return<=next;pc<=STATE_AT;end
    endtask
    task automatic recovery_all(input [15:0] next);
        begin recovery_destination<=0;recovery_offset<=0;recovery_return<=next;pc<=RECOVERY;end
    endtask
    task automatic reserve_capture(input [15:0] next);
        begin reserved_blocks<=2;reserved_snapshots<=2;reservation_scan<=0;reservation_return<=next;pc<=RESERVE_HISTORY;end
    endtask
    always @(posedge clk) begin : execution
        reg [63:0] x,y,z;
        reg [31:0] value;
        reg [2:0] validation;
        if(reset) begin
            for(integer d=0;d<DESTINATIONS;d=d+1) begin recovery[d]<=0;owners[d]<=0;end
            pc<=IDLE;prepared<=0;running<=0;seen_quantum<=0;op<=0;
            response_error<=0;response_handle<=0;response_frame<=0;response_state<=0;response_count<=0;
            response_packet<=0;response_length<=0;packet_destination<=0;packet_offset<=0;
            own_address<=0;own_data<=0;data_word<=0;high_snapshot<=0;
            context_slot<=0;input_slot<=0;destination<=0;cursor<=0;scan<=0;index<=0;
            transfer_events<=32'hffffffff;transfer_cells<=32'hffffffff;transfer_metadata<=32'hffffffff;publication_bindings<=32'hffffffff;
            next_frame<=0;start_frame<=0;frames<=0;event_base<=0;event_count<=0;event_index<=0;
            a0<=0;a1<=0;a2<=0;a3<=0;a4<=0;event_input<=0;event_offset<=0;event_packet<=0;event_length<=0;
            store_op<=0;store_view<=0;store_snapshot<=0;store_arg0<=0;store_arg1<=0;store_packet<=0;store_length<=0;
            state_track_changes<=0;state_changes<=0;prepare_is_derive<=0;derive_return<=0;
            derive_source_snapshot<=0;derive_base_snapshot<=0;derive_changes<=0;next_checkpoint<=0;changed_bits<=0;change_word_index<=0;
            state_op<=0;state_current<=0;state_target<=0;state_map<=0;state_activity<=0;state_track_activity<=0;
            state_channels<=0;state_packet<=0;state_length<=0;state_first<=0;state_count<=0;state_pairs<=0;
            last_error<=0;last_handle<=0;last_state<=0;last_frame<=0;last_initial<=0;last_events<=0;last_blocks<=0;last_owner<=0;last_references<=0;last_status<=0;last_order<=0;last_packet<=0;last_length<=0;
            service_return<=0;memory_return<=0;context_return<=0;history_return<=0;copy_return<=0;prepare_return<=0;
            checkpoint_return<=0;search_return<=0;state_at_return<=0;checkpoint_index<=0;checkpoint_word<=0;checkpoint_order<=0;checkpoint_snapshot<=0;
            snapshot_id<=0;snapshot_base<=0;prepare_map<=0;prepare_entry<=0;prepare_destination<=0;prepare_cursor<=0;prepare_budget<=0;prepare_amount<=0;prepare_ready<=0;
            copy_source<=0;copy_target<=0;copy_count<=0;copy_index<=0;
            low<=0;high<=0;middle<=0;query_frame<=0;range_index<=0;range_order<=0;replay_index<=0;
            range_start<=0;range_end<=0;range_view<=0;range_input<=0;range_return<=0;
            best_frame<=0;best_slot<=0;best_packet<=0;best_length<=0;found<=0;established<=0;
            target_count<=0;target_index<=0;target_list<=0;recovery_destination<=0;recovery_offset<=0;recovery_return<=0;initial_snapshot<=0;
        end else case(pc)
            IDLE: if(command_valid) begin
                op<=command_op;a0<=command_arg0;a1<=command_arg1;a2<=command_arg2;a3<=command_arg3;a4<=command_arg4;
                response_error<=0;response_count<=0;response_length<=0;last_error<=0;pc<=DISPATCH;
            end
            RESPONSE: if(response_ready) pc<=IDLE;
            READ_REQUEST: if(own_context) begin context_read_data<=context_words[context_index];pc<=READ_WAIT;end
                else if(memory_ready) pc<=READ_WAIT;
            READ_WAIT: if(own_context) begin data_word<=context_read_data;pc<=memory_return;end
                else if(memory_response_valid) begin
                if(memory_response_error) memory_fault();
                else begin data_word<=memory_read_data;pc<=memory_return;end
            end
            WRITE_REQUEST: if(own_context) begin context_words[context_index]<=own_data;pc<=WRITE_WAIT;end
                else if(memory_ready) pc<=WRITE_WAIT;
            WRITE_WAIT: if(own_context) pc<=memory_return;
                else if(memory_response_valid) begin
                if(memory_response_error) memory_fault(); else pc<=memory_return;
            end
            STORE_START: if(store_ready) pc<=STORE_WAIT;
            STORE_WAIT: if(store_valid) begin
                last_error<=store_error;last_handle<=store_handle;last_state<=store_state;last_frame<=store_frame;
                last_events<=store_events;last_blocks<=store_blocks;last_owner<=store_owner;last_references<=store_references;last_status<=store_status;last_initial<=store_initial;last_order<=store_order;
                last_packet<=store_record_packet;last_length<=store_record_length;pc<=service_return;
                if(store_error==7) memory_fault();
            end
            STATE_START: if(state_ready) pc<=STATE_WAIT;
            STATE_WAIT: if(state_valid) begin
                last_error<=state_error==5 && (state_op==3 || state_op==4 || state_op==5) ? 3 : state_error;pc<=service_return;
                if(state_error==7) memory_fault();
            end
            CONTEXT_READ: read_word(context_address(context_slot)+64'(cursor)*4,CONTEXT_READ_NEXT);
            CONTEXT_READ_NEXT: begin c[cursor[4:0]]<=data_word;
                if(cursor==31) pc<=context_return;else begin cursor<=cursor+1;pc<=CONTEXT_READ;end end
            CONTEXT_WRITE: write_word(context_address(context_slot)+64'(cursor)*4,c[cursor[4:0]],CONTEXT_WRITE_NEXT);
            CONTEXT_WRITE_NEXT: if(cursor==31) pc<=context_return;else begin cursor<=cursor+1;pc<=CONTEXT_WRITE;end
            HISTORY_READ: read_word(history_address(input_slot)+64'(cursor)*4,HISTORY_READ_NEXT);
            HISTORY_READ_NEXT: begin h[cursor[4:0]]<=data_word;
                if(cursor==23) pc<=history_return;else begin cursor<=cursor+1;pc<=HISTORY_READ;end end
            HISTORY_WRITE: write_word(history_address(input_slot)+64'(cursor)*4,h[cursor[4:0]],HISTORY_WRITE_NEXT);
            HISTORY_WRITE_NEXT: if(cursor==23) pc<=history_return;else begin cursor<=cursor+1;pc<=HISTORY_WRITE;end
            COPY_READ: if(copy_index==copy_count) pc<=copy_return;else read_word(copy_source+64'(copy_index)*4,COPY_WRITE);
            COPY_WRITE: write_word(copy_target+64'(copy_index)*4,data_word,COPY_NEXT);
            COPY_NEXT: begin copy_index<=copy_index+1;pc<=COPY_READ;end
            PREPARE_ENTRY: begin
                if(snapshot_id[31:0]>=high_snapshot) high_snapshot<=snapshot_id[31:0]+1;
                if(prepare_destination==32'(DESTINATIONS)) pc<=prepare_return;
                else begin prepare_map<=map_address(snapshot_id,prepare_destination);prepare_entry<=directory_address(snapshot_id,prepare_destination);
                    read_word(directory_address(snapshot_id,prepare_destination),PREPARE_VALID);end
            end
            PREPARE_VALID: if(data_word==0) begin prepare_cursor<=0;cursor<=0;pc<=PREPARE_CREATE;end
                else read_word(prepare_entry+40,PREPARE_GENERATION);
            PREPARE_GENERATION: if(data_word!=snapshot_id[63:32]) begin prepare_cursor<=0;cursor<=0;pc<=PREPARE_CREATE;end
                else read_word(prepare_entry+44,PREPARE_POSITION);
            PREPARE_POSITION: begin prepare_cursor<=data_word;pc<=PREPARE_COMPARE;end
            PREPARE_CREATE: begin
                x=output_address(prepare_destination);y=snapshot_base-16;
                case(cursor)
                    0:value=1;2:value=snapshot_base[31:0];3:value=snapshot_base[63:32];
                    4:value=x[31:0];5:value=x[63:32];6:value=prepare_map[31:0];7:value=prepare_map[63:32];
                    8:value=y[31:0];9:value=y[63:32];10:value=snapshot_id[63:32];default:value=0;
                endcase
                write_word(prepare_entry+64'(cursor)*4,value,PREPARE_CREATE_NEXT);
            end
            PREPARE_CREATE_NEXT: if(cursor==11) pc<=prepare_is_derive ? DERIVE_MAP_COPY : PREPARE_COMPARE;else begin cursor<=cursor+1;pc<=PREPARE_CREATE;end
            PREPARE_COMPARE: if(prepare_cursor==4225) begin prepare_destination<=prepare_destination+1;pc<=PREPARE_ENTRY;end
                else if(prepare_budget==0) begin prepare_ready<=0;pc<=prepare_return;end
                else begin
                    value=4225-prepare_cursor;if(value>prepare_budget) value=prepare_budget;
                    prepare_amount<=value;state_op<=2;state_current<=output_address(prepare_destination);
                    state_target<=snapshot_base;state_map<=prepare_map;state_first<=13'(prepare_cursor);state_count<=13'(value);
                    state_track_activity<=0;state_pairs<=0;service_return<=PREPARE_COMPARED;pc<=STATE_START;
                end
            PREPARE_COMPARED: if(last_error!=0) fail(last_error);else begin
                prepare_cursor<=prepare_cursor+prepare_amount;prepare_budget<=prepare_budget-prepare_amount;
                write_word(prepare_entry+44,prepare_cursor+prepare_amount,PREPARE_COMPARE);
            end
            CHECKPOINT_READ: read_word(checkpoint_address(input_slot,checkpoint_index)+64'(checkpoint_word)*4,CHECKPOINT_READ_NEXT);
            CHECKPOINT_READ_NEXT: begin
                if(checkpoint_word==0) checkpoint_order<=data_word;
                else if(checkpoint_word==1) checkpoint_snapshot[31:0]<=data_word;
                else checkpoint_snapshot[63:32]<=data_word;
                if(checkpoint_word==2) pc<=checkpoint_return;else begin checkpoint_word<=checkpoint_word+1;pc<=CHECKPOINT_READ;end
            end
            CHECKPOINT_WRITE: write_word(checkpoint_address(input_slot,checkpoint_index)+64'(checkpoint_word)*4,
                checkpoint_word==0 ? checkpoint_order : checkpoint_word==1 ? checkpoint_snapshot[31:0] : checkpoint_snapshot[63:32],CHECKPOINT_WRITE_NEXT);
            CHECKPOINT_WRITE_NEXT: if(checkpoint_word==2) pc<=checkpoint_return;else begin checkpoint_word<=checkpoint_word+1;pc<=CHECKPOINT_WRITE;end
            SEARCH: if(low==high) begin range_index<=low;pc<=search_return;end
                else begin middle<=low+(high-low)/2;call_store(10,{h[1],h[0]},0,{32'b0,low+(high-low)/2},0,SEARCH_COMPARE);end
            SEARCH_COMPARE: if(last_error!=0) fail(last_error);
                else begin if(last_frame<query_frame) low<=middle+1;else high<=middle;pc<=SEARCH;end
            DISPATCH: if(op==0 && quantum_frames==0) fail(4);
                else if(op==0) begin prepared<=0;running<=0;seen_quantum<=0;call_store(0,0,0,0,0,BOOT_CONTEXTS);end
                else if(!prepared && op==17) begin
                    if(a0[63:32]!=0) fail(1);else begin recovery_destination<=0;recovery_offset<=a0[31:0];pc<=PANIC_POLL;end end
                else if(!prepared) fail(4);
                else case(op)
                    1,2,5,6,7,11,21: if(a0>=64'(LOOPS) || (op==6 && a1>=64'(LOOPS))) fail(4);else load_context(a0[31:0],CONTROL_CONTEXT);
                    3: if(a1!={32'b0,quantum_frames} || a0+a1<a0 || (seen_quantum && a0!=next_frame) || a3>64'(INPUTS)*64'(INPUT_EVENTS)) fail(1);
                        else begin start_frame<=a0;frames<=a1;event_base<=a2;event_count<=a3[31:0];scan<=0;pc<=VALIDATE_INPUTS_CLEAR;end
                    4,15: if(a0>=64'(INPUTS) || (op==4 && (a4==0 || a4>64'(LOOPS)))) fail(4);
                        else if(a1>=a2) fail(1);
                        else if(transfer_metadata==0 || (op==4 && 64'(publication_bindings)<a4)) fail(5);
                        else begin range_input<=a0[31:0];range_start<=a1;range_end<=a2;target_list<=a3;target_count<=a4[31:0];
                            load_history(a0[31:0],CONTROL_RANGE);end
                    8: if(transfer_metadata==0) fail(5);else reserve_capture(IMPORT_RESERVED);
                    9: if(transfer_events==0) fail(5);else begin event_packet<=a2[23:0];event_length<=a2[25:24];call_store(12,a0,0,0,0,IMPORT_APPEND_CHECK);end
                    10: call_store(12,a0,0,0,0,IMPORT_FINISH);
                    12: if(transfer_events==0) fail(5);else call_store(10,a0,0,a1,0,EXPORT_READ);
                    13: if(transfer_metadata==0) fail(5);else call_store(9,a0,0,0,0,CONTROL_RELEASED);
                    14: if(a0>=64'(DESTINATIONS)) fail(4);
                        else if(a2[63:32]!=0) fail(1);
                        else if(packet_error(a1[23:0],a1[25:24])!=0) fail(packet_error(a1[23:0],a1[25:24]));else if(recovery[a0[31:0]]!=32) fail(5);else begin packet_destination<=a0[31:0];packet_offset<=a2[31:0];
                        call_state(5,output_address(a0[31:0]),0,0,a1[23:0],a1[25:24],0,CONTROL_SENT);end
                    16: if(seen_quantum || a0>=64'(INPUTS)) fail(5);else load_history(a0[31:0],CONTROL_RETENTION);
                    17: if(a0[63:32]!=0) fail(1);else begin recovery_all(RESPONSE);recovery_offset<=a0[31:0];end
                    18: begin scan<=0;pc<=CONTROL_FAULT;end
                    19: if(a0==0 && a1<64'(INPUTS)) begin response_state<=input_address(a1[31:0]);pc<=RESPONSE;end
                        else if(a0==1 && a1<64'(DESTINATIONS)) begin if(recovery[a1[31:0]]!=32) fail(5);else begin scan<=0;pc<=QUERY_OUTPUT;end end else fail(4);
                    22: if(a1==0 || a1>64'(LOOPS)) fail(4);
                        else if(publication_bindings<a1[31:0] || free_retirements<a1[31:0]*2) fail(5);
                        else begin range_view<=0;range_start<=0;range_end<=0;target_list<=a0;target_count<=a1[31:0];target_index<=0;target_seen<=0;pc<=GRAB_VALIDATE;end
                    23: if(a2==0 || a2>64'(LOOPS)) fail(4);
                        else if(publication_bindings<a2[31:0] || free_retirements<a2[31:0]*2) fail(5);
                        else begin range_view<=a0;target_list<=a1;target_count<=a2[31:0];target_index<=0;target_seen<=0;
                            call_store(12,a0,0,0,0,GROUP_DESCRIPTION);end
                    20: call_store(11,0,0,a0>64 ? 64 : a0,0,CONTROL_MAINTAINED);
                    default: fail(4);
                endcase
            BOOT_CONTEXTS: begin scan<=0;pc<=BOOT_CONTEXT;end
            BOOT_CONTEXT: if(scan==32'(LOOPS)*32) begin scan<=0;pc<=BOOT_DIRECTORY;end
                else write_word(arena_base+CONTEXT_OFFSET+64'(scan)*4,0,BOOT_CONTEXT_NEXT);
            BOOT_CONTEXT_NEXT: begin scan<=scan+1;pc<=BOOT_CONTEXT;end
            BOOT_DIRECTORY: if(scan==32'(SNAPSHOTS)*32'(DESTINATIONS)) begin
                    scan<=0;high_snapshot<=0;pc<=BOOT_OUTPUT;
                end else write_word(arena_base+DIRECTORY_OFFSET+64'(scan)*48,0,BOOT_DIRECTORY_NEXT);
            BOOT_DIRECTORY_NEXT: begin scan<=scan+1;pc<=BOOT_DIRECTORY;end
            BOOT_OUTPUT: if(scan==32'(DESTINATIONS)) begin scan<=0;pc<=BOOT_INPUT;end
                else begin recovery[scan]<=32;owners[scan]<=0;call_state(0,output_address(scan),0,0,0,0,0,BOOT_OUTPUT_NEXT);end
            BOOT_OUTPUT_NEXT: begin scan<=scan+1;pc<=BOOT_OUTPUT;end
            BOOT_INPUT: if(scan==32'(INPUTS)) begin scan<=0;pc<=BOOT_ACTIVITY;end
                else call_state(0,input_address(scan),0,0,0,0,0,BOOT_INPUT_NEXT);
            BOOT_INPUT_NEXT: begin scan<=scan+1;pc<=BOOT_INPUT;end
            BOOT_ACTIVITY: if(scan==32'(LOOPS)) call_store(1,0,0,0,0,BOOT_SNAPSHOT);
                else call_state(0,activity_address(scan),0,0,0,0,0,BOOT_ACTIVITY_NEXT);
            BOOT_ACTIVITY_NEXT: begin scan<=scan+1;pc<=BOOT_ACTIVITY;end
            BOOT_SNAPSHOT: if(last_error!=0) fail(last_error);else begin initial_snapshot<=last_handle;prepare_snapshot(last_handle,32'hffffffff,BOOT_HISTORIES);end
            BOOT_HISTORIES: begin scan<=0;pc<=BOOT_HISTORY;end
            BOOT_HISTORY: if(scan==32'(INPUTS)) begin
                    prepared<=1;running<=0;next_frame<=0;transfer_events<=32'hffffffff;transfer_cells<=32'hffffffff;
                    transfer_metadata<=32'hffffffff;publication_bindings<=32'hffffffff;pc<=RESPONSE;
                end else begin input_slot<=scan;call_store(4,0,initial_snapshot,64'(MAX_EVENTS)+64'(BLOCK_EVENTS)*2,0,BOOT_HISTORY_WRITER);end
            BOOT_HISTORY_WRITER: if(last_error!=0) fail(last_error);else begin
                for(integer j=0;j<24;j=j+1) h[j]<=0;
                h[0]<=last_handle[31:0];h[1]<=last_handle[63:32];h[5]<=1;h[12]<=30*48000;
                write_word(arena_base+64'(last_handle[31:0])*64+60,1,BOOT_HISTORY_OWNED);
            end
            BOOT_HISTORY_OWNED: call_store(2,0,initial_snapshot,0,0,BOOT_HISTORY_CHECKPOINT);
            BOOT_HISTORY_CHECKPOINT: checkpoint_write(0,0,initial_snapshot,BOOT_HISTORY_SAVED);
            BOOT_HISTORY_SAVED: clear_words(changes_address(input_slot),133,BOOT_HISTORY_MASK_ZERO);
            BOOT_HISTORY_MASK_ZERO: save_history(BOOT_HISTORY_NEXT);
            BOOT_HISTORY_NEXT: begin scan<=scan+1;pc<=BOOT_HISTORY;end
            CONTROL_CONTEXT: begin
                if(op==1) begin
                    if(c[0]!=0) fail(5);
                    else if(a1[63:32]==0 || a2>=64'(INPUTS) || a3>=64'(DESTINATIONS) || a4==0 || a4>65535) fail(4);
                    else begin scan<=0;pc<=BIND_UNIQUE;end
                end else if(c[0]==0) fail(4);
                else if(op==2) begin
                    if(a1>2) fail(2);else begin c[17]<=a1[31:0];c[18]<=a2[31:0];c[19]<=a2[63:32];
                        c[20]<=a3[31:0];c[21]<=a3[63:32];c[22]<=a4[31:0];save_context(RESPONSE);end
                end else if(op==21) begin
                    if(a1==0 || a1>64'(MAX_EVENTS)) fail(3);
                    else if(c[16]!=0 || {c[9],c[8]}!=0) fail(5);else begin c[31]<=a1[31:0];save_context(RESPONSE);end
                end else if(op==11) begin if(transfer_metadata==0) fail(5);else call_store(8,{c[7],c[6]},0,0,0,CONTROL_EXPORTED);end
                else if(op==6) begin range_view<={c[7],c[6]};load_context(a1[31:0],CONTROL_DUPLICATE);end
                else if(op==5) begin range_view<=a1;pc<=CONTROL_REPLACE;end
                else if(op==7) begin range_view<=0;pc<=CONTROL_REPLACE;end
            end
            CONTROL_DUPLICATE: if(a1>=64'(LOOPS) || c[0]==0 || range_view==0) fail(4);else pc<=CONTROL_REPLACE;
            CONTROL_EXPORTED: if(last_error!=0) fail(last_error);else begin
                transfer_metadata<=transfer_metadata-1;response_handle<=last_handle;range_view<=last_handle;call_store(12,last_handle,0,0,0,CONTROL_EXPORT_DESCRIPTION);end
            CONTROL_EXPORT_DESCRIPTION: begin response_frame<=last_frame;response_count<=last_events;
                response_state<=snapshot_address(last_initial);pc<=RESPONSE;end
            CONTROL_RELEASED: if(last_error!=0) fail(last_error);else begin transfer_metadata<=transfer_metadata-1;pc<=RESPONSE;end
            EXPORT_READ: if(last_error!=0) fail(last_error);else begin transfer_events<=transfer_events-1;
                response_frame<=last_frame;response_count<=last_order;response_packet<=last_packet;response_length<=last_length;pc<=RESPONSE;end
            CONTROL_SENT: if(last_error!=0) begin failed_destination<=a0[31:0];
                if(response_error==0) response_error<=last_error;failure_saved_scan<=0;failure_return<=RESPONSE;pc<=FAIL_DESTINATION;end else pc<=RESPONSE;
            CONTROL_RETENTION: begin h[12]<=a1[31:0];h[13]<=a1[63:32];save_history(RESPONSE);end
            CONTROL_FAULT: if(scan==32'(DESTINATIONS)) begin scan<=0;pc<=CONTROL_FAULT_CONTEXT;end else begin recovery[scan]<=0;scan<=scan+1;end
            IMPORT_VALIDATE: if(scan==4225) call_store(1,0,0,a0,1,IMPORT_SNAPSHOT);
                else read_word(a0+64'(scan)*4,IMPORT_VALIDATED);
            IMPORT_VALIDATED: begin
                validation=0;
                if(scan==4224) begin if(data_word>1) validation=1;end
                else if(index<128) begin
                    if(data_word[31:24]!=0 || data_word[7] || data_word[15:10]!=0 || data_word[23:16]>128) validation=1;
                end else if(index<256) begin
                    if(data_word>128) validation=1;
                    if((index==134 || index==166 || index>=224 && index<=229) && data_word!=128) validation=2;
                end else if(index==259) begin if(data_word>16383) validation=1;end
                else if(index==261) begin if(data_word>1) validation=1;end
                else if(index>=262) begin if(data_word!=0) validation=2;end
                else if(data_word>128) validation=1;
                if(validation!=0) fail(validation);
                else begin scan<=scan+1;index<=index==263 ? 0 : index+1;pc<=IMPORT_VALIDATE;end
            end
            IMPORT_SNAPSHOT: if(last_error!=0) fail(last_error);else begin snapshot_id<=last_handle;
                call_store(4,0,last_handle,0,0,IMPORT_WRITER);end
            IMPORT_WRITER: if(last_error!=0) begin call_store(3,0,snapshot_id,0,0,IMPORT_FAILED);end
                else begin range_view<=last_handle;call_store(3,0,snapshot_id,0,0,IMPORT_BEGUN);end
            IMPORT_FAILED: fail(3);
            IMPORT_BEGUN: begin transfer_metadata<=transfer_metadata-1;response_handle<=range_view;pc<=RESPONSE;end
            IMPORT_APPEND: call_store(5,a0,0,a1,0,IMPORT_APPENDED);
            IMPORT_APPENDED: if(last_error!=0) fail(last_error);else begin transfer_events<=transfer_events-1;pc<=RESPONSE;end
            IMPORT_FINISH: if(last_owner!=0) fail(4);else if(last_error!=0) fail(last_error);else if(last_status==1) call_store(6,a0,0,a1,0,IMPORT_FINISHED);
                else if(last_status==2 && last_frame==a1) pc<=IMPORT_FINISHED;else fail(4);
            IMPORT_FINISHED: if(last_error!=0) fail(last_error);else begin
                value=a2[31:0];if(value>256) value=256;if(value>transfer_cells) value=transfer_cells;
                prepare_snapshot(last_initial,value,IMPORT_PREPARED);
            end
            IMPORT_PREPARED: begin
                response_handle<=a0;response_count<=prepare_ready ? 1 : 0;
                if(seen_quantum) transfer_cells<=transfer_cells-(a2[31:0]<transfer_cells ? a2[31:0] : transfer_cells)+prepare_budget;
                pc<=RESPONSE;
            end
            CONTROL_REPLACE: if((range_view!=0 && recovery[c[2]]!=32) || publication_bindings==0 || free_retirements<2) fail(5);
                else if(range_view==0) begin last_frame<=0;pc<=CONTROL_REPLACE_RETAINED;end
                else call_store(12,range_view,0,0,0,CONTROL_REPLACE_DESCRIPTION);
            CONTROL_REPLACE_DESCRIPTION: if(last_error!=0) fail(last_error);
                else if(last_status!=2 || last_frame==0) fail(4);
                else aligned_duration(last_frame,CONTROL_REPLACE_ALIGNED);
            CONTROL_REPLACE_ALIGNED: read_word(directory_address(last_initial,c[2])+40,CONTROL_REPLACE_GENERATION);
            CONTROL_REPLACE_GENERATION: if(data_word!=last_initial[63:32]) fail(5);
                else read_word(directory_address(last_initial,c[2])+44,CONTROL_REPLACE_READY);
            CONTROL_REPLACE_READY: if(data_word!=4225) fail(5);else begin
                response_frame<=last_frame;call_store(8,range_view,0,0,0,CONTROL_REPLACE_RETAINED);end
            CONTROL_REPLACE_RETAINED: if(last_error!=0) fail(last_error);
                else if({c[7],c[6]}!=0) call_store(9,{c[7],c[6]},0,0,0,CONTROL_REPLACE_OLD);
                else pc<=CONTROL_REPLACE_OLD;
            CONTROL_REPLACE_OLD: if(last_error!=0) fail(last_error);
                else if({c[9],c[8]}!=0) call_store(9,{c[9],c[8]},0,0,0,CONTROL_REPLACE_WRITER);
                else pc<=CONTROL_REPLACE_WRITER;
            CONTROL_REPLACE_WRITER: if(last_error!=0) fail(last_error);else begin
                c[6]<=range_view[31:0];c[7]<=range_view[63:32];c[8]<=0;c[9]<=0;c[13]<=range_view==0 ? 0 : 2;
                c[17]<=0;c[18]<=0;c[19]<=0;
                if(c[16]==1) c[16]<=0;
                if(range_view==0) begin c[20]<=0;c[21]<=0;response_frame<=0;end
                else begin c[20]<=response_frame[31:0];c[21]<=response_frame[63:32];end
                publication_bindings<=publication_bindings-1;save_context(RESPONSE);
            end
            CONTROL_RANGE: if(op==4 && free_retirements<target_count*2) fail(5);
                else if((op==4 && range_start==range_end) || range_start<{h[9],h[8]} || range_end>{h[11],h[10]}) fail(1);
                else if(op==4) aligned_duration(range_end-range_start,GRAB_ALIGNED);
                else state_at(range_start,RANGE_STATE);
            GRAB_ALIGNED: begin target_index<=0;target_seen<=0;pc<=GRAB_VALIDATE;end
            GRAB_VALIDATE: if(target_index==target_count) begin
                    if(op==22 || op==23) pc<=GROUP_START;else state_at(range_start,RANGE_STATE);end
                else read_word(target_list+64'(target_index)*4,GRAB_VALIDATE_SLOT);
            GRAB_VALIDATE_SLOT: if(data_word>=32'(LOOPS) || target_seen[data_word[6:0]]) fail(4);
                else begin target_seen[data_word[6:0]]<=1;load_context(data_word,GRAB_VALIDATE_CONTEXT);end
            GRAB_VALIDATE_CONTEXT: if(c[0]==0) fail(4);else if(op!=22 && recovery[c[2]]!=32) fail(5);
                else if(op==23) read_word(directory_address(snapshot_id,c[2])+40,GROUP_MAP_GENERATION);
                else begin target_index<=target_index+1;pc<=GRAB_VALIDATE;end
            RANGE_STATE: begin checkpoint_snapshot<=response_handle;
                call_store(7,{h[1],h[0]},response_handle,range_start,range_end,RANGE_CREATED);end
            RANGE_CREATED: if(last_error!=0) begin range_failure<=last_error;call_store(3,0,checkpoint_snapshot,0,0,RANGE_FAILED);end
                else begin range_view<=last_handle;call_store(3,0,checkpoint_snapshot,0,0,RANGE_OWNED);end
            RANGE_FAILED: fail(range_failure);
            RANGE_OWNED: begin
                transfer_metadata<=transfer_metadata-1;response_handle<=range_view;response_frame<=range_end-range_start;
                response_state<=snapshot_address(checkpoint_snapshot);response_count<=last_events;
                if(op==15) pc<=RESPONSE;else begin target_index<=1;pc<=GRAB_RETAIN;end
            end
            GRAB_RETAIN: if(target_index==target_count) begin target_index<=0;pc<=GRAB_PUBLISH;end
                else call_store(8,range_view,0,0,0,GRAB_RETAINED);
            GRAB_RETAINED: if(last_error!=0) fail(last_error);else begin target_index<=target_index+1;pc<=GRAB_RETAIN;end
            GRAB_PUBLISH: if(target_index==target_count) begin publication_bindings<=publication_bindings-target_count;pc<=RESPONSE;end
                else read_word(target_list+64'(target_index)*4,GRAB_PUBLISH_SLOT);
            GRAB_PUBLISH_SLOT: load_context(data_word,GRAB_PUBLISH_CONTEXT);
            GRAB_PUBLISH_CONTEXT: if({c[7],c[6]}!=0) call_store(9,{c[7],c[6]},0,0,0,GRAB_PUBLISH_OLD);
                else pc<=GRAB_PUBLISH_OLD;
            GRAB_PUBLISH_OLD: if(last_error!=0) fail(last_error);
                else if({c[9],c[8]}!=0) call_store(9,{c[9],c[8]},0,0,0,GRAB_PUBLISH_WRITER);else pc<=GRAB_PUBLISH_WRITER;
            GRAB_PUBLISH_WRITER: if(last_error!=0) fail(last_error);else begin
                c[6]<=range_view[31:0];c[7]<=range_view[63:32];c[8]<=0;c[9]<=0;c[13]<=range_view==0 ? 0 : 2;
                c[17]<=0;c[18]<=0;c[19]<=0;
                x=range_end-range_start;c[20]<=x[31:0];c[21]<=x[63:32];if(c[16]==1) c[16]<=0;
                save_context(GRAB_PUBLISH_NEXT);
            end
            GRAB_PUBLISH_NEXT: begin target_index<=target_index+1;pc<=GRAB_PUBLISH;end
            STATE_AT: if(query_frame<{h[9],h[8]} || query_frame>{h[11],h[10]}) state_at_fail(1);
                else call_store(12,{h[1],h[0]},0,0,0,STATE_AT_DESCRIPTION);
            STATE_AT_DESCRIPTION: if(last_error!=0) state_at_fail(last_error);else lower_bound(query_frame,STATE_AT_INDEX);
            STATE_AT_INDEX: if(range_index<last_events) call_store(10,{h[1],h[0]},0,{32'b0,range_index},0,STATE_AT_ORDER);
                else begin last_order<=h[3];pc<=STATE_AT_ORDER;end
            STATE_AT_ORDER: begin range_order<=last_order;index<=h[5]-1;
                checkpoint_read(checkpoint_slot(h[4],h[5]-1),STATE_AT_CHECKPOINT);end
            STATE_AT_CHECKPOINT: if(checkpoint_order>range_order && index!=0) begin index<=index-1;
                    checkpoint_read(checkpoint_slot(h[4],index-1),STATE_AT_CHECKPOINT);end
                else if(checkpoint_order>range_order || range_order-checkpoint_order>31) state_at_fail(4);
                else if(checkpoint_order==range_order) call_store(2,0,checkpoint_snapshot,0,0,STATE_AT_RETAINED);
                else begin derive_base_snapshot<=checkpoint_snapshot;replay_index<=checkpoint_order-h[6];
                    call_store(1,0,0,snapshot_address(checkpoint_snapshot),1,STATE_AT_COPIED);end
            STATE_AT_RETAINED: if(last_error!=0) state_at_fail(last_error);else begin response_handle<=checkpoint_snapshot;pc<=state_at_return;end
            STATE_AT_COPIED: if(last_error!=0) state_at_fail(last_error);else begin
                checkpoint_snapshot<=last_handle;snapshot_base<=last_state;
                clear_words(arena_base+SCRATCH_CHANGES_OFFSET,133,STATE_AT_REPLAY);
            end
            STATE_AT_REPLAY: if(replay_index==range_index) derive_snapshot(checkpoint_snapshot,derive_base_snapshot,arena_base+SCRATCH_CHANGES_OFFSET,STATE_AT_PREPARED);
                else call_store(10,{h[1],h[0]},0,{32'b0,replay_index},0,STATE_AT_EVENT);
            STATE_AT_EVENT: if(last_error!=0 || last_length==0) state_at_fail(4);
                else begin call_state(1,snapshot_base,0,0,last_packet,last_length,0,STATE_AT_APPLIED);
                    state_track_changes<=1;state_changes<=arena_base+SCRATCH_CHANGES_OFFSET;end
            STATE_AT_APPLIED: if(last_error!=0) state_at_fail(last_error);else begin replay_index<=replay_index+1;pc<=STATE_AT_REPLAY;end
            STATE_AT_PREPARED: begin response_handle<=checkpoint_snapshot;pc<=state_at_return;end
            VALIDATE_INPUTS_CLEAR: if(scan==32'(INPUTS)) begin event_index<=0;last_input_offset<=0;pc<=VALIDATE_INPUT;end
                else begin density[scan]<=0;scan<=scan+1;end
            VALIDATE_INPUT: if(event_index==event_count) begin scan<=0;pc<=VALIDATE_CONTEXT;end
                else begin cursor<=0;event_return<=VALIDATE_EVENT;pc<=EVENT_READ;end
            EVENT_READ: read_word(event_base+64'(event_index)*16+64'(cursor)*4,EVENT_READ_NEXT);
            EVENT_READ_NEXT: begin
                if(cursor==0) event_input<=data_word;
                else if(cursor==1) event_offset<=data_word;
                else begin event_packet<=data_word[23:0];event_length<=data_word[25:24];end
                if(cursor==2) pc<=event_return;else begin cursor<=cursor+1;pc<=EVENT_READ;end
            end
            VALIDATE_EVENT: if(event_input>=32'(INPUTS) || {32'b0,event_offset}>=frames || event_offset<last_input_offset) fail(1);
                else if(packet_error(event_packet,event_length)!=0) fail(packet_error(event_packet,event_length));
                else if(density[event_input]>=32'(INPUT_EVENTS)) fail(3);
                else begin density[event_input]<=density[event_input]+1;last_input_offset<=event_offset;
                    event_index<=event_index+1;pc<=VALIDATE_INPUT;end
            VALIDATE_CONTEXT: if(scan==32'(LOOPS)) begin packet_offset<=0;recovery_all(PROCESS_RECORDS);end
                else load_context(scan,VALIDATE_CONTEXT_NEXT);
            VALIDATE_CONTEXT_NEXT: if(c[0]!=0 && c[17]==2 && c[23]==0 && recovery[c[2]]==32 && {c[19],c[18]}!=0 &&
                    ({c[11],c[10]}==0 || {c[19],c[18]}!={c[15],c[14]})) fail(2);
                else begin scan<=scan+1;pc<=VALIDATE_CONTEXT;end
            PROCESS_RECORDS: begin running<=1;scan<=0;pc<=PROCESS_RECORD;end
            PROCESS_RECORD: if(scan==32'(LOOPS)) begin scan<=0;pc<=PROCESS_RELEASE_SEED;end
                else load_context(scan,PROCESS_RECORD_CONTEXT);
            PROCESS_RECORD_CONTEXT: if(c[0]==0) pc<=PROCESS_RECORD_NEXT;
                else if(c[16]==1 && c[17]!=1 && {c[9],c[8]}!=0) call_store(6,{c[9],c[8]},0,{c[25],c[24]},0,PROCESS_RECORD_FINISHED);
                else pc<=PROCESS_RECORD_BEGIN;
            PROCESS_RECORD_FINISHED: if(last_error!=0) begin
                    if(response_error==0) response_error<=last_error;c[13]<=3;save_context(PROCESS_RECORD_NEXT);
                end else begin snapshot_id<=last_initial;write_word(arena_base+64'(c[8])*64+60,0,PROCESS_RECORD_OWNER_RELEASED);end
            PROCESS_RECORD_OWNER_RELEASED: begin
                    if({c[7],c[6]}!=0) call_store(9,{c[7],c[6]},0,0,0,PROCESS_RECORD_REPLACED);
                    else pc<=PROCESS_RECORD_REPLACED;end
            PROCESS_RECORD_REPLACED: if(last_error!=0) begin
                    c[13]<=3;if(response_error==0) response_error<=last_error;save_context(PROCESS_RECORD_NEXT);
                end else begin
                c[6]<=c[8];c[7]<=c[9];c[8]<=0;c[9]<=0;c[13]<=2;
                prepare_snapshot(snapshot_id,32'hffffffff,PROCESS_RECORD_PREPARED);
            end
            PROCESS_RECORD_PREPARED: save_context(PROCESS_RECORD_BEGIN);
            PROCESS_RECORD_BEGIN: if(c[17]==1 && c[22][0]) begin
                    reservation_saved_scan<=scan;reservation_context<=context_slot;reserve_capture(PROCESS_RECORD_RESERVED);end
                else pc<=PROCESS_RECORD_NEXT;
            PROCESS_RECORD_SEED: if({h[15],h[14]}!=0) begin
                    snapshot_id<={h[15],h[14]};call_store(4,0,{h[15],h[14]},c[31]==0 ? 0 : {32'b0,c[31]},0,PROCESS_RECORD_WRITER);
                end else state_at(start_frame,PROCESS_RECORD_SNAPSHOT);
            PROCESS_RECORD_SNAPSHOT: if(last_error!=0) begin
                    c[13]<=3;if(response_error==0) response_error<=last_error;save_context(PROCESS_RECORD_NEXT);
                end else begin h[14]<=response_handle[31:0];h[15]<=response_handle[63:32];snapshot_id<=response_handle;pc<=PROCESS_RECORD_SEED_PREPARED;end
            PROCESS_RECORD_SEED_PREPARED: save_history(PROCESS_RECORD_SEED_SAVED);
            PROCESS_RECORD_SEED_SAVED: call_store(4,0,snapshot_id,c[31]==0 ? 0 : {32'b0,c[31]},0,PROCESS_RECORD_WRITER);
            PROCESS_RECORD_WRITER: if(last_error!=0) begin
                    c[13]<=3;if(response_error==0) response_error<=last_error;save_context(PROCESS_RECORD_NEXT);
                end else begin pending_writer<=last_handle;write_word(arena_base+64'(last_handle[31:0])*64+60,2,PROCESS_RECORD_WRITER_OWNED);end
            PROCESS_RECORD_WRITER_OWNED: begin
                    if({c[9],c[8]}!=0) call_store(9,{c[9],c[8]},0,0,0,PROCESS_RECORD_WRITER_OLD);else pc<=PROCESS_RECORD_WRITER_OLD;end
            PROCESS_RECORD_WRITER_OLD: begin c[8]<=pending_writer[31:0];c[9]<=pending_writer[63:32];c[24]<=0;c[25]<=0;c[13]<=1;save_context(PROCESS_RECORD_NEXT);end
            PROCESS_RECORD_NEXT: begin scan<=scan+1;pc<=PROCESS_RECORD;end
            PROCESS_RELEASE_SEED: if(scan==32'(INPUTS)) begin event_index<=0;pc<=PROCESS_INPUT;end
                else load_history(scan,PROCESS_RELEASE_SEED_LOADED);
            PROCESS_RELEASE_SEED_LOADED: if({h[15],h[14]}==0) pc<=PROCESS_RELEASE_SEED_NEXT;
                else call_store(3,0,{h[15],h[14]},0,0,PROCESS_RELEASE_SEED_RELEASED);
            PROCESS_RELEASE_SEED_RELEASED: begin h[14]<=0;h[15]<=0;save_history(PROCESS_RELEASE_SEED_NEXT);end
            PROCESS_RELEASE_SEED_NEXT: begin scan<=scan+1;pc<=PROCESS_RELEASE_SEED;end
            PROCESS_INPUT: if(event_index==event_count) begin scan<=0;pc<=PROCESS_ADVANCE_HISTORY;end
                else begin cursor<=0;event_return<=PROCESS_INPUT_EVENT;pc<=EVENT_READ;end
            PROCESS_INPUT_EVENT: load_history(event_input,PROCESS_INPUT_HISTORY);
            PROCESS_INPUT_HISTORY: if(h[7]!=0) pc<=PROCESS_INPUT_STATE;
                else call_store(5,{h[1],h[0]},0,start_frame+64'(event_offset),0,PROCESS_INPUT_CAPTURED);
            PROCESS_INPUT_CAPTURED: if(last_error!=0) begin h[7]<=1;if(response_error==0) response_error<=last_error;pc<=PROCESS_INPUT_STATE;end
                else begin h[3]<=h[3]+1;h[2]<=h[2]+1;pc<=PROCESS_INPUT_STATE;end
            PROCESS_INPUT_STATE: begin call_state(1,input_address(event_input),0,0,event_packet,event_length,0,PROCESS_INPUT_OBSERVED);
                state_track_changes<=1;state_changes<=changes_address(input_slot);end
            PROCESS_INPUT_OBSERVED: if(last_error!=0) fail(last_error);
                else if(h[7]==0 && (h[2]==32 || expensive(event_packet))) begin
                    if(h[5]>=32'(SNAPSHOTS)) begin h[7]<=1;if(response_error==0) response_error<=3;pc<=PROCESS_INPUT_HISTORY_SAVED;end
                    else checkpoint_read(checkpoint_slot(h[4],h[5]-1),PROCESS_INPUT_CHECKPOINT_BASE);
                end else pc<=PROCESS_INPUT_HISTORY_SAVED;
            PROCESS_INPUT_CHECKPOINT_BASE: call_store(1,0,0,input_address(input_slot),1,PROCESS_INPUT_CHECKPOINT);
            PROCESS_INPUT_CHECKPOINT: if(last_error!=0) begin h[7]<=1;if(response_error==0) response_error<=last_error;pc<=PROCESS_INPUT_HISTORY_SAVED;end
                else begin next_checkpoint<=last_handle;
                    derive_snapshot(last_handle,checkpoint_snapshot,changes_address(input_slot),PROCESS_INPUT_CHECKPOINT_PREPARED);end
            PROCESS_INPUT_CHECKPOINT_PREPARED: checkpoint_write(checkpoint_slot(h[4],h[5]),h[3],next_checkpoint,PROCESS_INPUT_CHECKPOINT_WRITTEN);
            PROCESS_INPUT_CHECKPOINT_WRITTEN: begin h[2]<=0;h[5]<=h[5]+1;
                clear_words(changes_address(input_slot),133,PROCESS_INPUT_HISTORY_SAVED);end
            PROCESS_INPUT_HISTORY_SAVED: save_history(PROCESS_INPUT_RECORDS_START);
            PROCESS_INPUT_RECORDS_START: begin scan<=0;pc<=PROCESS_INPUT_RECORD;end
            PROCESS_INPUT_RECORD: if(scan==32'(LOOPS)) begin event_index<=event_index+1;pc<=PROCESS_INPUT;end
                else load_context(scan,PROCESS_INPUT_RECORD_LOADED);
            PROCESS_INPUT_RECORD_LOADED: if(c[0]==0 || c[1]!=event_input || c[13]!=1 || {c[9],c[8]}==0 ||
                    (event_packet[7:0]!=8'hff && !c[3][{1'b0,event_packet[3:0]}])) pc<=PROCESS_INPUT_RECORD_NEXT;
                else call_store(5,{c[9],c[8]},0,{c[25],c[24]}+64'(event_offset),0,PROCESS_INPUT_RECORDED);
            PROCESS_INPUT_RECORDED: if(last_error!=0) begin c[13]<=3;if(response_error==0) response_error<=last_error;save_context(PROCESS_INPUT_RECORD_NEXT);end
                else pc<=PROCESS_INPUT_RECORD_NEXT;
            PROCESS_INPUT_RECORD_NEXT: begin scan<=scan+1;pc<=PROCESS_INPUT_RECORD;end
            PROCESS_ADVANCE_HISTORY: if(scan==32'(INPUTS)) begin scan<=0;pc<=PROCESS_ADVANCE_RECORD;end
                else load_history(scan,PROCESS_ADVANCE_HISTORY_LOADED);
            PROCESS_ADVANCE_HISTORY_LOADED: if(h[7]!=0) pc<=PROCESS_ADVANCE_HISTORY_NEXT;
                else call_store(13,{h[1],h[0]},0,start_frame+frames,0,PROCESS_HISTORY_ADVANCED);
            PROCESS_HISTORY_ADVANCED: if(last_error!=0) begin h[7]<=1;if(response_error==0) response_error<=last_error;save_history(PROCESS_ADVANCE_HISTORY_NEXT);end
                else begin
                    x=start_frame+frames;y={h[13],h[12]};z=x>y ? x-y : 0;
                    h[10]<=x[31:0];h[11]<=x[63:32];h[8]<=z[31:0];h[9]<=z[63:32];
                    call_store(12,{h[1],h[0]},0,0,0,PROCESS_HISTORY_TRIM);
                end
            PROCESS_HISTORY_TRIM: lower_bound({h[9],h[8]},PROCESS_HISTORY_TRIM_INDEX);
            PROCESS_HISTORY_TRIM_INDEX: if(range_index<last_events) call_store(10,{h[1],h[0]},0,{32'b0,range_index},0,PROCESS_HISTORY_TRIM_ORDER);
                else begin last_order<=h[3];pc<=PROCESS_HISTORY_TRIM_ORDER;end
            PROCESS_HISTORY_TRIM_ORDER: begin range_order<=last_order;pc<=PROCESS_HISTORY_TRIM_CHECKPOINT;end
            PROCESS_HISTORY_TRIM_CHECKPOINT: if(h[5]<=1) checkpoint_read(h[4],PROCESS_HISTORY_DISCARD);
                else checkpoint_read(checkpoint_slot(h[4],1),PROCESS_HISTORY_TRIM_SECOND);
            PROCESS_HISTORY_TRIM_SECOND: if(checkpoint_order>range_order) checkpoint_read(h[4],PROCESS_HISTORY_DISCARD);
                else checkpoint_read(h[4],PROCESS_HISTORY_TRIM_OLD);
            PROCESS_HISTORY_TRIM_OLD: call_store(3,0,checkpoint_snapshot,0,0,PROCESS_HISTORY_TRIM_RELEASED);
            PROCESS_HISTORY_TRIM_RELEASED: begin h[4]<=checkpoint_slot(h[4],1);h[5]<=h[5]-1;pc<=PROCESS_HISTORY_TRIM_CHECKPOINT;end
            PROCESS_HISTORY_DISCARD: if(checkpoint_order==h[6]) save_history(PROCESS_ADVANCE_HISTORY_NEXT);
                else begin trim_order<=checkpoint_order;call_store(14,{h[1],h[0]},0,{32'b0,checkpoint_order-h[6]},0,PROCESS_HISTORY_DISCARDED);end
            PROCESS_HISTORY_DISCARDED: if(last_error!=0) fail(last_error);else begin h[6]<=trim_order;save_history(PROCESS_ADVANCE_HISTORY_NEXT);end
            PROCESS_ADVANCE_HISTORY_NEXT: begin scan<=scan+1;pc<=PROCESS_ADVANCE_HISTORY;end
            PROCESS_ADVANCE_RECORD: if(scan==32'(LOOPS)) begin scan<=0;pc<=PROCESS_OWNERS_CLEAR;end
                else load_context(scan,PROCESS_ADVANCE_RECORD_LOADED);
            PROCESS_ADVANCE_RECORD_LOADED: if(c[0]==0 || c[13]!=1 || {c[9],c[8]}==0) pc<=PROCESS_ADVANCE_RECORD_NEXT;
                else call_store(13,{c[9],c[8]},0,{c[25],c[24]}+frames,0,PROCESS_RECORD_ADVANCED);
            PROCESS_RECORD_ADVANCED: begin
                if(last_error!=0) begin c[13]<=3;if(response_error==0) response_error<=last_error;end
                else begin x={c[25],c[24]}+frames;c[24]<=x[31:0];c[25]<=x[63:32];end
                save_context(PROCESS_ADVANCE_RECORD_NEXT);
            end
            PROCESS_ADVANCE_RECORD_NEXT: begin scan<=scan+1;pc<=PROCESS_ADVANCE_RECORD;end
            PROCESS_OWNERS_CLEAR: if(scan==32'(DESTINATIONS)) begin scan<=0;pc<=PROCESS_CLEANUP;end
                else begin owners[scan]<=0;scan<=scan+1;end
            PROCESS_CLEANUP: if(scan==32'(LOOPS)) begin established<=1;scan<=0;pc<=PROCESS_OWNERS;end
                else load_context(scan,PROCESS_CLEANUP_LOADED);
            PROCESS_CLEANUP_LOADED: begin
                c[28]<=0;
                if(c[0]==0 || recovery[c[2]]!=32) save_context(PROCESS_CLEANUP_NEXT);
                else if({c[11],c[10]}!=0 && (c[30]!=0 || c[17]!=2 || {c[19],c[18]}>={c[21],c[20]} ||
                    {c[19],c[18]}==0 || {c[11],c[10]}!={c[7],c[6]})) begin
                    c[30]<=1;packet_destination<=c[2];packet_offset<=0;
                    call_state(4,output_address(c[2]),activity_address(context_slot),map_address(initial_snapshot,c[2]),0,0,1,PROCESS_CLEANED);
                end else save_context(PROCESS_CLEANUP_NEXT);
            end
            PROCESS_CLEANED: if(last_error!=0) begin
                    c[28]<=1;owners[c[2]]<=owners[c[2]]|c[3][15:0];if(response_error==0) response_error<=last_error;
                    save_context(PROCESS_CLEANUP_NEXT);
                end else call_store(9,{c[11],c[10]},0,0,0,PROCESS_CLEANUP_RELEASED);
            PROCESS_CLEANUP_RELEASED: if(last_error!=0) begin
                    c[28]<=1;owners[c[2]]<=owners[c[2]]|c[3][15:0];
                    if(response_error==0) response_error<=last_error;save_context(PROCESS_CLEANUP_NEXT);
                end else begin c[10]<=0;c[11]<=0;c[30]<=0;
                call_state(0,activity_address(context_slot),0,0,0,0,0,PROCESS_CLEANUP_RESET);end
            PROCESS_CLEANUP_RESET: save_context(PROCESS_CLEANUP_NEXT);
            PROCESS_CLEANUP_NEXT: begin scan<=scan+1;pc<=PROCESS_CLEANUP;end
            PROCESS_OWNERS: if(scan==32'(LOOPS)) begin
                    scan<=0;if(established) established<=0;else pc<=PROCESS_PLAY;
                end else load_context(scan,PROCESS_OWNER_LOADED);
            PROCESS_OWNER_LOADED: if(c[0]==0 || c[28]!=0 || c[17]!=2 || {c[19],c[18]}>={c[21],c[20]} ||
                    ({c[11],c[10]}!=0)!=established || recovery[c[2]]!=32) pc<=PROCESS_OWNER_NEXT;
                else if((owners[c[2]]&c[3][15:0])!=0) begin
                    c[28]<=1;c[23]<=1;if(response_error==0) response_error<=2;save_context(PROCESS_OWNER_NEXT);
                end else begin owners[c[2]]<=owners[c[2]]|c[3][15:0];pc<=PROCESS_OWNER_NEXT;end
            PROCESS_OWNER_NEXT: begin scan<=scan+1;pc<=PROCESS_OWNERS;end
            PROCESS_PLAY: if(scan==32'(LOOPS)) begin scan<=0;found<=0;pc<=PROCESS_HEAD;end
                else load_context(scan,PROCESS_PLAY_LOADED);
            PROCESS_PLAY_LOADED: if(c[0]==0 || c[28]!=0 || recovery[c[2]]!=32 || c[17]!=2 || {c[19],c[18]}>={c[21],c[20]}) pc<=PROCESS_PLAY_NEXT;
                else if(c[23]!=0 && {c[19],c[18]}!=0) begin if(response_error==0) response_error<=5;pc<=PROCESS_PLAY_NEXT;end
                else if({c[11],c[10]}==0) begin
                    if(c[13]!=2) begin if(response_error==0) response_error<=4;pc<=PROCESS_PLAY_NEXT;end
                    else if({c[19],c[18]}!=0) begin if(response_error==0) response_error<=2;pc<=PROCESS_PLAY_NEXT;end
                    else call_store(12,{c[7],c[6]},0,0,0,PROCESS_PLAY_DESCRIPTION);
                end else pc<=PROCESS_PLAY_NEXT;
            PROCESS_PLAY_DESCRIPTION: if(last_error!=0 || last_status!=2 || last_frame!={c[21],c[20]}) begin
                    if(response_error==0) response_error<=4;pc<=PROCESS_PLAY_NEXT;
                end else begin snapshot_id<=last_initial;
                    read_word(directory_address(last_initial,c[2])+40,PROCESS_PLAY_GENERATION);end
            PROCESS_PLAY_GENERATION: if(data_word!=snapshot_id[63:32]) begin if(response_error==0) response_error<=5;pc<=PROCESS_PLAY_NEXT;end
                else read_word(directory_address(snapshot_id,c[2])+44,PROCESS_PLAY_READY);
            PROCESS_PLAY_READY: if(data_word!=4225) begin if(response_error==0) response_error<=5;pc<=PROCESS_PLAY_NEXT;end
                else call_store(8,{c[7],c[6]},0,0,0,PROCESS_PLAY_PINNED);
            PROCESS_PLAY_PINNED: if(last_error!=0) begin if(response_error==0) response_error<=last_error;pc<=PROCESS_PLAY_NEXT;end
                else begin c[10]<=c[6];c[11]<=c[7];c[12]<=0;c[14]<=0;c[15]<=0;c[23]<=0;
                    copy_words(snapshot_address(snapshot_id),activity_address(context_slot),4225,PROCESS_PLAY_ACTIVITY);end
            PROCESS_PLAY_ACTIVITY: begin packet_destination<=c[2];packet_offset<=0;
                call_state(3,output_address(c[2]),snapshot_address(snapshot_id),map_address(snapshot_id,c[2]),0,0,1,PROCESS_PLAY_RESTORED);end
            PROCESS_PLAY_RESTORED: if(last_error!=0) begin
                    if(response_error==0) response_error<=last_error;failed_destination<=c[2];
                    save_context(PROCESS_PLAY_FAILED);
                end else save_context(PROCESS_PLAY_NEXT);
            PROCESS_PLAY_FAILED: begin failure_return<=PROCESS_PLAY_NEXT;failure_saved_scan<=scan;pc<=FAIL_DESTINATION;end
            PROCESS_PLAY_NEXT: begin scan<=scan+1;pc<=PROCESS_PLAY;end
            PROCESS_HEAD: if(scan==32'(LOOPS)) begin
                    if(found) load_context(best_slot,PROCESS_EMIT);else begin scan<=0;pc<=PROCESS_FINISH_CONTEXT;end
                end else load_context(scan,PROCESS_HEAD_LOADED);
            PROCESS_HEAD_LOADED: if(c[0]==0 || c[28]!=0 || c[23]!=0 || c[30]!=0 || c[17]!=2 ||
                    recovery[c[2]]!=32 || {c[11],c[10]}==0 || {c[19],c[18]}>={c[21],c[20]}) pc<=PROCESS_HEAD_NEXT;
                else call_store(10,{c[11],c[10]},0,{32'b0,c[12]},0,PROCESS_HEAD_RECORD);
            PROCESS_HEAD_RECORD: if(last_error!=0) begin if(response_error==0) response_error<=last_error;pc<=PROCESS_HEAD_NEXT;end
                else if(last_length==0 || last_frame>={c[15],c[14]}+frames) pc<=PROCESS_HEAD_NEXT;
                else if(last_frame<{c[15],c[14]}) begin c[23]<=1;if(response_error==0) response_error<=4;save_context(PROCESS_HEAD_NEXT);end
                else if(last_packet[7:0]!=8'hff && !c[3][{1'b0,last_packet[3:0]}]) begin c[12]<=c[12]+1;save_context(PROCESS_HEAD);end
                else begin
                    x=last_frame-{c[15],c[14]};
                    if(!found || x<best_frame) begin found<=1;best_frame<=x;best_slot<=context_slot;best_packet<=last_packet;best_length<=last_length;end
                    pc<=PROCESS_HEAD_NEXT;
                end
            PROCESS_HEAD_NEXT: begin scan<=scan+1;pc<=PROCESS_HEAD;end
            PROCESS_EMIT: if(best_packet[7:0]==8'hff && c[3][15:0]!=16'hffff) begin
                    if(response_error==0) response_error<=2;failed_destination<=c[2];
                    failure_saved_scan<=0;failure_return<=PROCESS_RESCAN;pc<=FAIL_DESTINATION;
                end else begin packet_destination<=c[2];packet_offset<=best_frame[31:0];
                    call_state(5,output_address(c[2]),0,0,best_packet,best_length,1,PROCESS_EMITTED);end
            PROCESS_EMITTED: if(last_error!=0) begin
                    if(response_error==0) response_error<=last_error;failed_destination<=c[2];
                    failure_saved_scan<=0;failure_return<=PROCESS_RESCAN;pc<=FAIL_DESTINATION;
                end else begin c[12]<=c[12]+1;save_context(PROCESS_RESCAN);end
            PROCESS_RESCAN: begin scan<=0;found<=0;pc<=PROCESS_HEAD;end
            PROCESS_FINISH_CONTEXT: if(scan==32'(LOOPS)) call_store(11,0,0,64'(MAINTENANCE),0,PROCESS_FINISHED);
                else load_context(scan,PROCESS_FINISH_LOADED);
            PROCESS_FINISH_LOADED: begin
                if(c[0]!=0) begin
                    if({c[11],c[10]}!=0 && c[28]==0 && c[23]==0 && c[30]==0) begin x={c[15],c[14]}+frames;c[14]<=x[31:0];c[15]<=x[63:32];end
                    c[16]<=c[17]==2 && {c[19],c[18]}>={c[21],c[20]} ? 0 : c[17];c[22]<=0;
                end
                save_context(PROCESS_FINISH_NEXT);
            end
            PROCESS_FINISH_NEXT: begin scan<=scan+1;pc<=PROCESS_FINISH_CONTEXT;end
            PROCESS_FINISHED: begin
                running<=0;seen_quantum<=1;next_frame<=start_frame+frames;response_frame<=start_frame+frames;
                transfer_events<=64;transfer_cells<=256;transfer_metadata<=64;publication_bindings<=128;pc<=RESPONSE;
            end
            FAIL_DESTINATION: begin
                if(recovery[failed_destination]==32) recovery[failed_destination]<=0;
                scan<=0;pc<=FAIL_CONTEXT;
            end
            FAIL_CONTEXT: if(scan==32'(LOOPS)) begin scan<=failure_saved_scan;pc<=failure_return;end
                else load_context(scan,FAIL_CONTEXT_LOADED);
            FAIL_CONTEXT_LOADED: if(c[0]!=0 && c[2]==failed_destination) begin c[23]<=1;c[30]<=0;save_context(FAIL_CONTEXT_NEXT);end
                else pc<=FAIL_CONTEXT_NEXT;
            FAIL_CONTEXT_NEXT: begin scan<=scan+1;pc<=FAIL_CONTEXT;end
            RECOVERY: if(recovery_destination==32'(DESTINATIONS)) pc<=recovery_return;
                else if(recovery[recovery_destination]==32) recovery_destination<=recovery_destination+1;
                else begin
                    packet_destination<=recovery_destination;packet_offset<=recovery_offset;
                    recovery_packet<={8'b0,recovery[recovery_destination][0] ? 8'd123 : 8'd120,
                        8'hb0 | {4'b0,recovery[recovery_destination][4:1]}};
                    pc<=RECOVERY_SEND;
                end
            RECOVERY_SEND: call_state(5,output_address(recovery_destination),0,0,recovery_packet,3,0,RECOVERY_SENT);
            RECOVERY_SENT: if(last_error!=0) begin if(response_error==0) response_error<=last_error;pc<=recovery_return;end
                else begin scan<=0;pc<=RECOVERY_ACTIVITY;end
            RECOVERY_ACTIVITY: if(scan==32'(LOOPS)) begin recovery[recovery_destination]<=recovery[recovery_destination]+1;pc<=RECOVERY;end
                else load_context(scan,RECOVERY_ACTIVITY_LOADED);
            RECOVERY_ACTIVITY_LOADED: if(c[0]!=0 && c[2]==recovery_destination && c[3][{1'b0,recovery_packet[3:0]}])
                call_state(1,activity_address(context_slot),0,0,recovery_packet,3,0,RECOVERY_ACTIVITY_NEXT);
                else pc<=RECOVERY_ACTIVITY_NEXT;
            RECOVERY_ACTIVITY_NEXT: begin scan<=scan+1;pc<=RECOVERY_ACTIVITY;end
            BIND_UNIQUE: if(scan==32'(LOOPS)) load_context(a0[31:0],BIND_COMMIT);
                else load_context(scan,BIND_UNIQUE_CHECK);
            BIND_UNIQUE_CHECK: if(c[0]!=0 && {c[5],c[4]}==a1) fail(4);else begin scan<=scan+1;pc<=BIND_UNIQUE;end
            BIND_COMMIT: begin c[0]<=1;c[1]<=a2[31:0];c[2]<=a3[31:0];c[3]<=a4[31:0];c[4]<=a1[31:0];c[5]<=a1[63:32];save_context(RESPONSE);end
            RESERVE_HISTORY: if(reservation_scan==32'(INPUTS)) begin reservation_scan<=0;pc<=RESERVE_RECORD;end
                else load_history(reservation_scan,RESERVE_HISTORY_LOADED);
            RESERVE_HISTORY_LOADED: call_store(12,{h[1],h[0]},0,0,0,RESERVE_HISTORY_VIEW);
            RESERVE_HISTORY_VIEW: begin
                value={h[13],h[12]}==0 ? 3 : 32'((MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS)+3;
                if(value>last_blocks) reserved_blocks<=reserved_blocks+value-last_blocks;
                value=32'(INPUT_EVENTS);
                if({h[13],h[12]}!=0 && 32'(MAX_EVENTS/32)+2>h[5]) value=value+32'(MAX_EVENTS/32)+2-h[5];
                reserved_snapshots<=reserved_snapshots+value;reservation_scan<=reservation_scan+1;pc<=RESERVE_HISTORY;
            end
            RESERVE_RECORD: if(reservation_scan==32'(LOOPS)) pc<=reservation_return;
                else load_context(reservation_scan,RESERVE_RECORD_LOADED);
            RESERVE_RECORD_LOADED: if(c[13]==1 && {c[9],c[8]}!=0) call_store(12,{c[9],c[8]},0,0,0,RESERVE_RECORD_VIEW);
                else begin reservation_scan<=reservation_scan+1;pc<=RESERVE_RECORD;end
            RESERVE_RECORD_VIEW: begin
                value=32'((MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS);
                if(value>last_blocks) reserved_blocks<=reserved_blocks+value-last_blocks;
                reservation_scan<=reservation_scan+1;pc<=RESERVE_RECORD;
            end
            IMPORT_RESERVED: if(free_snapshots<=reserved_snapshots || free_views<=2) fail(3);
                else begin scan<=0;index<=0;pc<=IMPORT_VALIDATE;end
            IMPORT_APPEND_CHECK: if(last_error!=0) fail(last_error);else if(last_owner!=0 || last_status!=1) fail(4);
                else begin value=(last_events+1+32'(BLOCK_EVENTS)-1)/32'(BLOCK_EVENTS);
                    import_needed<=value>last_blocks ? value-last_blocks : 0;reserve_capture(IMPORT_APPEND_RESERVED);end
            IMPORT_APPEND_RESERVED: if(free_blocks<reserved_blocks+import_needed) fail(3);else pc<=IMPORT_APPEND;
            PROCESS_RECORD_RESERVED: begin scan<=reservation_saved_scan;load_context(reservation_context,PROCESS_RECORD_RESERVATION);end
            PROCESS_RECORD_RESERVATION: if(free_blocks<reserved_blocks+32'((MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS)) begin
                    c[13]<=3;if(response_error==0) response_error<=3;save_context(PROCESS_RECORD_NEXT);
                end else load_history(c[1],PROCESS_RECORD_SEED);
            QUERY_OUTPUT: if(scan==32'(LOOPS)) begin response_state<=output_address(a1[31:0]);pc<=RESPONSE;end
                else load_context(scan,QUERY_OUTPUT_LOADED);
            QUERY_OUTPUT_LOADED: if(c[0]!=0 && c[2]==a1[31:0] && c[30]!=0) fail(5);
                else begin scan<=scan+1;pc<=QUERY_OUTPUT;end
            CONTROL_FAULT_CONTEXT: if(scan==32'(LOOPS)) pc<=RESPONSE;else load_context(scan,CONTROL_FAULT_LOADED);
            CONTROL_FAULT_LOADED: begin if(c[0]!=0) begin c[23]<=1;c[30]<=0;end save_context(CONTROL_FAULT_NEXT);end
            CONTROL_FAULT_NEXT: begin scan<=scan+1;pc<=CONTROL_FAULT_CONTEXT;end
            CONTROL_MAINTAINED: begin response_count<=store_work;pc<=RESPONSE;end
            ZERO_WRITE: if(copy_index==copy_count) pc<=copy_return;else write_word(copy_target+64'(copy_index)*4,0,ZERO_NEXT);
            ZERO_NEXT: begin copy_index<=copy_index+1;pc<=ZERO_WRITE;end
            DERIVE_ENTRY: begin
                if(snapshot_id[31:0]>=high_snapshot) high_snapshot<=snapshot_id[31:0]+1;
                if(prepare_destination==32'(DESTINATIONS)) begin prepare_is_derive<=0;pc<=derive_return;end
                else read_word(directory_address(derive_source_snapshot,prepare_destination)+40,DERIVE_GENERATION);
            end
            DERIVE_GENERATION: if(data_word!=derive_source_snapshot[63:32]) fail(4);
                else read_word(directory_address(derive_source_snapshot,prepare_destination)+44,DERIVE_READY);
            DERIVE_READY: if(data_word!=4225) fail(4);else begin
                prepare_map<=map_address(snapshot_id,prepare_destination);prepare_entry<=directory_address(snapshot_id,prepare_destination);
                prepare_cursor<=0;cursor<=0;pc<=PREPARE_CREATE;
            end
            DERIVE_MAP_COPY: copy_words(map_address(derive_source_snapshot,prepare_destination),prepare_map,133,DERIVE_COPIED);
            DERIVE_COPIED: begin change_word_index<=0;pc<=DERIVE_CHANGED_WORD;end
            DERIVE_CHANGED_WORD: if(change_word_index==133) write_word(prepare_entry+44,4225,DERIVE_NEXT);
                else read_word(derive_changes+64'(change_word_index)*4,DERIVE_CHANGED_READ);
            DERIVE_CHANGED_READ: begin changed_bits<=data_word;pc<=DERIVE_CHANGED_BITS;end
            DERIVE_CHANGED_BITS: if(changed_bits==0) begin change_word_index<=change_word_index+1;pc<=DERIVE_CHANGED_WORD;end
                else begin
                    value=0;for(integer n=31;n>=0;n=n-1) if(changed_bits[n]) value=32'(n);
                    changed_bits<=changed_bits & ~(32'b1 << value);
                    state_op<=2;state_current<=output_address(prepare_destination);state_target<=snapshot_base;state_map<=prepare_map;
                    state_first<=13'(change_word_index*32+value);state_count<=1;state_track_activity<=0;
                    state_track_changes<=0;state_pairs<=0;service_return<=DERIVE_CHANGED_DONE;pc<=STATE_START;
                end
            DERIVE_CHANGED_DONE: if(last_error!=0) fail(last_error);else pc<=DERIVE_CHANGED_BITS;
            DERIVE_NEXT: begin prepare_destination<=prepare_destination+1;pc<=DERIVE_ENTRY;end
            ALIGN_DURATION: begin
                x=(alignment_remainder<<1)|64'(alignment_value[alignment_bit]);
                if(x>={32'b0,quantum_frames}) x=x-{32'b0,quantum_frames};
                alignment_remainder<=x;
                if(alignment_bit==0) begin if(x!=0) fail(4);else pc<=alignment_return;end
                else alignment_bit<=alignment_bit-1;
            end
            GROUP_DESCRIPTION: if(last_error!=0) fail(last_error);else if(last_status!=2 || last_frame==0) fail(4);
                else if(last_references>32'hffffffff-target_count) fail(6);
                else begin range_start<=0;range_end<=last_frame;snapshot_id<=last_initial;aligned_duration(last_frame,GRAB_VALIDATE);end
            GROUP_MAP_GENERATION: if(data_word!=snapshot_id[63:32]) fail(5);
                else read_word(directory_address(snapshot_id,c[2])+44,GROUP_MAP_READY);
            GROUP_MAP_READY: if(data_word!=4225) fail(5);else begin target_index<=target_index+1;pc<=GRAB_VALIDATE;end
            GROUP_START: begin target_index<=0;response_handle<=range_view;response_frame<=range_end;
                pc<=op==22 ? GRAB_PUBLISH : GRAB_RETAIN;end
            PANIC_POLL: if(recovery_destination==32'(DESTINATIONS)) pc<=RESPONSE;
                else if(recovery[recovery_destination]==32) recovery_destination<=recovery_destination+1;
                else begin
                    packet_destination<=recovery_destination;packet_offset<=recovery_offset;
                    recovery_packet<={8'b0,recovery[recovery_destination][0] ? 8'd123 : 8'd120,
                        8'hb0 | {4'b0,recovery[recovery_destination][4:1]}};pc<=PANIC_EMIT;
                end
            PANIC_EMIT: if(packet_ready) begin
                if(packet_refused) fail(3);
                else begin recovery[recovery_destination]<=recovery[recovery_destination]+1;pc<=PANIC_POLL;end
            end
            default: memory_fault();
        endcase
    end
endmodule
