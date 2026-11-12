module metadata_cache_tb #(parameter integer DATA_WIDTH = 64, parameter integer LINE_WORDS = 16) (
    input wire clk, input wire reset,
    input wire c0_request_valid, output wire c0_request_ready,
    input wire [1:0] c0_request_opcode, input wire c0_request_invalidate,
    input wire [63:0] c0_request_address, input wire [31:0] c0_request_data,
    output wire c0_response_valid, input wire c0_response_ready,
    output wire [31:0] c0_response_data, output wire c0_response_error, output wire c0_fault,
    output wire [63:0] c0_hits, output wire [63:0] c0_misses, output wire [63:0] c0_fills, output wire [63:0] c0_writebacks,
    input wire c1_request_valid, output wire c1_request_ready,
    input wire [1:0] c1_request_opcode, input wire c1_request_invalidate,
    input wire [63:0] c1_request_address, input wire [31:0] c1_request_data,
    output wire c1_response_valid, input wire c1_response_ready,
    output wire [31:0] c1_response_data, output wire c1_response_error, output wire c1_fault,
    output wire [63:0] c1_hits, output wire [63:0] c1_misses, output wire [63:0] c1_fills, output wire [63:0] c1_writebacks,
    input wire [1:0] sim_image_request, output wire sim_image_done, input wire [4:0] sim_control,
    input wire [31:0] sim_fail_token, output wire [31:0] sim_fail_seen,
    output wire sim_busy, output wire sim_active, output wire sim_error, output wire sim_write,
    output wire [31:0] sim_index, output wire [63:0] sim_reads, output wire [63:0] sim_writes, output wire [31:0] sim_capacity,
    output wire m_axi_awid, output wire [31:0] m_axi_awaddr,
    output wire [7:0] m_axi_awlen, output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst, output wire m_axi_awvalid, output wire m_axi_awready,
    output wire [DATA_WIDTH-1:0] m_axi_wdata, output wire [DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wlast, output wire m_axi_wvalid, output wire m_axi_wready,
    output wire m_axi_bid, output wire [1:0] m_axi_bresp,
    output wire m_axi_bvalid, output wire m_axi_bready,
    output wire m_axi_arid, output wire [31:0] m_axi_araddr,
    output wire [7:0] m_axi_arlen, output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst, output wire m_axi_arvalid, output wire m_axi_arready,
    output wire m_axi_rid, output wire [DATA_WIDTH-1:0] m_axi_rdata,
    output wire [1:0] m_axi_rresp, output wire m_axi_rlast,
    output wire m_axi_rvalid, output wire m_axi_rready


);
    wire memory_valid;
    wire memory_ready;
    wire memory_write;
    wire [63:0] memory_address;
    wire [31:0] memory_write_data;
    wire [31:0] memory_count;
    wire memory_write_valid;
    wire memory_write_ready;
    wire memory_read_valid;
    wire memory_read_ready;
    wire memory_response_valid;
    wire memory_response_ready;
    wire memory_response_error;
    wire [31:0] memory_read_data;
    metadata_cache #(.LINES(2),.LINE_WORDS(LINE_WORDS)) cache_0 (
        .clk(clk),
        .reset(reset),
        .request_valid(c0_request_valid),
        .request_ready(c0_request_ready),
        .request_opcode(c0_request_opcode),
        .request_invalidate(c0_request_invalidate),
        .request_address(c0_request_address),
        .request_data(c0_request_data),
        .response_valid(c0_response_valid),
        .response_ready(c0_response_ready),
        .response_data(c0_response_data),
        .response_error(c0_response_error),
        .fault(c0_fault),
        .hits(c0_hits),
        .misses(c0_misses),
        .fills(c0_fills),
        .writebacks(c0_writebacks),
        .memory_valid(memory_valid),
        .memory_ready(memory_ready),
        .memory_write(memory_write),
        .memory_address(memory_address),
        .memory_write_data(memory_write_data),
        .memory_count(memory_count),
        .memory_write_valid(memory_write_valid),
        .memory_write_ready(memory_write_ready),
        .memory_read_valid(memory_read_valid),
        .memory_read_ready(memory_read_ready),
        .memory_response_valid(memory_response_valid),
        .memory_response_ready(memory_response_ready),
        .memory_response_error(memory_response_error),
        .memory_read_data(memory_read_data));
    wire memory_meta_valid;
    wire memory_meta_ready;
    wire memory_meta_write;
    wire [63:0] memory_meta_address;
    wire [31:0] memory_meta_write_data;
    wire [31:0] memory_meta_count;
    wire memory_meta_write_valid;
    wire memory_meta_write_ready;
    wire memory_meta_read_valid;
    wire memory_meta_read_ready;
    wire memory_meta_response_valid;
    wire memory_meta_response_ready;
    wire memory_meta_response_error;
    wire [31:0] memory_meta_read_data;
    metadata_cache #(.LINES(2),.LINE_WORDS(LINE_WORDS)) cache_1 (
        .clk(clk),
        .reset(reset),
        .request_valid(c1_request_valid),
        .request_ready(c1_request_ready),
        .request_opcode(c1_request_opcode),
        .request_invalidate(c1_request_invalidate),
        .request_address(c1_request_address),
        .request_data(c1_request_data),
        .response_valid(c1_response_valid),
        .response_ready(c1_response_ready),
        .response_data(c1_response_data),
        .response_error(c1_response_error),
        .fault(c1_fault),
        .hits(c1_hits),
        .misses(c1_misses),
        .fills(c1_fills),
        .writebacks(c1_writebacks),
        .memory_valid(memory_meta_valid),
        .memory_ready(memory_meta_ready),
        .memory_write(memory_meta_write),
        .memory_address(memory_meta_address),
        .memory_write_data(memory_meta_write_data),
        .memory_count(memory_meta_count),
        .memory_write_valid(memory_meta_write_valid),
        .memory_write_ready(memory_meta_write_ready),
        .memory_read_valid(memory_meta_read_valid),
        .memory_read_ready(memory_meta_read_ready),
        .memory_response_valid(memory_meta_response_valid),
        .memory_response_ready(memory_meta_response_ready),
        .memory_response_error(memory_meta_response_error),
        .memory_read_data(memory_meta_read_data));
    shared_axi #(.DATA_WIDTH(DATA_WIDTH)) transport (
        .arena_base(64'h01000000),.arena_bytes(64'd32768),.sample_bytes(64'd16384),.physical_base(32'h01000000),.*);
    axi_direct_memory #(.DATA_WIDTH(DATA_WIDTH),.WORDS(8192)) ram (.*);
endmodule
