module media_metadata_tb #(parameter integer DATA_WIDTH = 64, parameter integer LINE_WORDS = 16) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready, input wire [3:0] request_opcode,
    input wire [31:0] request_view, input wire [31:0] request_index, input wire [31:0] request_data,
    output wire response_valid, input wire response_ready,
    output wire [31:0] response_data, output wire [7:0] response_error, output wire fault,
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
    assign memory_valid = 0;
    wire memory_meta_valid;
    wire memory_ready;
    wire memory_meta_ready;
    wire memory_write;
    assign memory_write = 0;
    wire memory_meta_write;
    wire [63:0] memory_address;
    assign memory_address = 0;
    wire [63:0] memory_meta_address;
    wire [31:0] memory_write_data;
    assign memory_write_data = 0;
    wire [31:0] memory_meta_write_data;
    wire [31:0] memory_count;
    assign memory_count = 0;
    wire [31:0] memory_meta_count;
    wire memory_write_valid;
    assign memory_write_valid = 0;
    wire memory_meta_write_valid;
    wire memory_write_ready;
    wire memory_meta_write_ready;
    wire memory_read_valid;
    wire memory_meta_read_valid;
    wire memory_read_ready;
    assign memory_read_ready = 0;
    wire memory_meta_read_ready;
    wire memory_response_valid;
    wire memory_meta_response_valid;
    wire memory_response_ready;
    assign memory_response_ready = 0;
    wire memory_meta_response_ready;
    wire memory_response_error;
    wire memory_meta_response_error;
    wire [31:0] memory_read_data;
    wire [31:0] memory_meta_read_data;
    wire unused_sample = &{1'b0,memory_ready,memory_write_ready,memory_read_valid,memory_response_valid,memory_response_error,memory_read_data};
    media_metadata #(.VIEWS(4),.SLOTS(256),.CHUNK_FRAMES(1),.TABLE_PAGES(8),.TABLE_LINES(2),.REF_LINES(1),.STACK_LINES(1)) metadata (
        .clk(clk),
        .reset(reset),
        .arena_base(64'h01000000),
        .request_valid(request_valid),
        .request_ready(request_ready),
        .request_opcode(request_opcode),
        .request_view(request_view),
        .request_index(request_index),
        .request_data(request_data),
        .response_valid(response_valid),
        .response_ready(response_ready),
        .response_data(response_data),
        .response_error(response_error),
        .fault(fault),
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
    initial if (LINE_WORDS != 16) $fatal(1,"Metadata page width is sixteen words");
    shared_axi #(.DATA_WIDTH(DATA_WIDTH)) transport (
        .arena_base(64'h01000000),.arena_bytes(64'd32768),.sample_bytes(64'd16384),.physical_base(32'h01000000),.*);
    axi_direct_memory #(.DATA_WIDTH(DATA_WIDTH),.WORDS(8192)) ram (.*);
endmodule
