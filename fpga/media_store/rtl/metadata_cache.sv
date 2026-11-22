module metadata_cache #(
    parameter integer LINES = 8,
    parameter integer LINE_WORDS = 16,
    parameter integer BURST_MEMORY = 1,
    parameter integer ADDRESS_BITS = (LINES*LINE_WORDS < 2) ? 1 : $clog2(LINES*LINE_WORDS)
) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready,
    input wire [1:0] request_opcode, input wire request_invalidate,
    input wire [63:0] request_address, input wire [31:0] request_data,
    output wire response_valid, input wire response_ready,
    output reg [31:0] response_data, output reg response_error, output reg fault,
    output reg [63:0] hits, output reg [63:0] misses, output reg [63:0] fills, output reg [63:0] writebacks,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    output wire [31:0] memory_count, output wire memory_write_valid, input wire memory_write_ready,
    input wire memory_read_valid, output wire memory_read_ready,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    localparam integer SHIFT = $clog2(LINE_WORDS*4);
    localparam integer WORD_BITS = $clog2(LINE_WORDS);
    localparam integer LINE_BITS = LINES < 2 ? 1 : $clog2(LINES);
    localparam integer SCAN_BITS = $clog2(LINES+1);
    typedef enum logic [3:0] {IDLE, MATCH, LOOKUP, ACCESS, RESULT, RESPONSE, ZERO,
        FILL_REQUEST, FILL_DATA, FILL_ACK, WRITE_REQUEST, WRITE_READ, WRITE_DATA, WRITE_ACK, SCAN} phase_t;
    phase_t phase;
    reg [LINES-1:0] valid, dirty;
    reg [LINES-1:0] matched_lines;
    reg [63:0] tags [0:LINES-1];
    reg [63:0] address;
    reg [31:0] payload;
    reg [LINE_BITS-1:0] target, victim;
    reg [SCAN_BITS-1:0] scan;
    reg [WORD_BITS-1:0] index;
    reg [1:0] opcode;
    reg invalidate, flushing;
    reg hit;
    reg [LINE_BITS-1:0] hit_line;
    wire [63:0] tag = (address >> SHIFT) << SHIFT;
    wire [31:0] word_index = 32'((address-tag) >> 2);
    wire read_ram = phase == WRITE_READ || (phase == ACCESS && opcode == 0);
    wire [31:0] ram_read;
    wire ram_write = phase == ZERO || (phase == ACCESS && opcode == 1)
        || (phase == FILL_DATA && memory_read_valid && memory_read_ready)
        || (BURST_MEMORY == 0 && phase == FILL_ACK && memory_response_valid && !memory_response_error);
    wire [31:0] ram_index = phase == ACCESS ? word_index : 32'(index);
    wire [31:0] ram_data = phase == ZERO ? 32'b0 : phase == ACCESS ? payload : memory_read_data;
    wire unused_bits = &{1'b0,address[1:0]};
    assign request_ready = phase == IDLE && !reset && !fault;
    assign response_valid = phase == RESPONSE && !reset;
    assign memory_valid = !reset && (phase == FILL_REQUEST || phase == WRITE_REQUEST
        || (BURST_MEMORY == 0 && phase == WRITE_DATA));
    assign memory_write = phase == WRITE_REQUEST || phase == WRITE_DATA;
    assign memory_address = tags[target] + (BURST_MEMORY != 0 ? 64'b0 : 64'(index)*4);
    assign memory_write_data = ram_read;
    assign memory_count = BURST_MEMORY != 0 ? 32'(LINE_WORDS) : 32'd1;
    assign memory_write_valid = BURST_MEMORY != 0 && phase == WRITE_DATA && !reset;
    assign memory_read_ready = BURST_MEMORY != 0 && phase == FILL_DATA && !reset;
    assign memory_response_ready = phase == FILL_ACK || phase == WRITE_ACK || phase == FILL_DATA
        || phase == WRITE_REQUEST || phase == WRITE_READ || phase == WRITE_DATA;
    media_ram #(.DEPTH(LINES*LINE_WORDS),.ADDR_BITS(ADDRESS_BITS)) data_ram (
        .clk(clk),.read_enable(read_ram),
        .read_address(ADDRESS_BITS'(32'(target)*32'(LINE_WORDS)+(phase == ACCESS ? word_index : 32'(index)))),
        .read_data(ram_read),.write_enable(ram_write),
        .write_address(ADDRESS_BITS'(32'(target)*32'(LINE_WORDS)+ram_index)),.write_data(ram_data)
    );
    always @* begin
        hit = 0; hit_line = 0;
        for (integer i=0; i<LINES; i=i+1)
            if (matched_lines[i]) begin hit = 1; hit_line = LINE_BITS'(i); end
    end
    initial if (LINES < 1 || LINE_WORDS < 2 || (LINE_WORDS & (LINE_WORDS-1)) != 0)
        $fatal(1,"Invalid metadata cache dimensions");
    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; valid <= 0; dirty <= 0; matched_lines <= 0; address <= 0; payload <= 0;
            target <= 0; victim <= 0; index <= 0; scan <= 0; opcode <= 0;
            invalidate <= 0; flushing <= 0; response_data <= 0; response_error <= 0; fault <= 0;
            hits <= 0; misses <= 0; fills <= 0; writebacks <= 0;
        end else begin
            case (phase)
                IDLE: if (request_valid && request_ready) begin
                    opcode <= request_opcode; address <= request_address; payload <= request_data;
                    invalidate <= request_invalidate; response_data <= 0; response_error <= 0;
                    flushing <= request_opcode == 3; scan <= 0;
                    phase <= request_opcode == 3 ? SCAN : MATCH;
                end
                MATCH: begin
                    for (integer i=0; i<LINES; i=i+1) matched_lines[i] <= valid[i] && tags[i] == tag;
                    phase <= LOOKUP;
                end
                LOOKUP: begin
                    index <= 0;
                    if (hit) begin
                        hits <= hits+1; target <= hit_line; phase <= opcode == 2 ? ZERO : ACCESS;
                    end else begin
                        misses <= misses+1; target <= victim;
                        victim <= victim == LINE_BITS'(LINES-1) ? 0 : victim+1'b1;
                        if (valid[victim] && dirty[victim]) phase <= BURST_MEMORY != 0 ? WRITE_REQUEST : WRITE_READ;
                        else begin
                            tags[victim] <= tag; valid[victim] <= 0;
                            phase <= opcode == 2 ? ZERO : FILL_REQUEST;
                        end
                    end
                end
                ACCESS: begin
                    if (opcode == 1) dirty[target] <= 1;
                    phase <= opcode == 0 ? RESULT : RESPONSE;
                end
                RESULT: begin response_data <= ram_read; phase <= RESPONSE; end
                RESPONSE: if (response_ready) phase <= IDLE;
                ZERO: begin
                    if (index == WORD_BITS'(LINE_WORDS-1)) begin
                        tags[target] <= tag; valid[target] <= 1; dirty[target] <= 1; phase <= RESPONSE;
                    end else index <= index+1'b1;
                end
                FILL_REQUEST: if (memory_ready) phase <= BURST_MEMORY != 0 ? FILL_DATA : FILL_ACK;
                FILL_DATA: begin
                    if (memory_read_valid && memory_read_ready) begin
                        if (index == WORD_BITS'(LINE_WORDS-1)) phase <= FILL_ACK;
                        else index <= index+1'b1;
                    end
                    if (memory_response_valid && memory_response_error) begin
                        response_error <= 1; fault <= 1; phase <= RESPONSE;
                    end
                end
                FILL_ACK: if (memory_response_valid) begin
                    if (memory_response_error) begin response_error <= 1; fault <= 1; phase <= RESPONSE; end
                    else if (BURST_MEMORY != 0 || index == WORD_BITS'(LINE_WORDS-1)) begin
                        fills <= fills+1; valid[target] <= 1; dirty[target] <= 0; phase <= ACCESS;
                    end else begin index <= index+1'b1; phase <= FILL_REQUEST; end
                end
                WRITE_REQUEST: if (memory_ready) phase <= WRITE_READ;
                WRITE_READ: phase <= WRITE_DATA;
                WRITE_DATA: if (BURST_MEMORY != 0 ? memory_write_ready : memory_ready) begin
                    if (BURST_MEMORY == 0 || index == WORD_BITS'(LINE_WORDS-1)) phase <= WRITE_ACK;
                    else begin index <= index+1'b1; phase <= WRITE_READ; end
                end
                WRITE_ACK: if (memory_response_valid) begin
                    if (memory_response_error) begin response_error <= 1; fault <= 1; phase <= RESPONSE; end
                    else if (BURST_MEMORY == 0 && index != WORD_BITS'(LINE_WORDS-1)) begin index <= index+1'b1; phase <= WRITE_READ; end
                    else begin
                        writebacks <= writebacks+1; dirty[target] <= 0; index <= 0;
                        if (flushing) begin valid[target] <= !invalidate; scan <= scan+1'b1; phase <= SCAN; end
                        else begin tags[target] <= tag; valid[target] <= 0; phase <= opcode == 2 ? ZERO : FILL_REQUEST; end
                    end
                end
                SCAN: begin
                    if (scan == SCAN_BITS'(LINES)) phase <= RESPONSE;
                    else if (valid[LINE_BITS'(scan)] && dirty[LINE_BITS'(scan)]) begin
                        target <= LINE_BITS'(scan); index <= 0; phase <= BURST_MEMORY != 0 ? WRITE_REQUEST : WRITE_READ;
                    end else begin if (invalidate) valid[LINE_BITS'(scan)] <= 0; scan <= scan+1'b1; end
                end
                default: begin fault <= 1; response_error <= 1; phase <= RESPONSE; end
            endcase
            if ((phase == WRITE_REQUEST || phase == WRITE_READ || phase == WRITE_DATA)
                && memory_response_valid && memory_response_error) begin
                response_error <= 1; fault <= 1; phase <= RESPONSE;
            end
        end
    end
endmodule
