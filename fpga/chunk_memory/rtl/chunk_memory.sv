module chunk_memory #(
    parameter integer SLOTS = 32,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer READ_BUFFERS = 4,
    parameter integer WRITE_BUFFERS = 4
) (
    input wire clk, input wire reset,
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
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    localparam [2:0] READ = 0, WRITE = 1, PREFETCH = 2, FENCE = 3, RETIRE = 4;
    localparam [7:0] INVALID_CHUNK = 1, INVALID_SHAPE = 4, MEMORY_FAULT = 5;
    localparam integer RADDR_BITS = (READ_BUFFERS*CHUNK_FRAMES < 2) ? 1 : $clog2(READ_BUFFERS*CHUNK_FRAMES);
    localparam integer WADDR_BITS = (WRITE_BUFFERS*CHUNK_FRAMES < 2) ? 1 : $clog2(WRITE_BUFFERS*CHUNK_FRAMES);
    localparam [63:0] ARENA_BYTES = 64'(SLOTS)*64'(CHUNK_FRAMES)*4;
    typedef enum logic [3:0] { IDLE, CHECK, LOOKUP, WAIT_LINE, READ_REQUEST,
        READ_RESULT, READ_SEND, WRITE_DATA, COMMIT, FLUSH, RETIRING, RESPONSE } phase_t;
    typedef enum logic [2:0] { D_IDLE, D_READ_REQUEST, D_READ_WAIT,
        D_RAM_REQUEST, D_RAM_RESULT, D_WRITE_REQUEST, D_WRITE_WAIT } dma_phase_t;
    phase_t phase;
    dma_phase_t dma_phase;
    reg [2:0] opcode;
    reg [31:0] request_id, chunk, offset, count, index, line;
    reg use_write_pool;
    reg [63:0] requested_base, active_base;
    reg base_bound;
    reg [31:0] read_tag [0:READ_BUFFERS-1];
    reg [READ_BUFFERS-1:0] read_present, read_loaded;
    reg [31:0] write_tag [0:WRITE_BUFFERS-1];
    reg [WRITE_BUFFERS-1:0] write_present, write_loaded, dirty;
    reg [31:0] next_read, next_write, quiet;
    reg hint_pending;
    reg [31:0] hint_chunk;
    reg dma_write, dma_write_pool;
    reg dma_speculative, dma_obsolete;
    reg [31:0] dma_line, dma_chunk, dma_index, dma_data;
    reg read_hit, write_hit, read_victim, write_victim, flush_found, fill_found;
    reg [31:0] read_hit_line, write_hit_line, read_victim_line, write_victim_line, flush_line, fill_line;
    reg fill_write_pool, hint_hit, incoming_write_space;
    reg [31:0] dirty_count;
    integer i, candidate;
    wire dma_active = dma_phase != D_IDLE;
    wire read_pinned = !use_write_pool && (phase == WAIT_LINE || phase == READ_REQUEST || phase == READ_RESULT || phase == READ_SEND);
    wire write_claimed = opcode == WRITE && phase != IDLE && phase != RESPONSE;
    wire incoming_write = cmd_valid && cmd_opcode == WRITE && cmd_count != 0 && cmd_chunk < 32'(SLOTS)
        && cmd_offset <= 32'(CHUNK_FRAMES) && cmd_count <= 32'(CHUNK_FRAMES)-cmd_offset;
    wire need_space = (phase == LOOKUP && opcode == WRITE && !write_hit && !write_victim)
        || (phase == IDLE && incoming_write && !incoming_write_space);
    wire dma_response = memory_response_valid && memory_response_ready;
    wire supersede_fill = dma_active && dma_speculative && phase == LOOKUP && opcode == WRITE && dma_chunk == chunk;
    wire fill_word = dma_phase == D_READ_WAIT && dma_response && !memory_response_error
        && !dma_obsolete && !supersede_fill;
    wire [31:0] read_a_data, read_b_data, write_a_data, write_b_data;
    wire unused_read_b = &{1'b0,read_b_data};
    wire write_word = phase == WRITE_DATA && write_valid && write_ready;
    assign cmd_ready = phase == IDLE && !fault && !reset && (!incoming_write || incoming_write_space);
    assign write_ready = phase == WRITE_DATA && !fault && !reset;
    assign read_valid = phase == READ_SEND && !fault && !reset;
    assign read_last = index + 1 == count;
    assign response_valid = phase == RESPONSE && !reset;
    assign response_id = request_id;
    assign prefetch_ready = !hint_pending && !fault && !reset && phase != FLUSH && phase != RETIRING
        && (base_bound || (phase == IDLE && !cmd_valid))
        && !(cmd_valid && (cmd_opcode == PREFETCH || cmd_opcode == FENCE || cmd_opcode == RETIRE));
    assign busy = phase != IDLE || dma_active || hint_pending || (dirty_count != 0);
    assign memory_valid = !fault && !reset && (dma_phase == D_READ_REQUEST || dma_phase == D_WRITE_REQUEST);
    assign memory_write = dma_write;
    assign memory_address = active_base + (64'(dma_chunk)*64'(CHUNK_FRAMES) + {32'b0,dma_index})*4;
    assign memory_write_data = dma_data;
    assign memory_arena_base = active_base;
    assign memory_response_ready = !fault && !reset && (dma_phase == D_READ_WAIT || dma_phase == D_WRITE_WAIT);

    initial begin
        if (SLOTS < 1 || SLOTS > 65536 || CHUNK_FRAMES < 1 || CHUNK_FRAMES > 65536
            || (CHUNK_FRAMES & (CHUNK_FRAMES-1)) != 0 || READ_BUFFERS < 1 || WRITE_BUFFERS < 1
            || READ_BUFFERS > 65536 || WRITE_BUFFERS > 65536)
            $fatal(1,"Invalid chunk-memory capacities");
    end

    always @* begin
        read_hit = 0; write_hit = 0; read_hit_line = 0; write_hit_line = 0;
        read_victim = 0; write_victim = 0; read_victim_line = 0; write_victim_line = 0;
        flush_found = 0; flush_line = 0; fill_found = 0; fill_line = 0; fill_write_pool = 0;
        hint_hit = 0; dirty_count = 0; candidate = 0; incoming_write_space = 0;
        for (i = 0; i < READ_BUFFERS; i = i+1) begin
            if (read_present[i] && read_tag[i] == chunk) begin read_hit = 1; read_hit_line = 32'(i); end
            if (read_present[i] && read_tag[i] == hint_chunk) hint_hit = 1;
            if (!read_present[i] && !read_victim
                && !(dma_active && !dma_write_pool && dma_line == 32'(i))) begin read_victim = 1; read_victim_line = 32'(i); end
            if (read_present[i] && !read_loaded[i]) begin fill_found = 1; fill_line = 32'(i); end
        end
        for (i = 0; i < READ_BUFFERS; i = i+1) begin
            candidate = 32'(next_read) + i;
            if (candidate >= READ_BUFFERS) candidate = candidate - READ_BUFFERS;
            if (!read_victim && !(read_pinned && line == 32'(candidate))
                && !(dma_active && !dma_write_pool && dma_line == 32'(candidate))) begin
                read_victim = 1; read_victim_line = 32'(candidate);
            end
        end
        for (i = 0; i < WRITE_BUFFERS; i = i+1) begin
            if ((write_present[i] && write_tag[i] == cmd_chunk) || !write_present[i]
                || (write_loaded[i] && !dirty[i] && !(dma_active && dma_write_pool && dma_line == 32'(i))))
                incoming_write_space = 1;
            if (write_present[i] && write_tag[i] == chunk) begin write_hit = 1; write_hit_line = 32'(i); end
            if (write_present[i] && write_tag[i] == hint_chunk) hint_hit = 1;
            if (!write_present[i] && !write_victim) begin write_victim = 1; write_victim_line = 32'(i); end
            if (dirty[i]) dirty_count = dirty_count + 1;
            if (write_present[i] && !write_loaded[i]) begin fill_found = 1; fill_line = 32'(i); fill_write_pool = 1; end
        end
        for (i = 0; i < WRITE_BUFFERS; i = i+1) begin
            candidate = 32'(next_write) + i;
            if (candidate >= WRITE_BUFFERS) candidate = candidate - WRITE_BUFFERS;
            if (!write_victim && write_loaded[candidate] && !dirty[candidate]
                && !(dma_active && dma_write_pool && dma_line == 32'(candidate))) begin
                write_victim = 1; write_victim_line = 32'(candidate);
            end
            if (!flush_found && dirty[candidate] && write_loaded[candidate]
                && !(write_claimed && write_tag[candidate] == chunk)) begin
                flush_found = 1; flush_line = 32'(candidate);
            end
        end
    end

    chunk_ram #(.DEPTH(READ_BUFFERS*CHUNK_FRAMES), .ADDR_BITS(RADDR_BITS)) read_ram (
        .clk(clk), .a_enable(!reset && phase == READ_REQUEST && !use_write_pool), .a_write(1'b0),
        .a_address(RADDR_BITS'(line*32'(CHUNK_FRAMES)+offset+index)), .a_write_data(32'b0), .a_read_data(read_a_data),
        .b_enable(!reset && fill_word && !dma_write_pool), .b_write(1'b1),
        .b_address(RADDR_BITS'(dma_line*32'(CHUNK_FRAMES)+dma_index)), .b_write_data(memory_read_data), .b_read_data(read_b_data)
    );
    chunk_ram #(.DEPTH(WRITE_BUFFERS*CHUNK_FRAMES), .ADDR_BITS(WADDR_BITS)) write_ram (
        .clk(clk), .a_enable(!reset && (write_word || (phase == READ_REQUEST && use_write_pool) || (fill_word && dma_write_pool))),
        .a_write(write_word || (fill_word && dma_write_pool)),
        .a_address(WADDR_BITS'((fill_word && dma_write_pool) ? dma_line*32'(CHUNK_FRAMES)+dma_index : line*32'(CHUNK_FRAMES)+offset+index)),
        .a_write_data(fill_word && dma_write_pool ? memory_read_data : write_data), .a_read_data(write_a_data),
        .b_enable(!reset && dma_phase == D_RAM_REQUEST), .b_write(1'b0),
        .b_address(WADDR_BITS'(dma_line*32'(CHUNK_FRAMES)+dma_index)), .b_write_data(32'b0), .b_read_data(write_b_data)
    );

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; dma_phase <= D_IDLE; fault <= 0;
            opcode <= READ; request_id <= 0; chunk <= 0; offset <= 0; count <= 0; index <= 0; line <= 0;
            use_write_pool <= 0; requested_base <= 0; active_base <= 0; base_bound <= 0;
            read_present <= 0; read_loaded <= 0; write_present <= 0; write_loaded <= 0; dirty <= 0;
            next_read <= 0; next_write <= 0; quiet <= 0; hint_pending <= 0; hint_chunk <= 0;
            dma_write <= 0; dma_write_pool <= 0; dma_line <= 0; dma_chunk <= 0; dma_index <= 0; dma_data <= 0;
            dma_speculative <= 0; dma_obsolete <= 0;
            response_error <= 0; read_data <= 0;
            for (integer r = 0; r < READ_BUFFERS; r = r+1) read_tag[r] <= 0;
            for (integer w = 0; w < WRITE_BUFFERS; w = w+1) write_tag[w] <= 0;
        end else begin
            if (quiet < 32'(CHUNK_FRAMES+8)) quiet <= quiet + 1;
            if (prefetch_valid && prefetch_ready && prefetch_chunk < 32'(SLOTS)
                && arena_base[1:0] == 0 && arena_base <= 64'hffffffffffffffff-ARENA_BYTES
                && (!base_bound || arena_base == active_base)) begin
                hint_pending <= 1; hint_chunk <= prefetch_chunk;
                if (!base_bound) begin active_base <= arena_base; base_bound <= 1; end
            end
            if (supersede_fill) dma_obsolete <= 1;
            case (phase)
                IDLE: if (cmd_valid && cmd_ready) begin
                    opcode <= cmd_opcode; request_id <= cmd_id; chunk <= cmd_chunk;
                    offset <= cmd_offset; count <= cmd_count; index <= 0; requested_base <= arena_base;
                    response_error <= 0; phase <= CHECK;
                    if (cmd_opcode == WRITE) quiet <= 0;
                end
                CHECK: begin
                    if (opcode > RETIRE || requested_base[1:0] != 0
                        || requested_base > 64'hffffffffffffffff-ARENA_BYTES
                        || (base_bound && requested_base != active_base)) begin
                        response_error <= INVALID_SHAPE; phase <= RESPONSE;
                    end else if (opcode != FENCE && chunk >= 32'(SLOTS)) begin
                        response_error <= INVALID_CHUNK; phase <= RESPONSE;
                    end else if ((opcode == READ || opcode == WRITE)
                        && (offset > 32'(CHUNK_FRAMES) || count > 32'(CHUNK_FRAMES)-offset)) begin
                        response_error <= INVALID_SHAPE; phase <= RESPONSE;
                    end else begin
                        active_base <= requested_base; base_bound <= 1;
                        if (opcode == FENCE) begin hint_pending <= 0; phase <= FLUSH; end
                        else if (opcode == RETIRE) begin
                            if (hint_pending && hint_chunk == chunk) hint_pending <= 0;
                            phase <= RETIRING;
                        end else if (opcode == PREFETCH) begin
                            if (!hint_pending) begin hint_pending <= 1; hint_chunk <= chunk; end
                            phase <= RESPONSE;
                        end else begin
                            if (opcode == WRITE && count != 0 && hint_pending && hint_chunk == chunk) hint_pending <= 0;
                            phase <= count == 0 ? RESPONSE : LOOKUP;
                        end
                    end
                end
                LOOKUP: begin
                    if (opcode == WRITE && dma_active && dma_chunk == chunk && !dma_speculative) begin end
                    else if (write_hit) begin
                        line <= write_hit_line; use_write_pool <= 1;
                        phase <= write_loaded[write_hit_line] ? ((opcode == WRITE) ? WRITE_DATA : READ_REQUEST) : WAIT_LINE;
                        if (opcode == WRITE) for (integer r = 0; r < READ_BUFFERS; r = r+1)
                            if (read_present[r] && read_tag[r] == chunk) begin read_present[r] <= 0; read_loaded[r] <= 0; end
                    end else if (opcode == READ && read_hit) begin
                        line <= read_hit_line; use_write_pool <= 0;
                        phase <= read_loaded[read_hit_line] ? READ_REQUEST : WAIT_LINE;
                    end else if (opcode == READ && read_victim) begin
                        line <= read_victim_line; use_write_pool <= 0;
                        read_present[read_victim_line] <= 1; read_loaded[read_victim_line] <= 0;
                        read_tag[read_victim_line] <= chunk;
                        next_read <= read_victim_line + 1 == 32'(READ_BUFFERS) ? 0 : read_victim_line+1;
                        phase <= WAIT_LINE;
                    end else if (opcode == WRITE && write_victim) begin
                        line <= write_victim_line; use_write_pool <= 1;
                        write_present[write_victim_line] <= 1; write_tag[write_victim_line] <= chunk;
                        dirty[write_victim_line] <= 0;
                        write_loaded[write_victim_line] <= count == 32'(CHUNK_FRAMES);
                        next_write <= write_victim_line + 1 == 32'(WRITE_BUFFERS) ? 0 : write_victim_line+1;
                        for (integer r = 0; r < READ_BUFFERS; r = r+1)
                            if (read_present[r] && read_tag[r] == chunk) begin read_present[r] <= 0; read_loaded[r] <= 0; end
                        phase <= count == 32'(CHUNK_FRAMES) ? WRITE_DATA : WAIT_LINE;
                    end
                end
                WAIT_LINE: if (use_write_pool ? write_loaded[line] : read_loaded[line])
                    phase <= opcode == WRITE ? WRITE_DATA : READ_REQUEST;
                READ_REQUEST: phase <= READ_RESULT;
                READ_RESULT: begin read_data <= use_write_pool ? write_a_data : read_a_data; phase <= READ_SEND; end
                READ_SEND: if (read_valid && read_ready) begin
                    index <= index + 1;
                    phase <= read_last ? RESPONSE : READ_REQUEST;
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
                RETIRING: if (!(dma_active && dma_chunk == chunk)
                    && !(write_hit && dirty[write_hit_line])) begin
                    for (integer r = 0; r < READ_BUFFERS; r = r+1)
                        if (read_present[r] && read_tag[r] == chunk) begin read_present[r] <= 0; read_loaded[r] <= 0; end
                    for (integer w = 0; w < WRITE_BUFFERS; w = w+1)
                        if (write_present[w] && write_tag[w] == chunk) begin write_present[w] <= 0; write_loaded[w] <= 0; end
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
                        dma_chunk <= fill_write_pool ? write_tag[fill_line] : read_tag[fill_line];
                        dma_phase <= D_READ_REQUEST;
                    end else if (flush_found && (quiet >= 32'(CHUNK_FRAMES+8) || need_space || phase == FLUSH || phase == RETIRING)) begin
                        dma_line <= flush_line; dma_chunk <= write_tag[flush_line];
                        dma_speculative <= 0; dma_obsolete <= 0;
                        dma_write_pool <= 1; dma_write <= 1; dma_phase <= D_RAM_REQUEST;
                    end else if (hint_pending && phase != CHECK && phase != LOOKUP && phase != WAIT_LINE
                        && phase != FLUSH && phase != RETIRING && !(cmd_valid && cmd_ready)) begin
                        if (hint_hit) hint_pending <= 0;
                        else if (read_victim) begin
                            read_tag[read_victim_line] <= hint_chunk;
                            read_present[read_victim_line] <= 1; read_loaded[read_victim_line] <= 0;
                            dma_line <= read_victim_line; dma_chunk <= hint_chunk;
                            dma_write_pool <= 0; dma_write <= 0; dma_phase <= D_READ_REQUEST;
                            dma_speculative <= 1; dma_obsolete <= 0;
                            next_read <= read_victim_line + 1 == 32'(READ_BUFFERS) ? 0 : read_victim_line+1;
                            hint_pending <= 0;
                        end
                    end
                end
                D_READ_REQUEST: if (memory_valid && memory_ready) dma_phase <= D_READ_WAIT;
                D_READ_WAIT: if (dma_response) begin
                    if (dma_index+1 == 32'(CHUNK_FRAMES)) begin
                        if (dma_write_pool) write_loaded[dma_line] <= 1;
                        else if (!dma_obsolete && !supersede_fill) read_loaded[dma_line] <= 1;
                        else begin read_present[dma_line] <= 0; read_loaded[dma_line] <= 0; end
                        dma_phase <= D_IDLE;
                    end else begin dma_index <= dma_index+1; dma_phase <= D_READ_REQUEST; end
                end
                D_RAM_REQUEST: dma_phase <= D_RAM_RESULT;
                D_RAM_RESULT: begin dma_data <= write_b_data; dma_phase <= D_WRITE_REQUEST; end
                D_WRITE_REQUEST: if (memory_valid && memory_ready) dma_phase <= D_WRITE_WAIT;
                D_WRITE_WAIT: if (dma_response) begin
                    if (dma_index+1 == 32'(CHUNK_FRAMES)) begin dirty[dma_line] <= 0; dma_phase <= D_IDLE; end
                    else begin dma_index <= dma_index+1; dma_phase <= D_RAM_REQUEST; end
                end
                default: dma_phase <= D_IDLE;
            endcase
            if (dma_response && memory_response_error) begin
                fault <= 1; dma_phase <= D_IDLE;
                if (phase != IDLE && phase != RESPONSE) begin response_error <= MEMORY_FAULT; phase <= RESPONSE; end
            end
        end
    end
endmodule
