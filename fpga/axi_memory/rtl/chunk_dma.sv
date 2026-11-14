module chunk_dma #(
    parameter integer DATA_WIDTH = 32,
    parameter [31:0] MAX_WORDS = 32'hffffffff,
    parameter integer COUNT_BITS = MAX_WORDS < 1 ? 1 : $clog2(64'(MAX_WORDS)+1)
) (
    input wire clk, input wire reset,
    input wire [63:0] arena_base, input wire [63:0] arena_bytes,
    input wire [31:0] physical_base,
    input wire memory_valid, output wire memory_ready, input wire memory_write,
    input wire [63:0] memory_address, input wire [31:0] memory_write_data,
    input wire [31:0] memory_count,
    input wire memory_write_valid, output wire memory_write_ready,
    output wire memory_read_valid, input wire memory_read_ready,
    output wire memory_response_valid, input wire memory_response_ready,
    output reg memory_response_error, output wire [31:0] memory_read_data,
    output wire command_valid,
    input wire command_ready,
    output reg command_write,
    output wire [31:0] command_address,
    output reg [7:0] command_length,
    output reg [2:0] command_size,
    output wire write_valid,
    input wire write_ready,
    output wire [DATA_WIDTH-1:0] write_data,
    output wire [DATA_WIDTH/8-1:0] write_strobe,
    input wire read_valid,
    output wire read_ready,
    input wire [DATA_WIDTH-1:0] read_data,
    input wire read_error,
    input wire read_last,
    input wire completion_valid,
    output wire completion_ready,
    input wire completion_error
);
    localparam integer LANE_BITS = $clog2(DATA_WIDTH/8);
    typedef enum logic [3:0] {IDLE, CHECK_ADDRESS, MAP_ADDRESS, PREPARE, COMMAND, WRITE_COLLECT,
        WRITE_SEND, WRITE_ACK, READ_BEAT, READ_LOW, READ_HIGH,
        READ_DRAIN, NEXT_BURST, RESPONSE} phase_t;
    phase_t phase;
    reg half_word, last_beat;
    reg [31:0] address, burst_words;
    reg [COUNT_BITS-1:0] remaining;
    reg [7:0] beat_index;
    reg [63:0] beat_data;
    reg [63:0] offset, logical_address, logical_base, arena_capacity, arena_remaining;
    reg [64:0] byte_count, last_byte;
    reg [31:0] map_base;
    reg command_bad, arena_bad, mapped_bad, logical_bad, address_checked;
    wire [64:0] mapped = {33'b0,map_base}+{1'b0,offset};
    wire [64:0] mapped_end = {33'b0,address}+last_byte;
    wire [64:0] logical_end = {1'b0,logical_address}+last_byte;
    wire bad_address = command_bad || arena_bad || mapped_bad || logical_bad
        || byte_count > {1'b0,arena_remaining} || mapped_end[64:32] != 0;
    wire unused_bounds = &{1'b0,mapped_end[31:0],logical_end[63:0]};
    wire wide_beat = DATA_WIDTH == 64 && !address[2] && 32'(remaining) >= 2;
    wire [10:0] page_words = 11'd1024-{1'b0,address[11:2]};
    reg [5:0] planned_words;
    always @* begin
        planned_words = wide_beat ? 6'd32 : DATA_WIDTH == 64 ? 6'd1 : 6'd16;
        if (32'(remaining) < 32'(planned_words)) planned_words = 6'(remaining);
        if (page_words < 11'(planned_words)) planned_words = 6'(page_words);
        if (wide_beat) planned_words[0] = 0;
    end
    wire [LANE_BITS-1:0] lane = address[LANE_BITS-1:0];
    assign command_valid = phase == COMMAND;
    assign command_address = address;
    assign write_valid = phase == WRITE_SEND;
    assign write_data = DATA_WIDTH'(beat_data);
    assign write_strobe = command_size == 3
        ? {DATA_WIDTH/8{1'b1}} : (DATA_WIDTH/8)'(4'b1111) << lane;
    assign read_ready = phase == READ_BEAT || phase == READ_DRAIN;
    wire count_too_large;
    generate if (MAX_WORDS == 32'hffffffff) begin : unbounded_count
        assign count_too_large = 1'b0;
    end else begin : bounded_count
        assign count_too_large = memory_count > MAX_WORDS;
    end endgenerate
    assign completion_ready = phase == WRITE_ACK;
    assign memory_ready = phase == IDLE && !reset;
    assign memory_write_ready = phase == WRITE_COLLECT && !reset;
    assign memory_read_valid = (phase == READ_LOW || phase == READ_HIGH) && !reset;
    assign memory_read_data = phase == READ_HIGH ? beat_data[63:32] : 32'(beat_data >> (lane*8));
    assign memory_response_valid = phase == RESPONSE && !reset;

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; command_write <= 0; half_word <= 0; last_beat <= 0;
            address <= 0; remaining <= 0; burst_words <= 0;
            command_length <= 0; command_size <= 0; beat_index <= 0; beat_data <= 0;
            offset <= 0; logical_address <= 0; logical_base <= 0; arena_capacity <= 0; arena_remaining <= 0;
            byte_count <= 0; last_byte <= 0; map_base <= 0;
            command_bad <= 0; arena_bad <= 0; mapped_bad <= 0; logical_bad <= 0; address_checked <= 0;
            memory_response_error <= 0;
        end else begin
            case (phase)
                IDLE: if (memory_valid && memory_ready) begin
                    logical_address <= memory_address; logical_base <= arena_base;
                    arena_capacity <= arena_bytes; map_base <= physical_base;
                    byte_count <= {31'b0,memory_count,2'b0};
                    last_byte <= {31'b0,memory_count,2'b0}-1'b1;
                    command_bad <= memory_count == 0 || count_too_large
                        || memory_address[1:0] != 0;
                    remaining <= COUNT_BITS'(memory_count); command_write <= memory_write;
                    memory_response_error <= 0; address_checked <= 0; phase <= CHECK_ADDRESS;
                end
                CHECK_ADDRESS: begin
                    offset <= logical_address-logical_base;
                    command_bad <= command_bad || logical_address < logical_base;
                    phase <= MAP_ADDRESS;
                end
                MAP_ADDRESS: begin
                    address <= mapped[31:0]; arena_remaining <= arena_capacity-offset;
                    arena_bad <= offset > arena_capacity;
                    mapped_bad <= mapped[64:32] != 0 || mapped[1:0] != 0;
                    logical_bad <= logical_end[64];
                    phase <= PREPARE;
                end
                PREPARE: begin
                    address_checked <= 1;
                    if (!address_checked && bad_address) begin
                        memory_response_error <= 1; phase <= RESPONSE;
                    end else begin
                        burst_words <= 32'(planned_words);
                        command_size <= wide_beat ? 3'd3 : 3'd2;
                        command_length <= (wide_beat ? 8'(planned_words) >> 1 : 8'(planned_words))-1'b1;
                        beat_index <= 0; half_word <= 0; phase <= COMMAND;
                    end
                end
                COMMAND: if (command_valid && command_ready)
                    phase <= command_write ? WRITE_COLLECT : READ_BEAT;
                WRITE_COLLECT: if (memory_write_valid && memory_write_ready) begin
                    if (command_size == 3 && !half_word) begin
                        beat_data[31:0] <= memory_write_data; half_word <= 1;
                    end else begin
                        if (command_size == 3) beat_data[63:32] <= memory_write_data;
                        else beat_data <= {32'b0,memory_write_data} << (lane*8);
                        half_word <= 0; phase <= WRITE_SEND;
                    end
                end
                WRITE_SEND: if (write_valid && write_ready) begin
                    if (beat_index == command_length) phase <= WRITE_ACK;
                    else begin beat_index <= beat_index+1'b1; phase <= WRITE_COLLECT; end
                end
                WRITE_ACK: if (completion_valid && completion_ready) begin
                    if (completion_error) begin memory_response_error <= 1; phase <= RESPONSE; end
                    else phase <= NEXT_BURST;
                end
                READ_BEAT: if (read_valid && read_ready) begin
                    if (read_error) begin
                        memory_response_error <= 1;
                        phase <= read_last ? RESPONSE : READ_DRAIN;
                    end else begin
                        beat_data <= 64'(read_data); last_beat <= read_last; phase <= READ_LOW;
                    end
                end
                READ_LOW: if (memory_read_valid && memory_read_ready) begin
                    if (command_size == 3) phase <= READ_HIGH;
                    else phase <= last_beat ? NEXT_BURST : READ_BEAT;
                end
                READ_HIGH: if (memory_read_valid && memory_read_ready)
                    phase <= last_beat ? NEXT_BURST : READ_BEAT;
                READ_DRAIN: if (read_valid && read_ready && read_last) phase <= RESPONSE;
                NEXT_BURST: begin
                    if (32'(remaining) == burst_words) phase <= RESPONSE;
                    else begin
                        remaining <= COUNT_BITS'(32'(remaining)-burst_words);
                        address <= address+(burst_words << 2); phase <= PREPARE;
                    end
                end
                RESPONSE: if (memory_response_valid && memory_response_ready) phase <= IDLE;
                default: phase <= IDLE;
            endcase
        end
    end
endmodule
