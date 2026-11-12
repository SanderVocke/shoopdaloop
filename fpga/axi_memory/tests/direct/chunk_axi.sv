// Generated functional simulation top; do not edit by hand.
module chunk_axi #(
parameter integer DATA_WIDTH = 32,
    parameter integer SIM_WORDS = 8192,
    parameter [31:0] SIM_PHYSICAL_BASE = 32'h01000000
) (
    input wire clk,
    input wire reset,
    input wire [63:0] arena_base,
    input wire [63:0] arena_bytes,
    input wire [31:0] physical_base,
    input wire memory_valid,
    output wire memory_ready,
    input wire memory_write,
    input wire [63:0] memory_address,
    input wire [31:0] memory_write_data,
    input wire [31:0] memory_count,
    input wire memory_write_valid,
    output wire memory_write_ready,
    output wire memory_read_valid,
    input wire memory_read_ready,
    output wire memory_response_valid,
    input wire memory_response_ready,
    output reg memory_response_error,
    output wire [31:0] memory_read_data,
    output wire m_axi_awid,
    output wire [31:0] m_axi_awaddr,
    output wire [7:0] m_axi_awlen,
    output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst,
    output wire m_axi_awvalid,
    output wire m_axi_awready,
    output wire [DATA_WIDTH-1:0] m_axi_wdata,
    output wire [DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wlast,
    output wire m_axi_wvalid,
    output wire m_axi_wready,
    output wire m_axi_bid,
    output wire [1:0] m_axi_bresp,
    output wire m_axi_bvalid,
    output wire m_axi_bready,
    output wire m_axi_arid,
    output wire [31:0] m_axi_araddr,
    output wire [7:0] m_axi_arlen,
    output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst,
    output wire m_axi_arvalid,
    output wire m_axi_arready,
    output wire m_axi_rid,
    output wire [DATA_WIDTH-1:0] m_axi_rdata,
    output wire [1:0] m_axi_rresp,
    output wire m_axi_rlast,
    output wire m_axi_rvalid,
    output wire m_axi_rready,
    input wire [1:0] sim_image_request,
    output wire sim_image_done,
    input wire [4:0] sim_control,
    input wire [31:0] sim_fail_token,
    output wire [31:0] sim_fail_seen,
    output wire sim_busy, output wire sim_active, output wire sim_error, output wire sim_write,
    output wire [31:0] sim_index,
    output wire [63:0] sim_reads, output wire [63:0] sim_writes,
    output wire [31:0] sim_capacity
);
    localparam integer LANE_BITS = $clog2(DATA_WIDTH/8);
    typedef enum logic [3:0] {IDLE, PREPARE, COMMAND, WRITE_COLLECT,
        WRITE_SEND, WRITE_ACK, READ_BEAT, READ_LOW, READ_HIGH,
        READ_DRAIN, NEXT_BURST, RESPONSE} phase_t;
    phase_t phase;
    reg command_write, half_word, last_beat;
    reg [31:0] address, remaining, burst_words;
    reg [7:0] command_length, beat_index;
    reg [2:0] command_size;
    reg [63:0] beat_data;
    wire [63:0] offset = memory_address-arena_base;
    wire [64:0] byte_count = {31'b0,memory_count,2'b0};
    wire [64:0] mapped = {33'b0,physical_base}+{1'b0,offset};
    wire [64:0] mapped_end = mapped+byte_count-1'b1;
    wire [64:0] logical_end = {1'b0,memory_address}+byte_count-1'b1;
    wire bad_address = memory_count == 0 || memory_address < arena_base
        || offset > arena_bytes || byte_count > {1'b0,arena_bytes-offset}
        || mapped[64:32] != 0 || mapped_end[64:32] != 0 || logical_end[64]
        || mapped[1:0] != 0 || memory_address[1:0] != 0;
    wire unused_bounds = &{1'b0,mapped_end[31:0],logical_end[63:0]};
    wire wide_beat = DATA_WIDTH == 64 && !address[2] && remaining >= 2;
    wire [31:0] page_words = 32'd1024-{22'b0,address[11:2]};
    reg [31:0] planned_words;
    always @* begin
        planned_words = wide_beat ? 32'd32 : DATA_WIDTH == 64 ? 32'd1 : 32'd16;
        if (remaining < planned_words) planned_words = remaining;
        if (page_words < planned_words) planned_words = page_words;
        if (wide_beat) planned_words[0] = 0;
    end
    wire [LANE_BITS-1:0] lane = address[LANE_BITS-1:0];
    wire command_valid = phase == COMMAND;
    wire command_ready;
    wire [31:0] command_address = address;
    wire write_valid = phase == WRITE_SEND;
    wire write_ready;
    wire [DATA_WIDTH-1:0] write_data = DATA_WIDTH'(beat_data);
    wire [DATA_WIDTH/8-1:0] write_strobe = command_size == 3
        ? {DATA_WIDTH/8{1'b1}} : (DATA_WIDTH/8)'(4'b1111) << lane;
    wire read_valid, read_error, read_last;
    wire read_ready = phase == READ_BEAT || phase == READ_DRAIN;
    wire [DATA_WIDTH-1:0] read_data;
    wire completion_valid, completion_error;
    wire completion_ready = phase == WRITE_ACK;
    assign memory_ready = phase == IDLE && !reset;
    assign memory_write_ready = phase == WRITE_COLLECT && !reset;
    assign memory_read_valid = (phase == READ_LOW || phase == READ_HIGH) && !reset;
    assign memory_read_data = phase == READ_HIGH ? beat_data[63:32] : 32'(beat_data >> (lane*8));
    assign memory_response_valid = phase == RESPONSE && !reset;

    axi_master #(.DATA_WIDTH(DATA_WIDTH)) endpoint (.*);

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; command_write <= 0; half_word <= 0; last_beat <= 0;
            address <= 0; remaining <= 0; burst_words <= 0;
            command_length <= 0; command_size <= 0; beat_index <= 0; beat_data <= 0;
            memory_response_error <= 0;
        end else begin
            case (phase)
                IDLE: if (memory_valid && memory_ready) begin
                    address <= mapped[31:0]; remaining <= memory_count; command_write <= memory_write;
                    memory_response_error <= bad_address; phase <= bad_address ? RESPONSE : PREPARE;
                end
                PREPARE: begin
                    burst_words <= planned_words;
                    command_size <= wide_beat ? 3'd3 : 3'd2;
                    command_length <= 8'((wide_beat ? planned_words >> 1 : planned_words)-1'b1);
                    beat_index <= 0; half_word <= 0; phase <= COMMAND;
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
                    if (remaining == burst_words) phase <= RESPONSE;
                    else begin
                        remaining <= remaining-burst_words;
                        address <= address+(burst_words << 2); phase <= PREPARE;
                    end
                end
                RESPONSE: if (memory_response_valid && memory_response_ready) phase <= IDLE;
                default: phase <= IDLE;
            endcase
        end
    end
    axi_direct_memory #(.DATA_WIDTH(DATA_WIDTH),.WORDS(SIM_WORDS),.BASE(SIM_PHYSICAL_BASE)) sim_memory (.*);
endmodule
