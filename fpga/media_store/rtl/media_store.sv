module media_store #(
    parameter integer VIEWS = 32,
    parameter integer SLOTS = 32,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer READ_BUFFERS = 4,
    parameter integer WRITE_BUFFERS = 4,
    parameter integer VIEW_BITS = (VIEWS < 2) ? 1 : $clog2(VIEWS),
    parameter integer SLOT_BITS = (SLOTS < 2) ? 1 : $clog2(SLOTS),
    parameter integer TABLE_BITS = (VIEWS * SLOTS < 2) ? 1 : $clog2(VIEWS * SLOTS)
) (
    input wire clk, input wire reset,
    input wire [63:0] arena_base,
    input wire hint_valid, output wire hint_ready,
    input wire [63:0] hint_view, input wire [63:0] hint_position, input wire [63:0] hint_length,
    input wire [31:0] hint_count,
    input wire cmd_valid, output wire cmd_ready,
    input wire [2:0] cmd_opcode,
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
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data,
    input wire cfg_valid, output wire cfg_ready, input wire cfg_write,
    input wire [2:0] cfg_space, input wire [31:0] cfg_address,
    input wire [159:0] cfg_write_data,
    output wire cfg_response_valid, output reg cfg_response_error,
    output wire [159:0] cfg_read_data
);
    localparam [2:0] CREATE = 0, SLICE = 1, CLEAR = 2, REMOVE = 3,
        RECLAIM = 4, APPEND = 5, READ = 6, FENCE = 7;
    localparam [7:0] OK = 0, INVALID_VIEW = 1, INVALID_STATE = 2,
        MEMORY_FULL = 3, INVALID_SHAPE = 4, MEMORY_FAULT = 5;
    localparam integer SHIFT = $clog2(CHUNK_FRAMES);
    localparam [63:0] ARENA_BYTES = 64'(SLOTS) * 64'(CHUNK_FRAMES) * 4;
    typedef enum logic [5:0] {
        IDLE, DESCRIPTOR_READ, DESCRIPTOR_RESULT, CHECK,
        TABLE_READ, TABLE_RESULT, REF_READ, REF_RESULT, UPDATE_REFERENCE,
        PUBLISH, ALLOCATE, FREE_RESULT, ALLOCATE_WRITE,
        ZERO_REQUEST, ZERO_DATA, ZERO_WAIT, SAMPLE_TABLE_READ,
        SAMPLE_TABLE_RESULT, SAMPLE_REQUEST, SAMPLE_DATA, SAMPLE_WAIT,
        RETIRE_REQUEST, RETIRE_WAIT, FENCE_REQUEST, FENCE_WAIT,
        FINISH
    } phase_t;
    phase_t phase;
    reg descriptor_read, descriptor_write, table_read, table_write, ref_read, ref_write, free_read, free_write;
    reg [VIEW_BITS-1:0] descriptor_read_address, descriptor_write_address;
    reg [TABLE_BITS-1:0] table_read_address, table_write_address;
    reg [SLOT_BITS-1:0] ref_read_address, ref_write_address, free_read_address, free_write_address;
    reg [159:0] descriptor_write_data;
    reg [31:0] table_write_data, ref_write_data, free_write_data;
    wire [159:0] descriptor_data;
    wire [31:0] table_data, reference_data, free_data;
    reg [31:0] view_count, slot_count, free_count, slot_limit;
    reg cfg_pending, faulted, cfg_was_write;
    reg [2:0] cfg_response_space;
    reg [159:0] cfg_globals_data;
    reg [2:0] opcode;
    reg [31:0] view_index, count, index, first_chunk, remaining, append_index;
    reg [31:0] chunk, sample_index, zero_index, span_count, span_index;
    reg [63:0] base, start, length, cursor;
    reg [159:0] descriptor;
    wire [63:0] frames = descriptor[127:64];
    wire [31:0] offset = descriptor[63:32], chunks = descriptor[159:128];
    wire unused_descriptor = &{1'b0, descriptor[31:2], descriptor[0]};
    reg new_valid, new_range;
    reg [31:0] new_offset, new_chunks;
    reg [63:0] new_frames;
    reg fresh, arena_bound;
    wire [63:0] absolute_start = {32'b0, offset} + start;
    wire [63:0] append_end = frames + {32'b0, count};
    wire [63:0] append_chunks = (append_end + 64'(CHUNK_FRAMES-1)) >> SHIFT;
    wire [63:0] required_chunks = append_chunks - {32'b0, chunks};
    wire [63:0] growth = required_chunks > {32'b0, free_count}
        ? required_chunks - {32'b0, free_count} : 64'b0;
    wire [63:0] absolute_sample = {32'b0, offset} + cursor;
    wire [31:0] range_offset = (length == 0) ? 32'b0 : (absolute_start[31:0] & 32'(CHUNK_FRAMES-1));
    wire [63:0] range_chunks = ({32'b0, range_offset} + length + 64'(CHUNK_FRAMES-1)) >> SHIFT;
    wire [63:0] retained_end = (chunks == 0) ? 64'b0
        : (64'(chunks-1) << SHIFT);
    wire [63:0] retained_frames = retained_end > {32'b0, offset}
        ? retained_end - {32'b0, offset} : 64'b0;
    wire cfg_accept = cfg_valid && cfg_ready;
    wire cfg_in_range = (cfg_space == 0) ? cfg_address < 4
        : (cfg_space == 1) ? cfg_address < 32'(VIEWS)
        : (cfg_space == 2) ? cfg_address < 32'(VIEWS*SLOTS)
        : ((cfg_space == 3) || (cfg_space == 4)) && (cfg_address < 32'(SLOTS));
    wire cfg_value_valid = !cfg_write || (cfg_space == 1)
        || ((cfg_write_data[159:32] == 0)
            && ((cfg_space != 0) || (cfg_write_data[31:0] <= ((cfg_address == 0) ? 32'(VIEWS) : 32'(SLOTS)))));
    wire [31:0] control_ordinal = (opcode == SLICE) ? first_chunk + index : index;
    wire [TABLE_BITS-1:0] control_address = TABLE_BITS'(view_index * 32'(SLOTS) + control_ordinal);
    wire [TABLE_BITS-1:0] sample_address = TABLE_BITS'(view_index * 32'(SLOTS) + 32'(absolute_sample >> SHIFT));
    wire [31:0] word_offset = (phase == ZERO_REQUEST) ? zero_index : absolute_sample[31:0] & 32'(CHUNK_FRAMES-1);
    wire [31:0] samples_left = count-sample_index;
    wire [31:0] chunk_left = 32'(CHUNK_FRAMES) - (absolute_sample[31:0] & 32'(CHUNK_FRAMES-1));
    wire [31:0] contiguous = samples_left < chunk_left ? samples_left : chunk_left;
    wire [31:0] next_span = opcode == READ && (length-cursor) < {32'b0,contiguous} ? 32'(length-cursor) : contiguous;
    wire cm_cmd_ready, cm_write_ready, cm_read_valid, cm_read_last, cm_response_valid, cm_prefetch_ready;
    wire [31:0] cm_read_data, cm_response_id;
    wire [7:0] cm_response_error;
    wire unused_cm_status, cm_busy;
    wire resolver_ready, resolver_prefetch_valid;
    wire [31:0] resolver_prefetch_chunk, resolver_descriptor_address, resolver_table_address;
    wire unused_resolver_addresses = &{1'b0,resolver_descriptor_address,resolver_table_address};
    wire resolver_descriptor_valid, resolver_table_valid;
    wire descriptor_grant = resolver_descriptor_valid && !descriptor_read && !reset;
    wire table_grant = resolver_table_valid && !table_read && !reset;
    wire [VIEW_BITS-1:0] descriptor_ram_address = descriptor_read ? descriptor_read_address
        : resolver_descriptor_address[VIEW_BITS-1:0];
    wire [TABLE_BITS-1:0] table_ram_address = table_read ? table_read_address
        : resolver_table_address[TABLE_BITS-1:0];
    reg descriptor_response, table_response;
    wire hint_base_valid = arena_base[1:0] == 0 && arena_base <= 64'hffffffffffffffff-ARENA_BYTES
        && (!arena_bound || arena_base == base);
    wire hint_admission = !faulted && !reset && (arena_bound || (phase == IDLE && !cmd_valid));
    wire prefetch_admission = arena_bound || !cmd_valid;
    wire cm_data_read = phase == SAMPLE_DATA && opcode == READ;
    wire cm_data_write = phase == SAMPLE_DATA && opcode == APPEND;
    wire cm_response_ready = phase == ZERO_WAIT || phase == SAMPLE_WAIT || phase == RETIRE_WAIT || phase == FENCE_WAIT;
    wire cm_command = phase == ZERO_REQUEST || phase == SAMPLE_REQUEST || phase == RETIRE_REQUEST || phase == FENCE_REQUEST;
    wire [2:0] cm_opcode = phase == FENCE_REQUEST ? 3'd3 : phase == RETIRE_REQUEST ? 3'd4
        : phase == ZERO_REQUEST || opcode == APPEND ? 3'd1 : 3'd0;
    assign unused_cm_status = &{1'b0,cm_read_last,cm_response_id,cm_busy};

    function automatic [159:0] pack_view(input valid, input range_view,
        input [31:0] first_offset, input [63:0] frame_count, input [31:0] chunk_count);
        pack_view = {chunk_count, frame_count, first_offset, 30'b0, range_view, valid};
    endfunction

    initial begin
        if ((VIEWS < 1) || (VIEWS > 65536) || (SLOTS < 1) || (SLOTS > 65536)
            || (CHUNK_FRAMES < 1) || (CHUNK_FRAMES > 65536)
            || ((CHUNK_FRAMES & (CHUNK_FRAMES-1)) != 0)
            || (64'(VIEWS) * 64'(SLOTS) > 64'hffffffff))
            $fatal(1, "Invalid media store capacities");
    end
    assign busy = phase != IDLE;
    assign cmd_ready = !busy && !cfg_pending && !cfg_valid && !faulted && !reset;
    assign hint_ready = resolver_ready && hint_admission;
    assign cfg_ready = !busy && !cfg_pending && !reset;
    assign cfg_response_valid = cfg_pending && !reset;
    assign cfg_read_data = (cfg_response_error || cfg_was_write) ? 160'b0
        : (cfg_response_space == 0) ? cfg_globals_data
        : (cfg_response_space == 1) ? descriptor_data
        : (cfg_response_space == 2) ? {128'b0, table_data}
        : (cfg_response_space == 3) ? {128'b0, reference_data} : {128'b0, free_data};
    assign write_ready = cm_data_write && cm_write_ready && !reset;
    assign read_valid = cm_data_read && cm_read_valid && !reset;
    assign read_data = cm_read_data;

    chunk_memory #(.SLOTS(SLOTS), .CHUNK_FRAMES(CHUNK_FRAMES), .READ_BUFFERS(READ_BUFFERS), .WRITE_BUFFERS(WRITE_BUFFERS)) sample_memory (
        .clk(clk), .reset(reset), .arena_base(base),
        .cmd_valid(cm_command && !reset), .cmd_ready(cm_cmd_ready), .cmd_opcode(cm_opcode), .cmd_id(32'b0),
        .cmd_chunk(chunk), .cmd_offset(phase == ZERO_REQUEST ? 32'b0 : word_offset),
        .cmd_count(phase == ZERO_REQUEST ? 32'(CHUNK_FRAMES) : span_count),
        .write_valid((phase == ZERO_DATA || (cm_data_write && write_valid)) && !reset),
        .write_ready(cm_write_ready), .write_data(phase == ZERO_DATA ? 32'b0 : write_data),
        .read_valid(cm_read_valid), .read_ready(cm_data_read && read_ready), .read_data(cm_read_data), .read_last(cm_read_last),
        .response_valid(cm_response_valid), .response_ready(cm_response_ready), .response_id(cm_response_id), .response_error(cm_response_error),
        .prefetch_valid(resolver_prefetch_valid && prefetch_admission), .prefetch_ready(cm_prefetch_ready), .prefetch_chunk(resolver_prefetch_chunk),
        .busy(cm_busy), .fault(memory_fault),
        .memory_valid(memory_valid), .memory_ready(memory_ready), .memory_write(memory_write),
        .memory_address(memory_address), .memory_write_data(memory_write_data),
        .memory_response_valid(memory_response_valid), .memory_response_ready(memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data)
    );

    media_prefetch_resolver #(.SLOTS(SLOTS), .CHUNK_FRAMES(CHUNK_FRAMES)) resolver (
        .clk(clk), .reset(reset || faulted || memory_fault || (!arena_bound && cmd_valid && cmd_ready)),
        .hint_valid(hint_valid && hint_base_valid && hint_admission), .hint_ready(resolver_ready),
        .hint_view(hint_view), .hint_position(hint_position), .hint_length(hint_length), .hint_count(hint_count),
        .view_count(view_count), .slot_count(slot_count),
        .descriptor_valid(resolver_descriptor_valid), .descriptor_ready(descriptor_grant),
        .descriptor_address(resolver_descriptor_address), .descriptor_response(descriptor_response), .descriptor_data(descriptor_data),
        .table_valid(resolver_table_valid), .table_ready(table_grant), .table_address(resolver_table_address),
        .table_response(table_response), .table_data(table_data),
        .prefetch_valid(resolver_prefetch_valid), .prefetch_ready(cm_prefetch_ready && prefetch_admission), .prefetch_chunk(resolver_prefetch_chunk)
    );
    always @(posedge clk) begin
        if (reset) begin descriptor_response <= 0; table_response <= 0; end
        else begin descriptor_response <= descriptor_grant; table_response <= table_grant; end
    end

    media_ram #(.WIDTH(160), .DEPTH(VIEWS), .ADDR_BITS(VIEW_BITS)) descriptors (
        .clk(clk), .read_enable((descriptor_read || descriptor_grant) && !reset),
        .read_address(descriptor_ram_address), .read_data(descriptor_data),
        .write_enable(descriptor_write && !reset), .write_address(descriptor_write_address), .write_data(descriptor_write_data)
    );
    media_ram #(.DEPTH(VIEWS*SLOTS), .ADDR_BITS(TABLE_BITS)) tables (
        .clk(clk), .read_enable((table_read || table_grant) && !reset),
        .read_address(table_ram_address), .read_data(table_data),
        .write_enable(table_write && !reset), .write_address(table_write_address), .write_data(table_write_data)
    );
    media_ram #(.DEPTH(SLOTS), .ADDR_BITS(SLOT_BITS)) counts (
        .clk(clk), .read_enable(ref_read && !reset), .read_address(ref_read_address), .read_data(reference_data),
        .write_enable(ref_write && !reset), .write_address(ref_write_address), .write_data(ref_write_data)
    );
    media_ram #(.DEPTH(SLOTS), .ADDR_BITS(SLOT_BITS)) free_ids (
        .clk(clk), .read_enable(free_read && !reset), .read_address(free_read_address), .read_data(free_data),
        .write_enable(free_write && !reset), .write_address(free_write_address), .write_data(free_write_data)
    );

    always @* begin
        descriptor_read = 0; descriptor_write = 0; table_read = 0; table_write = 0;
        ref_read = 0; ref_write = 0; free_read = 0; free_write = 0;
        descriptor_read_address = VIEW_BITS'(view_index);
        descriptor_write_address = VIEW_BITS'(((opcode == CREATE) || (opcode == SLICE)) ? view_count : view_index);
        table_read_address = control_address;
        table_write_address = TABLE_BITS'(view_index * 32'(SLOTS) + append_index);
        ref_read_address = SLOT_BITS'(chunk); ref_write_address = SLOT_BITS'(chunk);
        free_read_address = SLOT_BITS'(free_count-1); free_write_address = SLOT_BITS'(free_count);
        descriptor_write_data = ((opcode == CREATE) || (opcode == SLICE) || new_valid)
            ? pack_view(new_valid, new_range, new_offset, new_frames, new_chunks) : 160'b0;
        table_write_data = chunk; ref_write_data = 1; free_write_data = chunk;
        case (phase)
            DESCRIPTOR_READ: descriptor_read = 1;
            TABLE_READ: table_read = 1;
            REF_READ: ref_read = 1;
            UPDATE_REFERENCE: begin
                ref_write = 1;
                if (opcode == SLICE) begin
                    ref_write_data = reference_data + 1;
                    table_write = 1;
                    table_write_address = TABLE_BITS'(view_count * 32'(SLOTS) + index);
                end else begin
                    ref_write_data = reference_data - 1;
                    free_write = reference_data == 1;
                end
            end
            PUBLISH: descriptor_write = 1;
            ALLOCATE: free_read = (remaining != 0) && (free_count != 0);
            ALLOCATE_WRITE: begin table_write = 1; ref_write = 1; end
            SAMPLE_TABLE_READ: begin table_read = 1; table_read_address = sample_address; end
            default: begin end
        endcase
        if (cfg_accept && cfg_in_range && cfg_value_valid) begin
            case (cfg_space)
                1: begin
                    descriptor_read = !cfg_write; descriptor_write = cfg_write;
                    descriptor_read_address = VIEW_BITS'(cfg_address); descriptor_write_address = VIEW_BITS'(cfg_address);
                    descriptor_write_data = cfg_write_data;
                end
                2: begin
                    table_read = !cfg_write; table_write = cfg_write;
                    table_read_address = TABLE_BITS'(cfg_address); table_write_address = TABLE_BITS'(cfg_address);
                    table_write_data = cfg_write_data[31:0];
                end
                3: begin
                    ref_read = !cfg_write; ref_write = cfg_write;
                    ref_read_address = SLOT_BITS'(cfg_address); ref_write_address = SLOT_BITS'(cfg_address);
                    ref_write_data = cfg_write_data[31:0];
                end
                4: begin
                    free_read = !cfg_write; free_write = cfg_write;
                    free_read_address = SLOT_BITS'(cfg_address); free_write_address = SLOT_BITS'(cfg_address);
                    free_write_data = cfg_write_data[31:0];
                end
                default: begin end
            endcase
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            done <= 0;
            error_code <= OK;
            result_view <= 0;
            result_empty <= 0;
            view_count <= 0;
            slot_count <= 0;
            free_count <= 0;
            slot_limit <= 32'(SLOTS);
            cfg_pending <= 0;
            cfg_response_error <= 0;
            cfg_globals_data <= 0;
            cfg_was_write <= 0;
            cfg_response_space <= 0;
            faulted <= 0;
            opcode <= CREATE;
            view_index <= 0;
            count <= 0;
            index <= 0;
            first_chunk <= 0;
            remaining <= 0;
            append_index <= 0;
            chunk <= 0;
            sample_index <= 0;
            zero_index <= 0;
            span_count <= 0; span_index <= 0;
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
            fresh <= 0;
            arena_bound <= 0;
        end else begin
            done <= 0;
            if (cm_command && cm_cmd_ready) arena_bound <= 1;
            if (hint_valid && hint_ready && hint_base_valid) begin
                if (!arena_bound) base <= arena_base;
            end
            if (resolver_prefetch_valid && cm_prefetch_ready && prefetch_admission) arena_bound <= 1;
            cfg_pending <= cfg_accept;
            if (cfg_accept) begin
                cfg_globals_data <= 0;
                cfg_was_write <= cfg_write;
                cfg_response_space <= cfg_space;
                cfg_response_error <= !cfg_in_range || !cfg_value_valid;
                if (cfg_in_range && cfg_value_valid && (cfg_space == 0)) begin
                    case (cfg_address)
                        0: if (cfg_write) view_count <= cfg_write_data[31:0]; else cfg_globals_data <= {128'b0, view_count};
                        1: if (cfg_write) slot_count <= cfg_write_data[31:0]; else cfg_globals_data <= {128'b0, slot_count};
                        2: if (cfg_write) free_count <= cfg_write_data[31:0]; else cfg_globals_data <= {128'b0, free_count};
                        3: if (cfg_write) slot_limit <= cfg_write_data[31:0]; else cfg_globals_data <= {128'b0, slot_limit};
                        default: cfg_globals_data <= 0;
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
                    if (!arena_bound) base <= arena_base;
                    error_code <= OK;
                    result_view <= 0;
                    result_empty <= 0;
                    index <= 0;
                    sample_index <= 0;
                    if ((arena_base[1:0] != 0) || (arena_base > 64'hffffffffffffffff - ARENA_BYTES)
                        || (arena_bound && arena_base != base)) begin
                        error_code <= INVALID_SHAPE;
                        phase <= FINISH;
                    end else if (cmd_opcode == CREATE) begin
                        if (view_count == 32'(VIEWS)) begin
                            error_code <= MEMORY_FULL;
                            phase <= FINISH;
                        end else begin
                            new_valid <= 1;
                            new_range <= 0;
                            new_offset <= 0;
                            new_frames <= 0;
                            new_chunks <= 0;
                            phase <= PUBLISH;
                        end
                    end else if (cmd_opcode == FENCE) begin
                            phase <= FENCE_REQUEST;
                    end else if (cmd_view >= {32'b0, view_count}) begin
                        error_code <= INVALID_VIEW;
                        phase <= FINISH;
                    end else phase <= DESCRIPTOR_READ;
                end
                DESCRIPTOR_READ: begin
                    phase <= DESCRIPTOR_RESULT;
                end
                DESCRIPTOR_RESULT: begin
                    descriptor <= descriptor_data;
                    if (!descriptor_data[0]) begin
                        error_code <= INVALID_VIEW; phase <= FINISH;
                    end else phase <= CHECK;
                end
                CHECK: begin
                    new_valid <= opcode != REMOVE;
                    new_range <= descriptor[1];
                    new_offset <= 0;
                    new_frames <= 0;
                    new_chunks <= 0;
                    case (opcode)
                        SLICE: if ((start > frames) || (length > frames - start) || (range_chunks > 64'(SLOTS))) begin
                            error_code <= INVALID_VIEW;
                            phase <= FINISH;
                        end else if (view_count == 32'(VIEWS)) begin
                            error_code <= MEMORY_FULL;
                            phase <= FINISH;
                        end else begin
                            new_range <= 1;
                            new_offset <= range_offset;
                            new_frames <= length;
                            new_chunks <= range_chunks[31:0];
                            first_chunk <= 32'(absolute_start >> SHIFT);
                            remaining <= range_chunks[31:0];
                            phase <= (length == 0) ? PUBLISH : TABLE_READ;
                        end
                        CLEAR, REMOVE: begin
                            remaining <= chunks;
                            phase <= (chunks == 0) ? PUBLISH : TABLE_READ;
                        end
                        RECLAIM: begin
                            new_chunks <= (chunks == 0) ? 0 : chunks - 1;
                            new_offset <= (chunks <= 1) ? 0 : offset;
                            new_frames <= (frames < retained_frames) ? frames : retained_frames;
                            result_empty <= chunks <= 1;
                            index <= (chunks == 0) ? 0 : chunks - 1;
                            remaining <= (chunks == 0) ? 0 : 1;
                            phase <= (chunks == 0) ? PUBLISH : TABLE_READ;
                        end
                        APPEND: if (descriptor[1]) begin
                            error_code <= INVALID_STATE;
                            phase <= FINISH;
                        end else if ((append_chunks > {32'b0, slot_limit}) || ({32'b0, slot_count} + growth > {32'b0, slot_limit})) begin
                            error_code <= MEMORY_FULL;
                            phase <= FINISH;
                        end else if (count == 0) phase <= FINISH;
                        else begin
                            new_frames <= append_end;
                            new_chunks <= append_chunks[31:0];
                            append_index <= chunks;
                            remaining <= required_chunks[31:0];
                            cursor <= frames;
                            phase <= ALLOCATE;
                        end
                        READ: if ((length == 0) || (length > frames) || (start >= length)) begin
                            error_code <= INVALID_VIEW;
                            phase <= FINISH;
                        end else begin
                            cursor <= start;
                            phase <= (count == 0) ? FINISH : SAMPLE_TABLE_READ;
                        end
                        default: begin error_code <= INVALID_SHAPE; phase <= FINISH; end
                    endcase
                end
                TABLE_READ: phase <= TABLE_RESULT;
                TABLE_RESULT: begin
                    chunk <= table_data;
                    if (table_data >= slot_count) begin error_code <= INVALID_VIEW; phase <= FINISH; end
                    else phase <= REF_READ;
                end
                REF_READ: phase <= REF_RESULT;
                REF_RESULT: begin
                    if ((reference_data == 0) || ((opcode == SLICE) && (reference_data == 32'hffffffff))) begin
                        error_code <= INVALID_STATE;
                        faulted <= 1;
                        phase <= FINISH;
                    end else phase <= opcode != SLICE && reference_data == 1 ? RETIRE_REQUEST : UPDATE_REFERENCE;
                end
                UPDATE_REFERENCE: begin
                    if ((opcode != SLICE) && (reference_data == 1)) free_count <= free_count + 1;
                    remaining <= remaining - 1;
                    index <= index + 1;
                    phase <= (remaining == 1) ? PUBLISH : TABLE_READ;
                end
                PUBLISH: begin
                    if ((opcode == CREATE) || (opcode == SLICE)) begin
                        result_view <= view_count;
                        view_count <= view_count + 1;
                    end
                    phase <= FINISH;
                end
                ALLOCATE: if (remaining == 0) phase <= SAMPLE_TABLE_READ;
                    else if (free_count != 0) begin
                        phase <= FREE_RESULT;
                    end else begin
                        chunk <= slot_count;
                        slot_count <= slot_count + 1;
                        fresh <= 1;
                        phase <= ALLOCATE_WRITE;
                    end
                FREE_RESULT: begin
                    chunk <= free_data;
                    free_count <= free_count - 1;
                    fresh <= 0;
                    phase <= ALLOCATE_WRITE;
                end
                ALLOCATE_WRITE: begin
                    append_index <= append_index + 1;
                    remaining <= remaining - 1;
                    zero_index <= 0;
                    phase <= fresh ? ZERO_REQUEST : ALLOCATE;
                end
                ZERO_REQUEST: if (cm_cmd_ready) phase <= ZERO_DATA;
                ZERO_DATA: if (cm_write_ready) begin
                    zero_index <= zero_index+1;
                    if (zero_index+1 == 32'(CHUNK_FRAMES)) phase <= ZERO_WAIT;
                end
                ZERO_WAIT: if (cm_response_valid) begin
                    if (cm_response_error != 0) begin
                        error_code <= cm_response_error; phase <= FINISH;
                        faulted <= 1;
                    end
                    else phase <= ALLOCATE;
                end
                SAMPLE_TABLE_READ: phase <= SAMPLE_TABLE_RESULT;
                SAMPLE_TABLE_RESULT: begin
                    chunk <= table_data;
                    span_count <= next_span; span_index <= 0;
                    if (table_data >= slot_count) begin error_code <= INVALID_VIEW; phase <= FINISH; end
                    else phase <= SAMPLE_REQUEST;
                end
                SAMPLE_REQUEST: if (cm_cmd_ready) phase <= SAMPLE_DATA;
                SAMPLE_DATA: if ((cm_data_read && cm_read_valid && read_ready) || (cm_data_write && write_valid && cm_write_ready)) begin
                    sample_index <= sample_index + 1;
                    span_index <= span_index+1;
                    cursor <= opcode == READ && cursor+1 == length ? 64'b0 : cursor+1;
                    if (span_index+1 == span_count) phase <= SAMPLE_WAIT;
                end
                SAMPLE_WAIT: if (cm_response_valid) begin
                    if (cm_response_error != 0) begin
                        error_code <= cm_response_error; phase <= FINISH;
                        faulted <= 1;
                    end
                    else phase <= sample_index == count ? (opcode == READ ? FINISH : PUBLISH) : SAMPLE_TABLE_READ;
                end
                RETIRE_REQUEST: if (cm_cmd_ready) phase <= RETIRE_WAIT;
                RETIRE_WAIT: if (cm_response_valid) begin
                    if (cm_response_error != 0) begin
                        error_code <= cm_response_error; phase <= FINISH;
                        faulted <= 1;
                    end
                    else phase <= UPDATE_REFERENCE;
                end
                FENCE_REQUEST: if (cm_cmd_ready) phase <= FENCE_WAIT;
                FENCE_WAIT: if (cm_response_valid) begin
                    error_code <= cm_response_error;
                    if (cm_response_error != 0) faulted <= 1;
                    phase <= FINISH;
                end
                FINISH: begin done <= 1; phase <= IDLE; end
                default: begin error_code <= INVALID_STATE; faulted <= 1; phase <= FINISH; end
            endcase
            if (memory_fault) begin
                faulted <= 1; error_code <= MEMORY_FAULT;
                if (phase != IDLE && phase != FINISH) phase <= FINISH;
            end
        end
    end
endmodule
