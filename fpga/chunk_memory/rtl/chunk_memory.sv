module chunk_memory #(
    parameter integer SLOTS = 32,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer READ_BUFFERS = 4,
    parameter integer WRITE_BUFFERS = 4,
    parameter integer BURST_MEMORY = 0,
    parameter integer RETIRE_CHUNKS = 1
) (
    input wire clk, input wire reset,
    input wire retire_valid, output wire retire_ready, input wire [31:0] retire_chunk,
    output reg retire_done, output reg [7:0] retire_error, output reg retire_busy,
    input wire [63:0] arena_base,
    input wire cmd_valid, output wire cmd_ready,
    input wire [2:0] cmd_opcode, input wire [31:0] cmd_id,
    input wire [31:0] cmd_chunk, input wire [31:0] cmd_offset, input wire [31:0] cmd_count,
    input wire write_valid, output wire write_ready, input wire [31:0] write_data,
    output wire read_valid, input wire read_ready,
    output reg [31:0] read_data, output wire read_last,
    output wire response_valid, input wire response_ready,
    output wire [31:0] response_id, output reg [7:0] response_error,
    input wire prefetch_valid, output wire prefetch_ready, input wire [31:0] prefetch_chunk,
    output wire busy, output reg fault,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    output wire [63:0] memory_arena_base,
    output wire [31:0] memory_count,
    output wire memory_write_valid, input wire memory_write_ready,
    input wire memory_read_valid, output wire memory_read_ready,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    localparam integer RETIRE_SHIFT = $clog2(RETIRE_CHUNKS);
    localparam [2:0] READ = 0, WRITE = 1, PREFETCH = 2, FENCE = 3, RETIRE = 4;
    localparam [7:0] INVALID_CHUNK = 1, INVALID_SHAPE = 4, MEMORY_FAULT = 5;
    localparam integer RADDR_BITS = (READ_BUFFERS*CHUNK_FRAMES < 2) ? 1 : $clog2(READ_BUFFERS*CHUNK_FRAMES);
    localparam integer WADDR_BITS = (WRITE_BUFFERS*CHUNK_FRAMES < 2) ? 1 : $clog2(WRITE_BUFFERS*CHUNK_FRAMES);
    localparam integer READ_LINE_BITS = READ_BUFFERS < 2 ? 1 : $clog2(READ_BUFFERS);
    localparam integer WRITE_LINE_BITS = WRITE_BUFFERS < 2 ? 1 : $clog2(WRITE_BUFFERS);
    localparam integer TAG_BITS = SLOTS < 2 ? 1 : $clog2(SLOTS);
    localparam integer SPAN_BITS = $clog2(CHUNK_FRAMES+1);
    localparam integer QUIET_BITS = $clog2(CHUNK_FRAMES+9);
    localparam [63:0] ARENA_BYTES = 64'(SLOTS)*64'(CHUNK_FRAMES)*4;
    typedef enum logic [3:0] { IDLE, CHECK, LOOKUP, WAIT_LINE, READ_STREAM,
        WRITE_DATA, COMMIT, FLUSH, RETIRING, RESPONSE } phase_t;
    typedef enum logic [3:0] { D_IDLE, D_READ_REQUEST, D_READ_WAIT,
        D_RAM_REQUEST, D_RAM_RESULT, D_WRITE_REQUEST, D_WRITE_WAIT,
        D_TRANSFER, D_STREAM_READ, D_STREAM_WRITE, D_STREAM_WAIT } dma_phase_t;
    phase_t phase;
    dma_phase_t dma_phase;
    reg [2:0] opcode;
    reg [31:0] request_id, line;
    reg [TAG_BITS-1:0] chunk;
    reg [SPAN_BITS-1:0] offset, count, index, read_issued;
    reg invalid_chunk_request, invalid_span_request, invalid_retire_request;
    reg read_pending, read_output_valid;
    reg use_write_pool;
    reg [63:0] requested_base, active_base;
    reg base_bound;
    reg [TAG_BITS-1:0] read_tag [0:READ_BUFFERS-1];
    reg [READ_BUFFERS-1:0] read_present, read_loaded;
    reg [TAG_BITS-1:0] write_tag [0:WRITE_BUFFERS-1];
    reg [WRITE_BUFFERS-1:0] write_present, write_loaded, dirty;
    reg [READ_LINE_BITS-1:0] next_read;
    reg [WRITE_LINE_BITS-1:0] next_write;
    reg [QUIET_BITS-1:0] quiet;
    reg hint_pending;
    reg [31:0] hint_chunk;
    reg dma_write, dma_write_pool;
    reg dma_speculative, dma_obsolete;
    reg [31:0] dma_line, dma_chunk, dma_data;
    reg [SPAN_BITS-1:0] dma_index;
    reg read_hit, write_hit, fill_found;
    wire read_victim, write_victim, flush_found;
    reg [31:0] read_hit_line, write_hit_line, fill_line;
    wire [31:0] read_victim_line, write_victim_line, flush_line;
    reg fill_write_pool, hint_hit, incoming_write_space;
    reg [31:0] dirty_count;
    reg retiring_dirty;
    reg background_dirty;
    reg [31:0] background_chunk;
    reg incoming_pressure, flush_requested;
    wire retirement_complete = retire_busy && phase == IDLE && !background_dirty
        && !(dma_active && (dma_chunk >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT));
    assign retire_ready = !reset && !fault && !retire_busy && phase != RETIRING
        && !(cmd_valid && cmd_opcode == RETIRE)
        && !((phase != IDLE && phase != RESPONSE) && (32'(chunk) >> RETIRE_SHIFT) == (retire_chunk >> RETIRE_SHIFT));
    integer i;
    wire dma_active = dma_phase != D_IDLE;
    wire read_pinned = !use_write_pool && (phase == WAIT_LINE || phase == READ_STREAM);
    wire write_claimed = opcode == WRITE && phase != IDLE && phase != RESPONSE;
    wire [SPAN_BITS:0] incoming_end = {1'b0,cmd_offset[SPAN_BITS-1:0]} + {1'b0,cmd_count[SPAN_BITS-1:0]};
    wire incoming_write = cmd_valid && cmd_opcode == WRITE && cmd_count != 0 && cmd_chunk < 32'(SLOTS)
        && cmd_offset < 32'(CHUNK_FRAMES) && (cmd_count >> SPAN_BITS) == 0
        && incoming_end <= (SPAN_BITS+1)'(CHUNK_FRAMES);
    wire need_space = (phase == LOOKUP && opcode == WRITE && !write_hit && !write_victim)
        || incoming_pressure;
    wire dma_response = memory_response_valid && memory_response_ready;
    wire supersede_fill = dma_active && dma_speculative && phase == LOOKUP && opcode == WRITE && dma_chunk == 32'(chunk);
    wire fill_word = (BURST_MEMORY != 0 ? memory_read_valid && memory_read_ready
        : dma_phase == D_READ_WAIT && dma_response && !memory_response_error)
        && !dma_obsolete && !supersede_fill;
    wire [31:0] read_a_data, read_b_data, write_a_data, write_b_data;
    always @(posedge clk) begin
        if (reset) begin incoming_pressure <= 0; flush_requested <= 0; end
        else begin
            incoming_pressure <= phase == IDLE && incoming_write && !incoming_write_space;
            flush_requested <= quiet >= QUIET_BITS'(CHUNK_FRAMES+8) || need_space || phase == FLUSH || phase == RETIRING || retire_busy;
        end
    end
    wire unused_read_b = &{1'b0,read_b_data};
    wire write_word = phase == WRITE_DATA && write_valid && write_ready;
    wire read_output_ready = !read_output_valid || read_ready;
    wire read_ram_ready = !read_pending || read_output_ready;
    wire read_issue = phase == READ_STREAM && read_issued < count && read_ram_ready && !fault && !reset;
    wire read_ram_reserved = phase == READ_STREAM && use_write_pool
        && (read_issue || (read_pending && !read_output_ready));
    wire fill_blocked = dma_write_pool && (write_word || read_ram_reserved);
    assign cmd_ready = phase == IDLE && !fault && !reset && !retirement_complete && (!incoming_write || incoming_write_space);
    assign write_ready = phase == WRITE_DATA && !fault && !reset;
    assign read_valid = phase == READ_STREAM && read_output_valid && !fault && !reset;
    assign read_last = index + 1 == count;
    assign response_valid = phase == RESPONSE && !reset;
    assign response_id = request_id;
    assign prefetch_ready = !hint_pending && !fault && !reset && phase != FLUSH && phase != RETIRING
        && (base_bound || (phase == IDLE && !cmd_valid))
        && !(phase == CHECK && opcode == RETIRE)
        && !(cmd_valid && (cmd_opcode == PREFETCH || cmd_opcode == FENCE || cmd_opcode == RETIRE));
    assign busy = phase != IDLE || dma_active || hint_pending || (dirty_count != 0) || retire_busy;
    assign memory_valid = !fault && !reset && (BURST_MEMORY != 0 ? dma_phase == D_TRANSFER
        : dma_phase == D_READ_REQUEST || dma_phase == D_WRITE_REQUEST);
    assign memory_write = dma_write;
    assign memory_address = active_base + (64'(dma_chunk)*64'(CHUNK_FRAMES)
        + (BURST_MEMORY != 0 ? 64'b0 : 64'(dma_index)))*4;
    assign memory_write_data = dma_data;
    assign memory_arena_base = active_base;
    assign memory_count = BURST_MEMORY != 0 ? 32'(CHUNK_FRAMES) : 32'd1;
    assign memory_write_valid = BURST_MEMORY != 0 && dma_phase == D_STREAM_WRITE && !fault && !reset;
    assign memory_read_ready = BURST_MEMORY != 0 && dma_phase == D_STREAM_READ && !fault && !reset
        && !fill_blocked;
    assign memory_response_ready = !fault && !reset && (BURST_MEMORY != 0
        ? dma_active && dma_phase != D_TRANSFER
        : (dma_phase == D_READ_WAIT && (!fill_blocked || memory_response_error)) || dma_phase == D_WRITE_WAIT);

    initial begin
        if (SLOTS < 1 || CHUNK_FRAMES < 1 || CHUNK_FRAMES > 65536
            || (CHUNK_FRAMES & (CHUNK_FRAMES-1)) != 0 || READ_BUFFERS < 1 || WRITE_BUFFERS < 1
            || READ_BUFFERS > 65536 || WRITE_BUFFERS > 65536
            || RETIRE_CHUNKS < 1 || RETIRE_CHUNKS > SLOTS || (RETIRE_CHUNKS & (RETIRE_CHUNKS-1)) != 0)
            $fatal(1,"Invalid chunk-memory capacities");
    end

    wire [READ_BUFFERS-1:0] read_free_mask, read_forward_mask, read_any_mask;
    wire [WRITE_BUFFERS-1:0] write_free_mask, write_forward_mask, write_any_mask, flush_forward_mask, flush_any_mask;
    generate for (genvar r=0; r<READ_BUFFERS; r=r+1) begin : read_candidates
        wire reserved = dma_active && !dma_write_pool && dma_line == 32'(r);
        assign read_free_mask[r] = !read_present[r] && !reserved;
        assign read_any_mask[r] = !(read_pinned && line == 32'(r)) && !reserved;
        if (r == 2**READ_LINE_BITS-1) begin : last_index
            assign read_forward_mask[r] = read_any_mask[r];
        end else begin : compare_index
            assign read_forward_mask[r] = read_any_mask[r] && READ_LINE_BITS'(r) >= next_read;
        end
    end
    for (genvar w=0; w<WRITE_BUFFERS; w=w+1) begin : write_candidates
        assign write_free_mask[w] = !write_present[w];
        assign write_any_mask[w] = write_loaded[w] && !dirty[w]
            && !(dma_active && dma_write_pool && dma_line == 32'(w));
        assign flush_any_mask[w] = dirty[w] && write_loaded[w]
            && !(write_claimed && write_tag[w] == chunk)
            && (phase != RETIRING || (32'(write_tag[w]) >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT));
        if (w == 2**WRITE_LINE_BITS-1) begin : last_index
            assign write_forward_mask[w] = write_any_mask[w];
            assign flush_forward_mask[w] = flush_any_mask[w];
        end else begin : compare_index
            assign write_forward_mask[w] = write_any_mask[w] && WRITE_LINE_BITS'(w) >= next_write;
            assign flush_forward_mask[w] = flush_any_mask[w] && WRITE_LINE_BITS'(w) >= next_write;
        end
    end endgenerate
    wire read_free_found, read_forward_found, read_any_found;
    wire write_free_found, write_forward_found, write_any_found, flush_forward_found, flush_any_found;
    wire [READ_LINE_BITS-1:0] read_free_line, read_forward_line, read_any_line;
    wire [WRITE_LINE_BITS-1:0] write_free_line, write_forward_line, write_any_line, flush_forward_line, flush_any_line;
    chunk_priority #(.LINES(READ_BUFFERS)) read_free_select(read_free_mask,read_free_found,read_free_line);
    chunk_priority #(.LINES(READ_BUFFERS)) read_forward_select(read_forward_mask,read_forward_found,read_forward_line);
    chunk_priority #(.LINES(READ_BUFFERS)) read_any_select(read_any_mask,read_any_found,read_any_line);
    chunk_priority #(.LINES(WRITE_BUFFERS)) write_free_select(write_free_mask,write_free_found,write_free_line);
    chunk_priority #(.LINES(WRITE_BUFFERS)) write_forward_select(write_forward_mask,write_forward_found,write_forward_line);
    chunk_priority #(.LINES(WRITE_BUFFERS)) write_any_select(write_any_mask,write_any_found,write_any_line);
    chunk_priority #(.LINES(WRITE_BUFFERS)) flush_forward_select(flush_forward_mask,flush_forward_found,flush_forward_line);
    chunk_priority #(.LINES(WRITE_BUFFERS)) flush_any_select(flush_any_mask,flush_any_found,flush_any_line);
    assign read_victim = read_free_found || read_forward_found || read_any_found;
    assign read_victim_line = read_free_found ? 32'(read_free_line) : read_forward_found ? 32'(read_forward_line) : 32'(read_any_line);
    assign write_victim = write_free_found || write_forward_found || write_any_found;
    assign write_victim_line = write_free_found ? 32'(write_free_line) : write_forward_found ? 32'(write_forward_line) : 32'(write_any_line);
    assign flush_found = flush_forward_found || flush_any_found;
    assign flush_line = flush_forward_found ? 32'(flush_forward_line) : 32'(flush_any_line);

    always @* begin
        read_hit = 0; write_hit = 0; read_hit_line = 0; write_hit_line = 0;
        fill_found = 0; fill_line = 0; fill_write_pool = 0;
        hint_hit = 0; dirty_count = 0; retiring_dirty = 0; background_dirty = 0; incoming_write_space = 0;
        for (i = 0; i < READ_BUFFERS; i = i+1) begin
            if (read_present[i] && read_tag[i] == chunk) begin read_hit = 1; read_hit_line = 32'(i); end
            if (read_present[i] && 32'(read_tag[i]) == hint_chunk) hint_hit = 1;
            if (read_present[i] && !read_loaded[i]
                && !(retire_busy && (32'(read_tag[i]) >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT))
                && !(phase == RETIRING && (32'(read_tag[i]) >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT))) begin fill_found = 1; fill_line = 32'(i); end
        end
        for (i = 0; i < WRITE_BUFFERS; i = i+1) begin
            if ((write_present[i] && 32'(write_tag[i]) == cmd_chunk) || !write_present[i]
                || (write_loaded[i] && !dirty[i] && !(dma_active && dma_write_pool && dma_line == 32'(i))))
                incoming_write_space = 1;
            if (write_present[i] && write_tag[i] == chunk) begin write_hit = 1; write_hit_line = 32'(i); end
            if (write_present[i] && 32'(write_tag[i]) == hint_chunk) hint_hit = 1;
            if (dirty[i]) dirty_count = dirty_count + 1;
            if (write_present[i] && dirty[i] && (32'(write_tag[i]) >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT)) retiring_dirty = 1;
            if (write_present[i] && dirty[i] && (32'(write_tag[i]) >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT)) background_dirty = 1;
            if (write_present[i] && !write_loaded[i]
                && !(retire_busy && (32'(write_tag[i]) >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT))
                && !(phase == RETIRING && (32'(write_tag[i]) >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT))) begin fill_found = 1; fill_line = 32'(i); fill_write_pool = 1; end
        end
    end

    (* KEEP = "yes" *) wire [WADDR_BITS-1:0] write_pool_read_address = WADDR_BITS'(line*32'(CHUNK_FRAMES)+32'(offset)+32'(read_issued));
    (* KEEP = "yes" *) wire [WADDR_BITS-1:0] write_pool_write_address = WADDR_BITS'(line*32'(CHUNK_FRAMES)+32'(offset)+32'(index));
    wire [WADDR_BITS-1:0] write_pool_fill_address = WADDR_BITS'(dma_line*32'(CHUNK_FRAMES)+32'(dma_index));
    chunk_ram #(.DEPTH(READ_BUFFERS*CHUNK_FRAMES), .ADDR_BITS(RADDR_BITS)) read_ram (
        .clk(clk), .a_enable(read_issue && !use_write_pool), .a_write(1'b0),
        .a_address(RADDR_BITS'(line*32'(CHUNK_FRAMES)+32'(offset)+32'(read_issued))), .a_write_data(32'b0), .a_read_data(read_a_data),
        .b_enable(!reset && fill_word && !dma_write_pool), .b_write(1'b1),
        .b_address(RADDR_BITS'(dma_line*32'(CHUNK_FRAMES)+32'(dma_index))), .b_write_data(memory_read_data), .b_read_data(read_b_data)
    );
    chunk_ram #(.DEPTH(WRITE_BUFFERS*CHUNK_FRAMES), .ADDR_BITS(WADDR_BITS)) write_ram (
        .clk(clk), .a_enable(!reset && (write_word || (read_issue && use_write_pool) || (fill_word && dma_write_pool))),
        .a_write(write_word || (fill_word && dma_write_pool)),
        .a_address((fill_word && dma_write_pool) ? write_pool_fill_address
            : read_issue ? write_pool_read_address : write_pool_write_address),
        .a_write_data(fill_word && dma_write_pool ? memory_read_data : write_data), .a_read_data(write_a_data),
        .b_enable(!reset && dma_phase == D_RAM_REQUEST), .b_write(1'b0),
        .b_address(WADDR_BITS'(dma_line*32'(CHUNK_FRAMES)+32'(dma_index))), .b_write_data(32'b0), .b_read_data(write_b_data)
    );

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; dma_phase <= D_IDLE; fault <= 0;
            opcode <= READ; request_id <= 0; chunk <= 0; offset <= 0; count <= 0; index <= 0; line <= 0;
            invalid_chunk_request <= 0; invalid_span_request <= 0; invalid_retire_request <= 0;
            read_issued <= 0; read_pending <= 0; read_output_valid <= 0;
            use_write_pool <= 0; requested_base <= 0; active_base <= 0; base_bound <= 0;
            read_present <= 0; read_loaded <= 0; write_present <= 0; write_loaded <= 0; dirty <= 0;
            next_read <= 0; next_write <= 0; quiet <= 0; hint_pending <= 0; hint_chunk <= 0;
            dma_write <= 0; dma_write_pool <= 0; dma_line <= 0; dma_chunk <= 0; dma_index <= 0; dma_data <= 0;
            dma_speculative <= 0; dma_obsolete <= 0;
            response_error <= 0; read_data <= 0;
            retire_busy <= 0; retire_done <= 0; retire_error <= 0; background_chunk <= 0;
            for (integer r = 0; r < READ_BUFFERS; r = r+1) read_tag[r] <= 0;
            for (integer w = 0; w < WRITE_BUFFERS; w = w+1) write_tag[w] <= 0;
        end else begin
            retire_done <= 0;
            if (retire_valid && retire_ready) begin
                if (retire_chunk >= 32'(SLOTS) || (retire_chunk & 32'(RETIRE_CHUNKS-1)) != 0) begin
                    retire_done <= 1; retire_error <= INVALID_CHUNK;
                end else begin
                    retire_busy <= 1; retire_error <= 0; background_chunk <= retire_chunk;
                    if (hint_pending && (hint_chunk >> RETIRE_SHIFT) == (retire_chunk >> RETIRE_SHIFT)) hint_pending <= 0;
                end
            end
            if (quiet < QUIET_BITS'(CHUNK_FRAMES+8)) quiet <= quiet + 1'b1;
            if (prefetch_valid && prefetch_ready && prefetch_chunk < 32'(SLOTS)
                && !(retire_busy && (prefetch_chunk >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT))
                && arena_base[1:0] == 0 && arena_base <= 64'hffffffffffffffff-ARENA_BYTES
                && (!base_bound || arena_base == active_base)) begin
                hint_pending <= 1; hint_chunk <= prefetch_chunk;
                if (!base_bound) begin active_base <= arena_base; base_bound <= 1; end
            end
            if (supersede_fill) dma_obsolete <= 1;
            case (phase)
                IDLE: if (cmd_valid && cmd_ready) begin
                    opcode <= cmd_opcode; request_id <= cmd_id; chunk <= TAG_BITS'(cmd_chunk);
                    offset <= SPAN_BITS'(cmd_offset); count <= SPAN_BITS'(cmd_count); index <= 0; requested_base <= arena_base;
                    invalid_chunk_request <= cmd_chunk >= 32'(SLOTS);
                    invalid_span_request <= cmd_offset > 32'(CHUNK_FRAMES) || cmd_count > 32'(CHUNK_FRAMES)-cmd_offset;
                    invalid_retire_request <= (cmd_chunk & 32'(RETIRE_CHUNKS-1)) != 0
                        || {32'b0,cmd_chunk}+64'(RETIRE_CHUNKS) > 64'(SLOTS);
                    read_issued <= 0; read_pending <= 0; read_output_valid <= 0;
                    response_error <= 0; phase <= CHECK;
                    if (cmd_opcode == WRITE) quiet <= 0;
                end
                CHECK: begin
                    if (opcode > RETIRE || requested_base[1:0] != 0
                        || requested_base > 64'hffffffffffffffff-ARENA_BYTES
                        || (base_bound && requested_base != active_base)) begin
                        response_error <= INVALID_SHAPE; phase <= RESPONSE;
                    end else if (opcode != FENCE && (invalid_chunk_request
                        || (retire_busy && (32'(chunk) >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT)))) begin
                        response_error <= INVALID_CHUNK; phase <= RESPONSE;
                    end else if (opcode == RETIRE && invalid_retire_request) begin
                        response_error <= INVALID_SHAPE; phase <= RESPONSE;
                    end else if ((opcode == READ || opcode == WRITE)
                        && invalid_span_request) begin
                        response_error <= INVALID_SHAPE; phase <= RESPONSE;
                    end else begin
                        active_base <= requested_base; base_bound <= 1;
                        if (opcode == FENCE) begin hint_pending <= 0; phase <= FLUSH; end
                        else if (opcode == RETIRE) begin
                            if (hint_pending && (hint_chunk >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT)) hint_pending <= 0;
                            phase <= RETIRING;
                        end else if (opcode == PREFETCH) begin
                            if (!hint_pending) begin hint_pending <= 1; hint_chunk <= 32'(chunk); end
                            phase <= RESPONSE;
                        end else begin
                            if (opcode == WRITE && count != 0 && hint_pending && hint_chunk == 32'(chunk)) hint_pending <= 0;
                            phase <= count == 0 ? RESPONSE : LOOKUP;
                        end
                    end
                end
                LOOKUP: begin
                    if (opcode == WRITE && dma_active && dma_chunk == 32'(chunk) && !dma_speculative) begin end
                    else if (write_hit) begin
                        line <= write_hit_line; use_write_pool <= 1;
                        phase <= write_loaded[write_hit_line] ? ((opcode == WRITE) ? WRITE_DATA : READ_STREAM) : WAIT_LINE;
                        if (opcode == WRITE) for (integer r = 0; r < READ_BUFFERS; r = r+1)
                            if (read_present[r] && read_tag[r] == chunk) begin read_present[r] <= 0; read_loaded[r] <= 0; end
                    end else if (opcode == READ && read_hit) begin
                        line <= read_hit_line; use_write_pool <= 0;
                        phase <= read_loaded[read_hit_line] ? READ_STREAM : WAIT_LINE;
                    end else if (opcode == READ && read_victim) begin
                        line <= read_victim_line; use_write_pool <= 0;
                        read_present[read_victim_line] <= 1; read_loaded[read_victim_line] <= 0;
                        read_tag[read_victim_line] <= TAG_BITS'(chunk);
                        next_read <= READ_LINE_BITS'(read_victim_line + 1 == 32'(READ_BUFFERS) ? 0 : read_victim_line+1);
                        phase <= WAIT_LINE;
                    end else if (opcode == WRITE && write_victim) begin
                        line <= write_victim_line; use_write_pool <= 1;
                        write_present[write_victim_line] <= 1; write_tag[write_victim_line] <= TAG_BITS'(chunk);
                        dirty[write_victim_line] <= 0;
                        write_loaded[write_victim_line] <= count == SPAN_BITS'(CHUNK_FRAMES);
                        next_write <= WRITE_LINE_BITS'(write_victim_line + 1 == 32'(WRITE_BUFFERS) ? 0 : write_victim_line+1);
                        for (integer r = 0; r < READ_BUFFERS; r = r+1)
                            if (read_present[r] && read_tag[r] == chunk) begin read_present[r] <= 0; read_loaded[r] <= 0; end
                        phase <= count == SPAN_BITS'(CHUNK_FRAMES) ? WRITE_DATA : WAIT_LINE;
                    end
                end
                WAIT_LINE: if (use_write_pool ? write_loaded[line] : read_loaded[line])
                    phase <= opcode == WRITE ? WRITE_DATA : READ_STREAM;
                READ_STREAM: begin
                    if (read_output_ready) begin
                        read_output_valid <= read_pending;
                        if (read_pending) read_data <= use_write_pool ? write_a_data : read_a_data;
                    end
                    if (read_ram_ready) read_pending <= read_issue;
                    if (read_issue) read_issued <= read_issued+1;
                    if (read_valid && read_ready) begin
                        index <= index+1;
                        if (read_last) phase <= RESPONSE;
                    end
                end
                WRITE_DATA: if (write_word) begin
                    quiet <= 0; index <= index + 1;
                    if (index+1 == count) phase <= COMMIT;
                end
                COMMIT: begin dirty[line] <= 1; phase <= RESPONSE; end
                FLUSH: begin
                    hint_pending <= 0;
                    if (!dma_active && dirty_count == 0) phase <= RESPONSE;
                end
                RETIRING: if (!(dma_active && (dma_chunk >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT))
                    && !retiring_dirty) begin
                    for (integer r = 0; r < READ_BUFFERS; r = r+1)
                        if (read_present[r] && (32'(read_tag[r]) >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT)) begin read_present[r] <= 0; read_loaded[r] <= 0; end
                    for (integer w = 0; w < WRITE_BUFFERS; w = w+1)
                        if (write_present[w] && (32'(write_tag[w]) >> RETIRE_SHIFT) == (32'(chunk) >> RETIRE_SHIFT)) begin write_present[w] <= 0; write_loaded[w] <= 0; end
                    phase <= RESPONSE;
                end
                RESPONSE: if (response_ready) phase <= IDLE;
                default: phase <= IDLE;
            endcase

            if (!fault) case (dma_phase)
                D_IDLE: begin
                    dma_index <= 0;
                    if (fill_found) begin
                        dma_line <= fill_line; dma_write_pool <= fill_write_pool; dma_write <= 0;
                        dma_speculative <= 0; dma_obsolete <= 0;
                        dma_chunk <= fill_write_pool ? 32'(write_tag[fill_line]) : 32'(read_tag[fill_line]);
                        dma_phase <= BURST_MEMORY != 0 ? D_TRANSFER : D_READ_REQUEST;
                    end else if (flush_found && flush_requested) begin
                        dma_line <= flush_line; dma_chunk <= 32'(write_tag[flush_line]);
                        dma_speculative <= 0; dma_obsolete <= 0;
                        dma_write_pool <= 1; dma_write <= 1;
                        dma_phase <= BURST_MEMORY != 0 ? D_TRANSFER : D_RAM_REQUEST;
                    end else if (hint_pending && phase != CHECK && phase != LOOKUP && phase != WAIT_LINE
                        && phase != FLUSH && phase != RETIRING && !(phase == IDLE && cmd_valid)) begin
                        if (hint_hit || (retire_busy && (hint_chunk >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT))) hint_pending <= 0;
                        else if (read_victim) begin
                            read_tag[read_victim_line] <= TAG_BITS'(hint_chunk);
                            read_present[read_victim_line] <= 1; read_loaded[read_victim_line] <= 0;
                            dma_line <= read_victim_line; dma_chunk <= hint_chunk;
                            dma_write_pool <= 0; dma_write <= 0;
                            dma_phase <= BURST_MEMORY != 0 ? D_TRANSFER : D_READ_REQUEST;
                            dma_speculative <= 1; dma_obsolete <= 0;
                            next_read <= READ_LINE_BITS'(read_victim_line + 1 == 32'(READ_BUFFERS) ? 0 : read_victim_line+1);
                            hint_pending <= 0;
                        end
                    end
                end
                D_READ_REQUEST: if (memory_valid && memory_ready) dma_phase <= D_READ_WAIT;
                D_READ_WAIT: if (dma_response) begin
                    if (dma_index == SPAN_BITS'(CHUNK_FRAMES-1)) begin
                        if (dma_write_pool) write_loaded[dma_line] <= 1;
                        else if (!dma_obsolete && !supersede_fill) read_loaded[dma_line] <= 1;
                        else begin read_present[dma_line] <= 0; read_loaded[dma_line] <= 0; end
                        dma_phase <= D_IDLE;
                    end else begin dma_index <= dma_index+1'b1; dma_phase <= D_READ_REQUEST; end
                end
                D_RAM_REQUEST: dma_phase <= D_RAM_RESULT;
                D_RAM_RESULT: begin
                    dma_data <= write_b_data;
                    dma_phase <= BURST_MEMORY != 0 ? D_STREAM_WRITE : D_WRITE_REQUEST;
                end
                D_WRITE_REQUEST: if (memory_valid && memory_ready) dma_phase <= D_WRITE_WAIT;
                D_WRITE_WAIT: if (dma_response) begin
                    if (dma_index == SPAN_BITS'(CHUNK_FRAMES-1)) begin dirty[dma_line] <= 0; dma_phase <= D_IDLE; end
                    else begin dma_index <= dma_index+1'b1; dma_phase <= D_RAM_REQUEST; end
                end
                D_TRANSFER: if (memory_valid && memory_ready)
                    dma_phase <= dma_write ? D_RAM_REQUEST : D_STREAM_READ;
                D_STREAM_READ: if (memory_read_valid && memory_read_ready) begin
                    if (dma_index == SPAN_BITS'(CHUNK_FRAMES-1)) dma_phase <= D_STREAM_WAIT;
                    else dma_index <= dma_index+1'b1;
                end
                D_STREAM_WRITE: if (memory_write_valid && memory_write_ready) begin
                    if (dma_index == SPAN_BITS'(CHUNK_FRAMES-1)) dma_phase <= D_STREAM_WAIT;
                    else begin dma_index <= dma_index+1'b1; dma_phase <= D_RAM_REQUEST; end
                end
                D_STREAM_WAIT: if (dma_response && !memory_response_error) begin
                    if (dma_write) dirty[dma_line] <= 0;
                    else if (dma_write_pool) write_loaded[dma_line] <= 1;
                    else if (!dma_obsolete && !supersede_fill) read_loaded[dma_line] <= 1;
                    else begin read_present[dma_line] <= 0; read_loaded[dma_line] <= 0; end
                    dma_phase <= D_IDLE;
                end
                default: dma_phase <= D_IDLE;
            endcase
            if (dma_response && memory_response_error) begin
                fault <= 1; dma_phase <= D_IDLE;
                if (retire_busy) begin retire_busy <= 0; retire_done <= 1; retire_error <= MEMORY_FAULT; end
                if (phase != IDLE && phase != RESPONSE) begin response_error <= MEMORY_FAULT; phase <= RESPONSE; end
            end
            if (retirement_complete && !fault && !(dma_response && memory_response_error)) begin
                for (integer r = 0; r < READ_BUFFERS; r=r+1)
                    if (read_present[r] && (32'(read_tag[r]) >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT)) begin read_present[r] <= 0; read_loaded[r] <= 0; end
                for (integer w = 0; w < WRITE_BUFFERS; w=w+1)
                    if (write_present[w] && (32'(write_tag[w]) >> RETIRE_SHIFT) == (background_chunk >> RETIRE_SHIFT)) begin write_present[w] <= 0; write_loaded[w] <= 0; end
                retire_busy <= 0; retire_done <= 1; retire_error <= 0;
            end
        end
    end
endmodule
