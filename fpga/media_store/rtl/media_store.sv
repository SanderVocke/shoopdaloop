module media_store #(
    parameter integer REUSE_VIEWS = 0,
    parameter integer VIEWS = 32,
    parameter integer SLOTS = 1024,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer RETIRED_TABLES = 256,
    parameter integer RECLAIM_BLOCKS = 0, parameter integer RECLAIM_CYCLES = 5000,
    parameter integer RECLAIM_GUARD = 1024,
    parameter integer TABLE_PAGES = (SLOTS/BLOCK_CHUNKS+14)/15 + 8*VIEWS + 16,
    parameter integer TABLE_LINES = 8, parameter integer REF_LINES = 4, parameter integer STACK_LINES = 2,
    parameter integer READ_BUFFERS = 4,
    parameter integer WRITE_BUFFERS = 4, parameter integer BURST_MEMORY = 0,
    parameter integer VIEW_BITS = (VIEWS < 2) ? 1 : $clog2(VIEWS),
    parameter integer SLOT_BITS = (SLOTS/BLOCK_CHUNKS < 2) ? 1 : $clog2(SLOTS/BLOCK_CHUNKS),
    parameter integer TABLE_BITS = VIEW_BITS + SLOT_BITS
) (
    input wire clk, input wire reset,
    input wire maintenance_enable, input wire maintenance_tick,
    output wire [31:0] reclaim_pending, output wire [31:0] reclaim_high_water,
    output wire [63:0] reclaim_references, output wire reclaim_active, output wire reclaim_released,
    output wire [31:0] reclaim_block, output wire [63:0] reclaim_blocks,
    output wire [63:0] reclaim_freed, output wire [63:0] reclaim_cycles,
    output wire reclaim_quiescent,
    output wire [31:0] reclaim_pages_used, output wire [31:0] reclaim_pages_available,
    output wire [63:0] reclaim_pages_freed, output reg [63:0] reclaim_capacity_failures,
    input wire [63:0] arena_base,
    input wire hint_valid, output wire hint_ready,
    input wire [63:0] hint_view, input wire [63:0] hint_position, input wire [63:0] hint_length,
    input wire [31:0] hint_count,
    input wire cmd_valid, output wire cmd_ready,
    input wire [3:0] cmd_opcode,
    input wire [63:0] cmd_view, input wire [63:0] cmd_start,
    input wire [63:0] cmd_length, input wire [31:0] cmd_count,
    output wire busy, output reg done, output reg [7:0] error_code,
    output wire memory_fault,
    output reg [31:0] result_view, output reg result_empty,
    input wire write_valid, output wire write_ready, input wire [31:0] write_data,
    output wire read_valid, input wire read_ready, output wire [31:0] read_data,
    output wire memory_valid, input wire memory_ready,
    output wire memory_write, output wire [63:0] memory_address,
    output wire [31:0] memory_write_data,
    output wire [63:0] memory_arena_base,
    output wire [31:0] memory_count,
    output wire memory_write_valid, input wire memory_write_ready,
    input wire memory_read_valid, output wire memory_read_ready,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data,
    output wire memory_meta_valid, input wire memory_meta_ready,
    output wire memory_meta_write, output wire [63:0] memory_meta_address,
    output wire [31:0] memory_meta_write_data,
    output wire [31:0] memory_meta_count,
    output wire memory_meta_write_valid, input wire memory_meta_write_ready,
    input wire memory_meta_read_valid, output wire memory_meta_read_ready,
    input wire memory_meta_response_valid, output wire memory_meta_response_ready,
    input wire memory_meta_response_error, input wire [31:0] memory_meta_read_data,
    input wire cfg_valid, output wire cfg_ready, input wire cfg_write,
    input wire [2:0] cfg_space, input wire [31:0] cfg_address,
    input wire [159:0] cfg_write_data,
    output wire cfg_response_valid, output reg cfg_response_error,
    output reg [159:0] cfg_read_data
);
    localparam [3:0] CREATE = 0, SLICE = 1, CLEAR = 2, REMOVE = 3,
        RECLAIM = 4, APPEND = 5, READ = 6, FENCE = 7;
    localparam [7:0] OK = 0, INVALID_VIEW = 1, INVALID_STATE = 2,
        MEMORY_FULL = 3, INVALID_SHAPE = 4, MEMORY_FAULT = 5;
    localparam integer SHIFT = $clog2(CHUNK_FRAMES);
    localparam integer SPAN_BITS = $clog2(CHUNK_FRAMES+1);
    localparam integer OFFSET_BITS = SHIFT < 1 ? 1 : SHIFT;
    localparam integer BLOCK_SHIFT = $clog2(BLOCK_CHUNKS);
    localparam integer UNIT_SHIFT = SHIFT+BLOCK_SHIFT;
    localparam integer BLOCKS = SLOTS/BLOCK_CHUNKS;
    localparam [63:0] BLOCK_FRAMES = 64'(CHUNK_FRAMES)*64'(BLOCK_CHUNKS);
    localparam [63:0] ARENA_BYTES = (((64'(SLOTS)*64'(CHUNK_FRAMES)+15)/16)*16 + 2*((64'(BLOCKS)+15)/16)*16 + 64'(TABLE_PAGES)*16+64'(RETIRED_TABLES)*16)*4;
    typedef enum logic [5:0] {
        IDLE, ADMIT, DESCRIPTOR_READ, DESCRIPTOR_RESULT, CHECK, FREE_VIEW_READ, FREE_VIEW_RESULT,
        APPEND_END, APPEND_CHUNKS, APPEND_REQUIRED, APPEND_GROWTH, APPEND_EXTENT, APPEND_VALIDATE, APPEND_CHECK,
        SLICE_START, SLICE_OFFSET, SLICE_END, SLICE_CHUNKS, SLICE_CHECK, SLICE_CAPACITY, RECLAIM_END, RECLAIM_FRAMES, RECLAIM_CHECK,
        TABLE_READ, TABLE_RESULT, REF_READ, REF_RESULT, UPDATE_REFERENCE,
        BEGIN_CREATE, PUBLISH, DETACH_ROOT, DRAIN_WAIT, ALLOCATE, FREE_RESULT, ALLOCATE_WRITE,
        ZERO_REQUEST, ZERO_DATA, ZERO_WAIT, SAMPLE_TABLE_READ,
        SAMPLE_TABLE_RESULT, SAMPLE_REQUEST, SAMPLE_DATA, SAMPLE_WAIT,
        RETIRE_REQUEST, RETIRE_WAIT, FENCE_REQUEST, FENCE_WAIT, FENCE_METADATA,
        FINISH
    } phase_t;
    (* fsm_encoding = "one_hot" *) phase_t phase;
    reg descriptor_read, descriptor_write, table_read, table_write, ref_read, ref_write, free_read, free_write;
    reg [VIEW_BITS-1:0] descriptor_read_address, descriptor_write_address;
    reg [TABLE_BITS-1:0] table_read_address, table_write_address;
    reg [SLOT_BITS-1:0] ref_read_address, ref_write_address, free_read_address, free_write_address;
    reg [159:0] descriptor_write_data;
    reg [31:0] table_write_data, ref_write_data, free_write_data;
    wire [159:0] descriptor_data;
    reg [159:0] foreground_descriptor;
    reg [31:0] table_data, reference_data, free_data;
    reg [31:0] allocation_view;
    reg [31:0] view_count, slot_count, free_count, slot_limit;
    reg [VIEWS-1:0] valid_views;
    wire [31:0] views_used = REUSE_VIEWS != 0 ? 32'($countones(valid_views)) : view_count;
    reg cfg_pending, cfg_result_valid, faulted, cfg_was_write;
    reg cfg_request_valid, cfg_request_write, cfg_request_in_range, cfg_request_value_valid;
    reg [2:0] cfg_request_space;
    reg [31:0] cfg_request_address;
    reg [159:0] cfg_request_data;
    reg [2:0] cfg_response_space;
    reg [159:0] cfg_globals_data;
    reg [3:0] opcode;
    reg admission_base_valid, admission_start_valid;
    reg admission_view_greater, admission_view_at_end, admission_view_outside, admission_view_full;
    reg [31:0] view_index, count, index, first_chunk, remaining, append_index;
    reg [31:0] chunk, sample_block, sample_index;
    reg [31:0] sample_last;
    reg samples_finished;
    reg [SPAN_BITS-1:0] zero_index;
    reg [SPAN_BITS-1:0] span_count, span_index;
    reg [OFFSET_BITS-1:0] sample_offset;
    reg [63:0] base, start, length, cursor;
    reg [159:0] descriptor;
    wire [63:0] frames = descriptor[127:64];
    wire [31:0] offset = descriptor[63:32], chunks = descriptor[159:128];
    wire unused_descriptor = &{1'b0, descriptor[31:2], descriptor[0]};
    reg new_valid, new_range;
    reg [31:0] new_offset, new_chunks;
    reg [63:0] new_frames;
    reg fresh, arena_bound;
    reg [63:0] absolute_start, slice_available, slice_end;
    reg slice_out_of_bounds, slice_length_valid, slice_capacity_valid;
    wire [63:0] slice_chunk_result = (slice_end+BLOCK_FRAMES-1) >> UNIT_SHIFT;
    reg [63:0] append_end, append_chunks, required_chunks, growth, append_extent;
    reg append_capacity_ok;
    wire [63:0] absolute_sample = {32'b0, offset} + cursor;
    reg [31:0] range_offset;
    reg [31:0] range_chunks;
    reg [63:0] retained_end;
    reg [63:0] retained_frames;
    reg [63:0] length_last, span_until_wrap;
    reg [31:0] span_contiguous;
    wire cfg_accept = cfg_valid && cfg_ready;
    wire cfg_in_range = (cfg_space == 0) ? cfg_address < 4
        : (cfg_space == 1) ? cfg_address < 32'(VIEWS)
        : (cfg_space == 2) ? ((cfg_address >> SLOT_BITS) < 32'(VIEWS) && (cfg_address & ((32'd1 << SLOT_BITS)-1)) < 32'(BLOCKS))
        : (cfg_space == 5) ? cfg_address == 0 && cfg_write
        : (cfg_space == 6) ? (cfg_address < 15 || cfg_address == 16 || cfg_address == 17) && !cfg_write
        : (cfg_space == 7) ? cfg_address < 32'(RETIRED_TABLES)*16 && !cfg_write
        : ((cfg_space == 3) || (cfg_space == 4)) && (cfg_address < 32'(BLOCKS));
    wire cfg_value_valid = !cfg_write || (cfg_space == 1)
        || ((cfg_write_data[159:32] == 0)
            && ((cfg_space != 0) || (cfg_write_data[31:0] <= ((cfg_address == 0) ? 32'(VIEWS) : 32'(BLOCKS)))));
    wire [31:0] control_ordinal = (opcode == SLICE) ? first_chunk + index : index;
    wire [TABLE_BITS-1:0] control_address = TABLE_BITS'((view_index << SLOT_BITS) | control_ordinal);
    wire [TABLE_BITS-1:0] sample_address = TABLE_BITS'((view_index << SLOT_BITS) | 32'(absolute_sample >> UNIT_SHIFT));
    wire [31:0] samples_left = count-sample_index;
    wire [31:0] chunk_left = 32'(CHUNK_FRAMES) - (absolute_sample[31:0] & 32'(CHUNK_FRAMES-1));
    wire [31:0] contiguous = samples_left < chunk_left ? samples_left : chunk_left;
    wire [SPAN_BITS-1:0] next_span = opcode == READ && span_until_wrap < {32'b0,span_contiguous}
        ? SPAN_BITS'(span_until_wrap) : SPAN_BITS'(span_contiguous);
    wire cm_cmd_ready, cm_write_ready, cm_read_valid, cm_read_last, cm_response_valid, cm_prefetch_ready;
    wire [31:0] cm_read_data, cm_response_id;
    wire [7:0] cm_response_error;
    wire unused_cm_status, cm_busy;
    wire resolver_ready, resolver_prefetch_valid;
    wire [31:0] resolver_prefetch_chunk, resolver_descriptor_address, resolver_table_address;
    wire unused_resolver_addresses = &{1'b0,resolver_descriptor_address,resolver_table_address};
    wire resolver_descriptor_valid, resolver_table_valid;
    wire descriptor_grant = resolver_descriptor_valid && !descriptor_read && !reset;
    wire table_grant;
    wire [31:0] resolver_table_data;
    wire [VIEW_BITS-1:0] descriptor_ram_address = descriptor_read ? descriptor_read_address
        : resolver_descriptor_address[VIEW_BITS-1:0];
    reg descriptor_response;
    reg table_response;
    wire hint_base_valid = arena_base[1:0] == 0 && arena_base <= 64'hffffffffffffffff-ARENA_BYTES
        && (!arena_bound || arena_base == base);
    wire hint_admission = !faulted && !reset && (arena_bound || (phase == IDLE && !cmd_valid));
    wire prefetch_admission = arena_bound || !cmd_valid;
    wire cm_data_read = phase == SAMPLE_DATA && opcode == READ;
    wire cm_data_write = phase == SAMPLE_DATA && opcode == APPEND;
    reg sample_stream_enabled, zero_stream_enabled;
    always @(posedge clk) begin
        if (reset) begin sample_stream_enabled <= 0; zero_stream_enabled <= 0; end
        else begin
            sample_stream_enabled <= phase == SAMPLE_DATA;
            zero_stream_enabled <= phase == ZERO_DATA;
        end
    end
    wire cm_response_ready = phase == ZERO_WAIT || phase == SAMPLE_WAIT || phase == RETIRE_WAIT || phase == FENCE_WAIT;
    wire cm_command = phase == ZERO_REQUEST || (phase == SAMPLE_REQUEST && sample_block < slot_count)
        || phase == RETIRE_REQUEST || phase == FENCE_REQUEST;
    wire [2:0] cm_opcode = phase == FENCE_REQUEST ? 3'd3 : phase == RETIRE_REQUEST ? 3'd4
        : phase == ZERO_REQUEST || opcode == APPEND ? 3'd1 : 3'd0;
    reg cm_pending;
    reg [2:0] cm_pending_opcode;
    reg [31:0] cm_pending_chunk, cm_pending_offset, cm_pending_count;
    wire cm_endpoint_ready;
    assign cm_cmd_ready = !cm_pending && !reset;
    always @(posedge clk) begin
        if (reset) begin
            cm_pending <= 0; cm_pending_opcode <= 0;
            cm_pending_chunk <= 0; cm_pending_offset <= 0; cm_pending_count <= 0;
        end else begin
            if (cm_command && cm_cmd_ready && advance) begin
                cm_pending <= 1; cm_pending_opcode <= cm_opcode;
                cm_pending_chunk <= phase == RETIRE_REQUEST || phase == RETIRE_WAIT ? (chunk << BLOCK_SHIFT) : chunk;
                cm_pending_offset <= phase == ZERO_REQUEST ? 32'b0 : 32'(sample_offset);
                cm_pending_count <= phase == ZERO_REQUEST ? 32'(CHUNK_FRAMES) : 32'(span_count);
            end
            if (cm_pending && cm_endpoint_ready) cm_pending <= 0;
        end
    end
    assign unused_cm_status = &{1'b0,cm_read_last,cm_response_id,cm_busy,cm_arena_base};
    wire cm_fault, metadata_fault;
    wire [63:0] cm_arena_base;
    assign memory_fault = cm_fault || metadata_fault || gc_fault;
    assign memory_arena_base = arena_bound ? base : arena_base;

    function automatic [159:0] pack_view(input valid, input range_view,
        input [31:0] first_offset, input [63:0] frame_count, input [31:0] chunk_count);
        pack_view = {chunk_count, frame_count, first_offset, 30'b0, range_view, valid};
    endfunction

    initial begin
        if ((VIEWS < 1) || (VIEWS > 65536) || (SLOTS < 1)
            || (TABLE_BITS > 32) || (CHUNK_FRAMES < 1) || (CHUNK_FRAMES > 65536)
            || ((CHUNK_FRAMES & (CHUNK_FRAMES-1)) != 0)
            || BLOCK_CHUNKS < 1 || BLOCK_CHUNKS > 65536 || (BLOCK_CHUNKS & (BLOCK_CHUNKS-1)) != 0
            || SLOTS % BLOCK_CHUNKS != 0
            || RETIRED_TABLES < 1 || RECLAIM_BLOCKS < 0 || RECLAIM_GUARD < 0 || RECLAIM_CYCLES <= RECLAIM_GUARD
           )
            $fatal(1, "Invalid media store capacities");
    end
    assign busy = phase != IDLE;
    assign cmd_ready = !busy && !gc_lock_held && !cfg_request_valid && !cfg_pending && !cfg_result_valid && !cfg_valid && !faulted && !reset && advance;
    assign hint_ready = resolver_ready && hint_admission;
    assign cfg_ready = !busy && !gc_lock_held && !cfg_request_valid && !cfg_pending && !cfg_result_valid && !reset && advance;
    assign cfg_response_valid = cfg_result_valid && !reset;
    wire [159:0] cfg_result = (cfg_response_error || cfg_was_write) ? 160'b0
        : (cfg_response_space == 0 || cfg_response_space == 6) ? cfg_globals_data
        : (cfg_response_space == 1) ? foreground_descriptor
        : (cfg_response_space == 2 || cfg_response_space == 7) ? {128'b0, table_data}
        : (cfg_response_space == 3) ? {128'b0, reference_data} : {128'b0, free_data};
    assign write_ready = cm_data_write && cm_write_ready && !reset && sample_stream_enabled;
    assign read_valid = cm_data_read && cm_read_valid && !reset && sample_stream_enabled;
    assign read_data = cm_read_data;

    chunk_memory #(.RETIRE_CHUNKS(BLOCK_CHUNKS),.SLOTS(SLOTS), .CHUNK_FRAMES(CHUNK_FRAMES), .READ_BUFFERS(READ_BUFFERS), .WRITE_BUFFERS(WRITE_BUFFERS), .BURST_MEMORY(BURST_MEMORY)) sample_memory (
        .clk(clk), .reset(reset), .arena_base(base),
        .retire_valid(gc_retire_valid),.retire_ready(gc_retire_ready),.retire_chunk(reclaim_block << BLOCK_SHIFT),
        .retire_done(gc_retire_done),.retire_error(gc_retire_error),.retire_busy(gc_retire_busy),
        .cmd_valid(cm_pending && !reset), .cmd_ready(cm_endpoint_ready), .cmd_opcode(cm_pending_opcode), .cmd_id(32'b0),
        .cmd_chunk(cm_pending_chunk), .cmd_offset(cm_pending_offset), .cmd_count(cm_pending_count),
        .write_valid(((phase == ZERO_DATA && zero_stream_enabled) || (cm_data_write && write_valid && sample_stream_enabled)) && !reset),
        .write_ready(cm_write_ready), .write_data(phase == ZERO_DATA ? 32'b0 : write_data),
        .read_valid(cm_read_valid), .read_ready(cm_data_read && read_ready && sample_stream_enabled), .read_data(cm_read_data), .read_last(cm_read_last),
        .response_valid(cm_response_valid), .response_ready(cm_response_ready && advance), .response_id(cm_response_id), .response_error(cm_response_error),
        .prefetch_valid(resolver_prefetch_valid && prefetch_admission), .prefetch_ready(cm_prefetch_ready), .prefetch_chunk(resolver_prefetch_chunk),
        .busy(cm_busy), .fault(cm_fault),
        .memory_valid(memory_valid), .memory_ready(memory_ready), .memory_write(memory_write),
        .memory_address(memory_address), .memory_write_data(memory_write_data), .memory_arena_base(cm_arena_base),
        .memory_count(memory_count), .memory_write_valid(memory_write_valid),
        .memory_write_ready(memory_write_ready), .memory_read_valid(memory_read_valid),
        .memory_read_ready(memory_read_ready),
        .memory_response_valid(memory_response_valid), .memory_response_ready(memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data)
    );

    reg resolver_rebind;
    always @(posedge clk) begin
        if (reset) resolver_rebind <= 0;
        else resolver_rebind <= !arena_bound && cmd_valid && cmd_ready;
    end
    media_prefetch_resolver #(.SLOTS(BLOCKS), .CHUNK_FRAMES(CHUNK_FRAMES),.BLOCK_CHUNKS(BLOCK_CHUNKS)) resolver (
        .clk(clk), .reset(reset || faulted || memory_fault || cancel_lookup || resolver_rebind),
        .hint_valid(hint_valid && hint_base_valid && hint_admission), .hint_ready(resolver_ready),
        .hint_view(hint_view), .hint_position(hint_position), .hint_length(hint_length), .hint_count(hint_count),
        .view_count(view_count), .slot_count(slot_count),
        .descriptor_valid(resolver_descriptor_valid), .descriptor_ready(descriptor_grant),
        .descriptor_address(resolver_descriptor_address), .descriptor_response(descriptor_response), .descriptor_data(descriptor_data),
        .table_valid(resolver_table_valid), .table_ready(table_grant), .table_address(resolver_table_address),
        .table_response(table_response), .table_data(resolver_table_data),
        .prefetch_valid(resolver_prefetch_valid), .prefetch_ready(cm_prefetch_ready && prefetch_admission), .prefetch_chunk(resolver_prefetch_chunk)
    );
    always @(posedge clk) begin
        if (reset) begin descriptor_response <= 0; end
        else begin descriptor_response <= descriptor_grant; end
    end

    media_ram #(.WIDTH(160), .DEPTH(VIEWS), .ADDR_BITS(VIEW_BITS)) descriptors (
        .clk(clk), .read_enable((descriptor_read || descriptor_grant) && !reset),
        .read_address(descriptor_ram_address), .read_data(descriptor_data),
        .write_enable(descriptor_write && advance && !reset), .write_address(descriptor_write_address), .write_data(descriptor_write_data)
    );
    always @(posedge clk) begin
        if (reset) foreground_descriptor <= 0;
        else if (descriptor_read && advance) foreground_descriptor <= descriptor_data;
    end
    reg [3:0] metadata_done;
    reg metadata_pending, metadata_prefetch, metadata_writing, metadata_gc;
    reg [31:0] hint_epoch, metadata_epoch;
    wire cancel_lookup = phase == CHECK && (opcode == CLEAR || opcode == REMOVE);
    reg [1:0] metadata_slot;
    reg [7:0] metadata_error;
    reg [31:0] hint_table_data;
    wire [31:0] metadata_result;
    wire [7:0] metadata_result_error;
    wire metadata_ready, metadata_valid;
    wire [31:0] retired_count, retired_high_water, retired_head, retired_tail;
    wire [63:0] retired_references;
    assign reclaim_pending = retired_count;
    assign reclaim_high_water = retired_high_water;
    assign reclaim_references = retired_references;
    wire gc_fault, unused_gc_lock_request, gc_lock_grant, gc_lock_held, gc_freed;
    wire gc_request_valid, gc_request_ready;
    wire [3:0] gc_request_opcode;
    wire [31:0] gc_request_index, gc_request_data;
    wire gc_retire_valid, gc_retire_ready, gc_retire_done, gc_retire_busy;
    wire [7:0] gc_retire_error;
    wire manual_grant = cmd_valid && cmd_ready && hint_base_valid && cmd_start[63:32] == 0
        && cmd_opcode == 9;
    reg gc_grant_pending, gc_grant_manual;
    reg [31:0] gc_grant_input_cycles, gc_grant_input_blocks;
    wire [31:0] gc_grant_cycles = gc_grant_manual && gc_grant_input_cycles != 0
        ? gc_grant_input_cycles : 32'(RECLAIM_CYCLES);
    wire [31:0] gc_grant_blocks = gc_grant_manual && gc_grant_input_blocks != 0
        ? gc_grant_input_blocks : 32'(RECLAIM_BLOCKS);
    always @(posedge clk) begin
        if (reset) begin
            gc_grant_pending <= 0; gc_grant_manual <= 0;
            gc_grant_input_cycles <= 0; gc_grant_input_blocks <= 0;
        end
        else begin
            gc_grant_pending <= maintenance_tick || manual_grant;
            gc_grant_manual <= manual_grant;
            gc_grant_input_cycles <= cmd_start[31:0]; gc_grant_input_blocks <= cmd_count;
        end
    end
    assign gc_lock_grant = (phase == IDLE || phase == DRAIN_WAIT) && !cmd_valid && !cfg_valid
        && !cfg_request_valid && !cfg_pending && !cfg_result_valid && !metadata_pending && !metadata_request_valid;
    assign reclaim_quiescent = !gc_lock_held && !metadata_pending && !metadata_request_valid && !gc_retire_busy;
    media_reclaimer #(.GUARD_CYCLES(RECLAIM_GUARD)) reclaimer (
        .clk(clk),.reset(reset),.enable((maintenance_enable || phase == DRAIN_WAIT) && !faulted),
        .drain(phase == DRAIN_WAIT),.grant(gc_grant_pending),
        .grant_cycles(gc_grant_cycles),.grant_blocks(gc_grant_blocks),
        .pending_tables(retired_count),.free_count(free_count),.allocated_count(slot_count),
        .lock_request(unused_gc_lock_request),.lock_grant(gc_lock_grant),.lock_held(gc_lock_held),
        .request_valid(gc_request_valid),.request_ready(gc_request_ready),.request_opcode(gc_request_opcode),
        .request_index(gc_request_index),.request_data(gc_request_data),
        .response_valid(metadata_valid && metadata_pending && metadata_gc),.response_data(metadata_result),.response_error(metadata_result_error),
        .retire_valid(gc_retire_valid),.retire_ready(gc_retire_ready),.active_block(reclaim_block),
        .retire_done(gc_retire_done),.retire_error(gc_retire_error),
        .freed(gc_freed),.released(reclaim_released),.active(reclaim_active),.fault(gc_fault),
        .released_blocks(reclaim_blocks),.freed_blocks(reclaim_freed),.maintenance_cycles(reclaim_cycles)
    );
    wire metadata_request_ready;
    reg metadata_request_valid;
    reg [3:0] metadata_request_opcode;
    reg [31:0] metadata_request_view, metadata_request_index, metadata_request_data;
    reg [3:0] metadata_opcode;
    reg [31:0] metadata_view, metadata_index, metadata_data;
    reg [1:0] metadata_selected;
    wire checking = (phase == APPEND_CHECK && count != 0 && !descriptor[1] && append_capacity_ok)
        || phase == SLICE_CAPACITY;
    wire trimming = phase == PUBLISH && opcode == RECLAIM;
    wire detaching = phase == DETACH_ROOT;
    wire draining = phase == FENCE_METADATA;
    wire debug_drain = phase == IDLE && cfg_request_valid && cfg_request_space == 5 && cfg_request_in_range && cfg_request_value_valid;
    wire [3:0] metadata_required = {checking || trimming || detaching || draining || debug_drain,
        free_read || free_write, ref_read || ref_write, table_read || table_write};
    reg [3:0] prepared_metadata;
    reg phase_epoch, prepared_epoch;
    reg prepared_config, prepared_valid;
    wire controls_ready = prepared_valid && prepared_epoch == phase_epoch && prepared_config == cfg_request_valid;
    wire advance = controls_ready && (prepared_metadata & ~metadata_done) == 0;
    wire foreground_metadata = controls_ready && (prepared_metadata & ~metadata_done) != 0;
    always @(posedge clk) begin
        if (reset) begin
            prepared_metadata <= 0; prepared_epoch <= 0;
            prepared_config <= 0; prepared_valid <= 0;
        end else begin
            prepared_metadata <= metadata_required; prepared_epoch <= phase_epoch;
            prepared_config <= cfg_request_valid; prepared_valid <= 1;
        end
    end
    wire gc_eligible = gc_request_valid && (phase == IDLE || phase == DRAIN_WAIT)
        && (gc_lock_held || (!cfg_valid && !cfg_request_valid && !cfg_pending && !cfg_result_valid && !cmd_valid))
        && !faulted;
    wire issue_metadata = !metadata_pending && !reset && (foreground_metadata || gc_eligible ||
        (resolver_table_valid && !cfg_valid && !faulted && !gc_lock_held));
    wire is_gc = !foreground_metadata && gc_eligible;
    wire is_prefetch = !foreground_metadata && !is_gc;
    assign gc_request_ready = issue_metadata && is_gc && metadata_ready;
    assign metadata_ready = !metadata_request_valid || metadata_request_ready;
    assign table_grant = issue_metadata && is_prefetch && metadata_ready;
    assign resolver_table_data = hint_table_data;
    always @* begin
        metadata_opcode = 0; metadata_view = resolver_table_address >> SLOT_BITS;
        metadata_index = resolver_table_address & ((32'd1 << SLOT_BITS)-1); metadata_data = 0; metadata_selected = 0;
        if (foreground_metadata) begin
            if (prepared_metadata[0] && !metadata_done[0]) begin
                metadata_selected = 0; metadata_opcode = {3'b0,table_write};
                metadata_view = 32'(table_write ? table_write_address : table_read_address) >> SLOT_BITS;
                metadata_index = 32'(table_write ? table_write_address : table_read_address) & ((32'd1 << SLOT_BITS)-1);
                metadata_data = table_write_data;
                if (cfg_request_valid && cfg_request_space == 7) begin
                    metadata_opcode = 13; metadata_index = cfg_request_address;
                end
            end else if (prepared_metadata[1] && !metadata_done[1]) begin
                metadata_selected = 1; metadata_opcode = ref_write ? 4'd3 : 4'd2;
                metadata_index = 32'(ref_write ? ref_write_address : ref_read_address); metadata_data = ref_write_data;
            end else if (prepared_metadata[2] && !metadata_done[2]) begin
                metadata_selected = 2; metadata_opcode = free_write ? 4'd5 : 4'd4;
                metadata_index = 32'(free_write ? free_write_address : free_read_address); metadata_data = free_write_data;
            end else begin
                metadata_selected = 3;
                metadata_opcode = checking ? 4'd7 : trimming ? 4'd6 : detaching ? 4'd10 : debug_drain ? 4'd9 : 4'd8;
                metadata_view = opcode == SLICE ? allocation_view : view_index;
                metadata_index = checking ? (opcode == SLICE ? range_chunks[31:0] : append_chunks[31:0]) : new_chunks;
            end
        end else if (is_gc) begin
            metadata_opcode = gc_request_opcode; metadata_view = 0;
            metadata_index = gc_request_index; metadata_data = gc_request_data;
        end
    end
    media_metadata #(.VIEWS(VIEWS),.SLOTS(BLOCKS),.CHUNK_FRAMES(CHUNK_FRAMES*BLOCK_CHUNKS),.TABLE_PAGES(TABLE_PAGES),.RETIRED_TABLES(RETIRED_TABLES),
        .TABLE_LINES(TABLE_LINES),.REF_LINES(REF_LINES),.STACK_LINES(STACK_LINES),.BURST_MEMORY(BURST_MEMORY)) metadata (
        .clk(clk),.reset(reset),.arena_base(memory_arena_base),
        .request_valid(metadata_request_valid),.request_ready(metadata_request_ready),.request_opcode(metadata_request_opcode),
        .request_view(metadata_request_view),.request_index(metadata_request_index),.request_data(metadata_request_data),
        .response_valid(metadata_valid),.response_ready(metadata_pending),
        .response_data(metadata_result),.response_error(metadata_result_error),.fault(metadata_fault),
        .pages_used(reclaim_pages_used),.pages_available(reclaim_pages_available),.pages_freed(reclaim_pages_freed),.retired_head(retired_head),.retired_tail(retired_tail),.retired_count(retired_count),.retired_high_water(retired_high_water),.retired_references(retired_references),
        .memory_valid(memory_meta_valid),.memory_ready(memory_meta_ready),.memory_write(memory_meta_write),
        .memory_address(memory_meta_address),.memory_write_data(memory_meta_write_data),.memory_count(memory_meta_count),
        .memory_write_valid(memory_meta_write_valid),.memory_write_ready(memory_meta_write_ready),
        .memory_read_valid(memory_meta_read_valid),.memory_read_ready(memory_meta_read_ready),
        .memory_response_valid(memory_meta_response_valid),.memory_response_ready(memory_meta_response_ready),
        .memory_response_error(memory_meta_response_error),.memory_read_data(memory_meta_read_data)
    );
    always @(posedge clk) begin
        if (reset) begin
            metadata_request_valid <= 0; metadata_request_opcode <= 0;
            metadata_request_view <= 0; metadata_request_index <= 0; metadata_request_data <= 0;
        end else if (metadata_ready) begin
            metadata_request_valid <= issue_metadata;
            if (issue_metadata) begin
                metadata_request_opcode <= metadata_opcode;
                metadata_request_view <= metadata_view; metadata_request_index <= metadata_index;
                metadata_request_data <= metadata_data;
            end
        end
    end
    always @(posedge clk) begin
        if (reset) begin
            metadata_done <= 0; metadata_pending <= 0; metadata_prefetch <= 0; metadata_writing <= 0; metadata_slot <= 0;
            metadata_error <= 0; table_data <= 0; reference_data <= 0; free_data <= 0;
            hint_table_data <= 0; table_response <= 0; metadata_gc <= 0; hint_epoch <= 0; metadata_epoch <= 0;
        end else begin
            table_response <= 0;
            if (cancel_lookup) hint_epoch <= hint_epoch+1;
            if (advance) begin metadata_done <= 0; metadata_error <= 0; end
            if (issue_metadata && metadata_ready) begin
                metadata_pending <= 1; metadata_prefetch <= is_prefetch; metadata_gc <= is_gc; metadata_epoch <= hint_epoch; metadata_slot <= metadata_selected; metadata_writing <= metadata_opcode[0] && metadata_opcode != 13;
            end
            if (metadata_valid && metadata_pending) begin
                metadata_pending <= 0;
                if (metadata_prefetch) begin
                    hint_table_data <= metadata_result_error == 0 ? metadata_result : 32'hffffffff;
                    table_response <= metadata_epoch == hint_epoch && !cancel_lookup;
                end else if (!metadata_gc) begin
                    metadata_done[metadata_slot] <= 1; metadata_error <= metadata_result_error;
                    if (!metadata_writing) case (metadata_slot)
                        0: table_data <= metadata_result;
                        1: reference_data <= metadata_result;
                        2: free_data <= metadata_result;
                        default: begin end
                    endcase
                end
            end
        end
    end

    always @* begin
        descriptor_read = 0; descriptor_write = 0; table_read = 0; table_write = 0;
        ref_read = 0; ref_write = 0; free_read = 0; free_write = 0;
        descriptor_read_address = VIEW_BITS'(view_index);
        descriptor_write_address = VIEW_BITS'(((opcode == CREATE) || (opcode == SLICE)) ? allocation_view : ((phase == BEGIN_CREATE) ? view_count : view_index));
        table_read_address = control_address;
        table_write_address = TABLE_BITS'((view_index << SLOT_BITS) | append_index);
        ref_read_address = SLOT_BITS'(chunk); ref_write_address = SLOT_BITS'(chunk);
        free_read_address = SLOT_BITS'(free_count-1); free_write_address = SLOT_BITS'(free_count);
        descriptor_write_data = ((opcode == CREATE) || (opcode == SLICE) || new_valid)
            ? pack_view(new_valid, new_range, new_offset, new_frames, new_chunks) : 160'b0;
        table_write_data = chunk; ref_write_data = 1; free_write_data = chunk;
        case (phase)
            DESCRIPTOR_READ: descriptor_read = 1;
            FREE_VIEW_READ: begin descriptor_read = allocation_view < view_count; descriptor_read_address = VIEW_BITS'(allocation_view); end
            TABLE_READ: table_read = 1;
            REF_READ: ref_read = chunk < slot_count;
            UPDATE_REFERENCE: begin
                ref_write = 1;
                if (opcode == SLICE) begin
                    ref_write_data = reference_data + 1;
                    table_write = 1;
                    table_write_address = TABLE_BITS'((allocation_view << SLOT_BITS) | index);
                end else begin
                    ref_write_data = reference_data - 1;
                    free_write = reference_data == 1;
                end
            end
            BEGIN_CREATE: descriptor_write = 1;
            PUBLISH: descriptor_write = 1;
            ALLOCATE: free_read = (remaining != 0) && (free_count != 0);
            ALLOCATE_WRITE: begin table_write = 1; ref_write = 1; end
            SAMPLE_TABLE_READ: begin table_read = 1; table_read_address = sample_address; end
            default: begin end
        endcase
        if (cfg_request_valid && cfg_request_in_range && cfg_request_value_valid) begin
            case (cfg_request_space)
                1: begin
                    descriptor_read = !cfg_request_write; descriptor_write = cfg_request_write;
                    descriptor_read_address = VIEW_BITS'(cfg_request_address); descriptor_write_address = VIEW_BITS'(cfg_request_address);
                    descriptor_write_data = cfg_request_data;
                end
                2: begin
                    table_read = !cfg_request_write; table_write = cfg_request_write;
                    table_read_address = TABLE_BITS'(cfg_request_address); table_write_address = TABLE_BITS'(cfg_request_address);
                    table_write_data = cfg_request_data[31:0];
                end
                3: begin
                    ref_read = !cfg_request_write; ref_write = cfg_request_write;
                    ref_read_address = SLOT_BITS'(cfg_request_address); ref_write_address = SLOT_BITS'(cfg_request_address);
                    ref_write_data = cfg_request_data[31:0];
                end
                7: table_read = !cfg_request_write;
                4: begin
                    free_read = !cfg_request_write; free_write = cfg_request_write;
                    free_read_address = SLOT_BITS'(cfg_request_address); free_write_address = SLOT_BITS'(cfg_request_address);
                    free_write_data = cfg_request_data[31:0];
                end
                default: begin end
            endcase
        end
    end

    task automatic enter_phase(input phase_t next);
        begin phase <= next; phase_epoch <= ~phase_epoch; end
    endtask

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; phase_epoch <= 0;
            done <= 0; reclaim_capacity_failures <= 0;
            error_code <= OK;
            result_view <= 0;
            result_empty <= 0;
            view_count <= 0; allocation_view <= 0; valid_views <= 0;
            slot_count <= 0;
            free_count <= 0;
            slot_limit <= 32'(BLOCKS);
            cfg_pending <= 0;
            cfg_result_valid <= 0; cfg_read_data <= 0;
            cfg_request_valid <= 0; cfg_request_write <= 0;
            cfg_request_in_range <= 0; cfg_request_value_valid <= 0;
            cfg_request_space <= 0; cfg_request_address <= 0; cfg_request_data <= 0;
            cfg_response_error <= 0;
            cfg_globals_data <= 0;
            cfg_was_write <= 0;
            cfg_response_space <= 0;
            faulted <= 0;
            opcode <= CREATE;
            admission_base_valid <= 0; admission_start_valid <= 0;
            admission_view_greater <= 0; admission_view_at_end <= 0;
            admission_view_outside <= 0; admission_view_full <= 0;
            view_index <= 0;
            count <= 0;
            index <= 0;
            first_chunk <= 0;
            remaining <= 0;
            append_index <= 0;
            chunk <= 0; sample_block <= 0;
            sample_index <= 0;
            sample_last <= 0; samples_finished <= 0;
            zero_index <= 0;
            span_count <= 0; span_index <= 0;
            sample_offset <= 0;
            base <= 0;
            start <= 0;
            length <= 0;
            cursor <= 0;
            descriptor <= 0;
            new_valid <= 0;
            new_range <= 0;
            new_offset <= 0;
            new_chunks <= 0;
            new_frames <= 0;
            absolute_start <= 0; slice_available <= 0; slice_end <= 0; slice_out_of_bounds <= 0; slice_length_valid <= 0; slice_capacity_valid <= 0;
            append_end <= 0; append_chunks <= 0; required_chunks <= 0; growth <= 0; append_extent <= 0;
            append_capacity_ok <= 0;
            range_chunks <= 0; retained_end <= 0; retained_frames <= 0;
            range_offset <= 0;
            length_last <= 0; span_until_wrap <= 0; span_contiguous <= 0;
            fresh <= 0;
            arena_bound <= 0;
        end else begin
            done <= 0;
            if (cfg_pending) cfg_read_data <= cfg_result;
            if (descriptor_write && advance) valid_views[descriptor_write_address] <= descriptor_write_data[0];
            if (advance && metadata_error == 0) begin
            if (cm_command && cm_cmd_ready) arena_bound <= 1;
            if (hint_valid && hint_ready && hint_base_valid) begin
                if (!arena_bound) base <= arena_base;
            end
            if (resolver_prefetch_valid && cm_prefetch_ready && prefetch_admission) arena_bound <= 1;
            cfg_request_valid <= cfg_accept;
            if (cfg_accept) begin
                cfg_request_write <= cfg_write; cfg_request_space <= cfg_space;
                cfg_request_address <= cfg_address; cfg_request_data <= cfg_write_data;
                cfg_request_in_range <= cfg_in_range; cfg_request_value_valid <= cfg_value_valid;
            end
            cfg_pending <= cfg_request_valid;
            cfg_result_valid <= cfg_pending;
            if (cfg_request_valid) begin
                cfg_globals_data <= 0;
                cfg_was_write <= cfg_request_write;
                cfg_response_space <= cfg_request_space;
                cfg_response_error <= !cfg_request_in_range || !cfg_request_value_valid;
                if (cfg_request_in_range && cfg_request_value_valid && (cfg_request_space == 0)) begin
                    case (cfg_request_address)
                        0: if (cfg_request_write) view_count <= cfg_request_data[31:0]; else cfg_globals_data <= {128'b0, view_count};
                        1: if (cfg_request_write) slot_count <= cfg_request_data[31:0]; else cfg_globals_data <= {128'b0, slot_count};
                        2: if (cfg_request_write) free_count <= cfg_request_data[31:0]; else cfg_globals_data <= {128'b0, free_count};
                        3: if (cfg_request_write) slot_limit <= cfg_request_data[31:0]; else cfg_globals_data <= {128'b0, slot_limit};
                        default: cfg_globals_data <= 0;
                    endcase
                end
                if (cfg_request_in_range && cfg_request_value_valid && cfg_request_space == 6) begin
                    case (cfg_request_address)
                        0: cfg_globals_data <= {128'b0,retired_count};
                        1: cfg_globals_data <= {96'b0,retired_references};
                        2: cfg_globals_data <= {128'b0,retired_high_water};
                        3: cfg_globals_data <= {96'b0,reclaim_blocks};
                        4: cfg_globals_data <= {96'b0,reclaim_freed};
                        5: cfg_globals_data <= {96'b0,reclaim_cycles};
                        6: cfg_globals_data <= {128'b0,reclaim_block};
                        7: cfg_globals_data <= {158'b0,reclaim_released,reclaim_active};
                        8: cfg_globals_data <= {128'b0,32'(RETIRED_TABLES)};
                        9: cfg_globals_data <= {128'b0,retired_head};
                        10: cfg_globals_data <= {128'b0,retired_tail};
                        11: cfg_globals_data <= {128'b0,reclaim_pages_used};
                        12: cfg_globals_data <= {128'b0,reclaim_pages_available};
                        13: cfg_globals_data <= {96'b0,reclaim_capacity_failures};
                        14: cfg_globals_data <= {96'b0,reclaim_pages_freed};
                        16: cfg_globals_data <= {views_used,32'(TABLE_PAGES),reclaim_pages_used,slot_limit,slot_count-free_count};
                        17: cfg_globals_data <= {32'(VIEWS),32'(BLOCK_CHUNKS),32'(CHUNK_FRAMES),ARENA_BYTES};
                        default: begin end
                    endcase
                end
            end
            case (phase)
                IDLE: if (cmd_valid && cmd_ready) begin
                    opcode <= cmd_opcode;
                    view_index <= cmd_view[31:0];
                    start <= cmd_start;
                    length <= cmd_length;
                    count <= cmd_count;
                    admission_base_valid <= hint_base_valid;
                    admission_start_valid <= cmd_start[63:32] == 0;
                    admission_view_greater <= cmd_view > {32'b0,view_count};
                    admission_view_at_end <= cmd_view == {32'b0,view_count};
                    admission_view_outside <= cmd_view >= {32'b0,view_count};
                    admission_view_full <= cmd_view >= 64'(VIEWS);
                    if (!arena_bound) base <= arena_base;
                    error_code <= OK;
                    result_view <= 0;
                    result_empty <= 0;
                    index <= 0;
                    sample_index <= 0;
                    sample_last <= cmd_count-1'b1; samples_finished <= 0;
                    if (manual_grant) done <= 1;
                    else enter_phase(ADMIT);
                end
                ADMIT: begin
                    if (!admission_base_valid) begin
                        error_code <= INVALID_SHAPE;
                        enter_phase(FINISH);
                    end else if ((opcode == 9 || opcode == 10) && !admission_start_valid) begin error_code <= INVALID_SHAPE; enter_phase(FINISH); end
                    else if (opcode == 9) begin done <= 1; enter_phase(IDLE); end
                    else if (opcode == 10) enter_phase(DRAIN_WAIT);
                    else if (opcode == 8) begin
                        opcode <= CLEAR;
                        if (admission_view_greater) begin error_code <= INVALID_VIEW; enter_phase(FINISH); end
                        else if (admission_view_full) begin error_code <= MEMORY_FULL; enter_phase(FINISH); end
                        else if (admission_view_at_end) begin
                            new_valid <= 1; new_range <= 0; new_offset <= 0; new_frames <= 0; new_chunks <= 0;
                            enter_phase(BEGIN_CREATE);
                        end else enter_phase(DESCRIPTOR_READ);
                    end else if (opcode == CREATE) begin
                        new_valid <= 1; new_range <= 0; new_offset <= 0; new_frames <= 0; new_chunks <= 0;
                        allocation_view <= REUSE_VIEWS != 0 ? 32'b0 : view_count; enter_phase(FREE_VIEW_READ);
                    end else if (opcode == FENCE) begin
                            enter_phase(FENCE_REQUEST);
                    end else if (admission_view_outside) begin
                        error_code <= INVALID_VIEW;
                        enter_phase(FINISH);
                    end else enter_phase(DESCRIPTOR_READ);
                end
                FREE_VIEW_READ: begin
                    if (allocation_view == view_count) begin
                        if (view_count == 32'(VIEWS)) begin error_code <= MEMORY_FULL; enter_phase(FINISH); end
                        else enter_phase(opcode == CREATE ? PUBLISH : SLICE_CAPACITY);
                    end else enter_phase(FREE_VIEW_RESULT);
                end
                FREE_VIEW_RESULT: begin
                    if (!foreground_descriptor[0]) enter_phase(opcode == CREATE ? PUBLISH : SLICE_CAPACITY);
                    else begin allocation_view <= allocation_view+1; enter_phase(FREE_VIEW_READ); end
                end
                DESCRIPTOR_READ: begin
                    length_last <= length-1'b1;
                    enter_phase(DESCRIPTOR_RESULT);
                end
                DESCRIPTOR_RESULT: begin
                    descriptor <= foreground_descriptor;
                    if (!foreground_descriptor[0]) begin
                        error_code <= INVALID_VIEW; enter_phase(FINISH);
                    end else enter_phase(CHECK);
                end
                CHECK: begin
                    new_valid <= opcode != REMOVE;
                    new_range <= descriptor[1];
                    new_offset <= 0;
                    new_frames <= 0;
                    new_chunks <= 0;
                    case (opcode)
                        SLICE: enter_phase(SLICE_START);
                        CLEAR, REMOVE: begin
                            remaining <= chunks;
                            enter_phase((chunks == 0) ? PUBLISH : DETACH_ROOT);
                        end
                        RECLAIM: enter_phase(RECLAIM_END);
                        APPEND: if (descriptor[1]) begin
                            error_code <= INVALID_STATE;
                            enter_phase(FINISH);
                        end else enter_phase(APPEND_END);
                        READ: if ((length == 0) || (length > frames) || (start >= length)) begin
                            error_code <= INVALID_VIEW;
                            enter_phase(FINISH);
                        end else begin
                            cursor <= start;
                            enter_phase((count == 0) ? FINISH : SAMPLE_TABLE_READ);
                        end
                        default: begin error_code <= INVALID_SHAPE; enter_phase(FINISH); end
                    endcase
                end
                APPEND_END: begin append_end <= frames+{32'b0,count}; enter_phase(APPEND_CHUNKS); end
                APPEND_CHUNKS: begin append_chunks <= (append_end+BLOCK_FRAMES-1) >> UNIT_SHIFT; enter_phase(APPEND_REQUIRED); end
                APPEND_REQUIRED: begin required_chunks <= append_chunks-{32'b0,chunks}; enter_phase(APPEND_GROWTH); end
                APPEND_GROWTH: begin
                    growth <= required_chunks > {32'b0,free_count} ? required_chunks-{32'b0,free_count} : 64'b0;
                    enter_phase(APPEND_EXTENT);
                end
                APPEND_EXTENT: begin append_extent <= {32'b0,slot_count}+growth; enter_phase(APPEND_VALIDATE); end
                APPEND_VALIDATE: begin
                    append_capacity_ok <= append_chunks <= {32'b0,slot_limit} && append_extent <= {32'b0,slot_limit};
                    enter_phase(APPEND_CHECK);
                end
                APPEND_CHECK: begin
                    if (!append_capacity_ok) begin
                        error_code <= MEMORY_FULL; enter_phase(FINISH);
                    end else if (count == 0) enter_phase(FINISH);
                    else begin
                        new_frames <= append_end; new_chunks <= append_chunks[31:0];
                        append_index <= chunks; remaining <= required_chunks[31:0]; cursor <= frames;
                        enter_phase(ALLOCATE);
                    end
                end
                SLICE_START: begin
                    slice_out_of_bounds <= start > frames;
                    slice_available <= frames-start;
                    absolute_start <= {32'b0,offset}+start;
                    enter_phase(SLICE_OFFSET);
                end
                SLICE_OFFSET: begin
                    slice_length_valid <= length <= slice_available;
                    range_offset <= length == 0 ? 32'b0 : (absolute_start[31:0] & 32'(BLOCK_FRAMES-1));
                    enter_phase(SLICE_END);
                end
                SLICE_END: begin slice_end <= {32'b0,range_offset}+length; enter_phase(SLICE_CHUNKS); end
                SLICE_CHUNKS: begin
                    range_chunks <= slice_chunk_result[31:0]; slice_capacity_valid <= slice_chunk_result <= 64'(BLOCKS);
                    enter_phase(SLICE_CHECK);
                end
                SLICE_CHECK: begin
                    if (slice_out_of_bounds || !slice_length_valid || !slice_capacity_valid) begin
                        error_code <= INVALID_VIEW; enter_phase(FINISH);
                    end else begin
                        new_range <= 1; new_offset <= range_offset; new_frames <= length;
                        new_chunks <= range_chunks[31:0]; first_chunk <= 32'(absolute_start >> UNIT_SHIFT);
                        remaining <= range_chunks[31:0]; allocation_view <= REUSE_VIEWS != 0 ? 32'b0 : view_count; enter_phase(FREE_VIEW_READ);
                    end
                end
                SLICE_CAPACITY: enter_phase(length == 0 ? PUBLISH : TABLE_READ);
                RECLAIM_END: begin
                    retained_end <= chunks == 0 ? 64'b0 : (64'(chunks-1) << UNIT_SHIFT);
                    enter_phase(RECLAIM_FRAMES);
                end
                RECLAIM_FRAMES: begin
                    retained_frames <= retained_end > {32'b0,offset} ? retained_end-{32'b0,offset} : 64'b0;
                    enter_phase(RECLAIM_CHECK);
                end
                RECLAIM_CHECK: begin
                    new_chunks <= chunks == 0 ? 0 : chunks-1;
                    new_offset <= chunks <= 1 ? 0 : offset;
                    new_frames <= frames < retained_frames ? frames : retained_frames;
                    result_empty <= chunks <= 1; index <= chunks == 0 ? 0 : chunks-1;
                    remaining <= chunks == 0 ? 0 : 1;
                    enter_phase(chunks == 0 ? PUBLISH : TABLE_READ);
                end
                DETACH_ROOT: enter_phase(PUBLISH);
                DRAIN_WAIT: if (retired_count == 0 && !reclaim_active && !metadata_pending && !gc_retire_busy) enter_phase(FINISH);
                TABLE_READ: enter_phase(TABLE_RESULT);
                TABLE_RESULT: begin
                    chunk <= table_data;
                    enter_phase(REF_READ);
                end
                REF_READ: begin
                    if (chunk >= slot_count) begin error_code <= INVALID_VIEW; enter_phase(FINISH); end
                    else enter_phase(REF_RESULT);
                end
                REF_RESULT: begin
                    if ((reference_data == 0) || ((opcode == SLICE) && (reference_data == 32'hffffffff))) begin
                        error_code <= INVALID_STATE;
                        faulted <= 1;
                        enter_phase(FINISH);
                    end else enter_phase(opcode != SLICE && reference_data == 1 ? RETIRE_REQUEST : UPDATE_REFERENCE);
                end
                UPDATE_REFERENCE: begin
                    if ((opcode != SLICE) && (reference_data == 1)) free_count <= free_count + 1;
                    remaining <= remaining - 1;
                    index <= index + 1;
                    enter_phase((remaining == 1) ? PUBLISH : TABLE_READ);
                end
                BEGIN_CREATE: begin
                    view_count <= view_count+1'b1;
                    enter_phase(FINISH);
                end
                PUBLISH: begin
                    if ((opcode == CREATE) || (opcode == SLICE)) begin
                        result_view <= allocation_view;
                        if (allocation_view == view_count) view_count <= view_count + 1;
                    end
                    enter_phase(FINISH);
                end
                ALLOCATE: if (remaining == 0) enter_phase(SAMPLE_TABLE_READ);
                    else if (free_count != 0) begin
                        enter_phase(FREE_RESULT);
                    end else begin
                        chunk <= slot_count;
                        slot_count <= slot_count + 1;
                        fresh <= 1;
                        enter_phase(ALLOCATE_WRITE);
                    end
                FREE_RESULT: begin
                    chunk <= free_data;
                    free_count <= free_count - 1;
                    fresh <= 0;
                    enter_phase(ALLOCATE_WRITE);
                end
                ALLOCATE_WRITE: begin
                    append_index <= append_index + 1;
                    remaining <= remaining - 1;
                    zero_index <= 0;
                    enter_phase(fresh && BLOCK_CHUNKS == 1 ? ZERO_REQUEST : ALLOCATE);
                end
                ZERO_REQUEST: if (cm_cmd_ready) enter_phase(ZERO_DATA);
                ZERO_DATA: if (cm_write_ready) begin
                    zero_index <= zero_index+1'b1;
                    if (zero_index == SPAN_BITS'(CHUNK_FRAMES-1)) enter_phase(ZERO_WAIT);
                end
                ZERO_WAIT: if (cm_response_valid) begin
                    if (cm_response_error != 0) begin
                        error_code <= cm_response_error; enter_phase(FINISH);
                        faulted <= 1;
                    end
                    else enter_phase(ALLOCATE);
                end
                SAMPLE_TABLE_READ: begin
                    sample_offset <= OFFSET_BITS'(absolute_sample & 64'(CHUNK_FRAMES-1));
                    span_until_wrap <= length-cursor;
                    span_contiguous <= contiguous;
                    enter_phase(SAMPLE_TABLE_RESULT);
                end
                SAMPLE_TABLE_RESULT: begin
                    chunk <= (table_data << BLOCK_SHIFT) | (32'(absolute_sample >> SHIFT) & 32'(BLOCK_CHUNKS-1));
                    sample_block <= table_data;
                    span_count <= SPAN_BITS'(next_span); span_index <= 0;
                    enter_phase(SAMPLE_REQUEST);
                end
                SAMPLE_REQUEST: begin
                    if (sample_block >= slot_count) begin error_code <= INVALID_VIEW; enter_phase(FINISH); end
                    else if (cm_cmd_ready) enter_phase(SAMPLE_DATA);
                end
                SAMPLE_DATA: if ((cm_data_read && cm_read_valid && read_ready) || (cm_data_write && write_valid && cm_write_ready)) begin
                    sample_index <= sample_index + 1;
                    samples_finished <= sample_index == sample_last;
                    span_index <= span_index+1'b1;
                    cursor <= opcode == READ && cursor == length_last ? 64'b0 : cursor+1;
                    if (span_index+1'b1 == span_count) enter_phase(SAMPLE_WAIT);
                end
                SAMPLE_WAIT: if (cm_response_valid) begin
                    if (cm_response_error != 0) begin
                        error_code <= cm_response_error; enter_phase(FINISH);
                        faulted <= 1;
                    end
                    else enter_phase(samples_finished ? (opcode == READ ? FINISH : PUBLISH) : SAMPLE_TABLE_READ);
                end
                RETIRE_REQUEST: if (cm_cmd_ready) enter_phase(RETIRE_WAIT);
                RETIRE_WAIT: if (cm_response_valid) begin
                    if (cm_response_error != 0) begin
                        error_code <= cm_response_error; enter_phase(FINISH);
                        faulted <= 1;
                    end
                    else enter_phase(UPDATE_REFERENCE);
                end
                FENCE_REQUEST: if (cm_cmd_ready) enter_phase(FENCE_WAIT);
                FENCE_WAIT: if (cm_response_valid) begin
                    error_code <= cm_response_error;
                    if (cm_response_error != 0) faulted <= 1;
                    enter_phase(cm_response_error == 0 ? FENCE_METADATA : FINISH);
                end
                FENCE_METADATA: enter_phase(FINISH);
                FINISH: begin done <= 1; enter_phase(IDLE); end
                default: begin error_code <= INVALID_STATE; faulted <= 1; enter_phase(FINISH); end
            endcase
            end else if (advance && metadata_error != 0) begin
                error_code <= metadata_error;
                if (phase == IDLE) begin
                    cfg_request_valid <= 0; cfg_pending <= 1; cfg_result_valid <= 0;
                    cfg_response_error <= 1; cfg_was_write <= cfg_request_write;
                    cfg_response_space <= cfg_request_space;
                end else enter_phase(FINISH);
            end
            if (memory_meta_valid && memory_meta_ready && !arena_bound) begin
                base <= arena_base; arena_bound <= 1;
            end
            if (phase == FINISH && error_code == MEMORY_FULL) reclaim_capacity_failures <= reclaim_capacity_failures+1;
            if (gc_freed) free_count <= free_count+1;
            if (memory_fault) begin
                faulted <= 1; error_code <= MEMORY_FAULT;
                if (phase != IDLE && phase != FINISH) enter_phase(FINISH);
            end
        end
    end
endmodule
