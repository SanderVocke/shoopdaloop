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
    wire command_valid;
    wire command_ready;
    wire command_write;
    wire [31:0] command_address;
    wire [7:0] command_length;
    wire [2:0] command_size;
    wire write_valid;
    wire write_ready;
    wire [DATA_WIDTH-1:0] write_data;
    wire [DATA_WIDTH/8-1:0] write_strobe;
    wire read_valid;
    wire read_ready;
    wire [DATA_WIDTH-1:0] read_data;
    wire read_error;
    wire read_last;
    wire completion_valid;
    wire completion_ready;
    wire completion_error;
    chunk_dma #(.DATA_WIDTH(DATA_WIDTH)) transfer (.*);
    axi_master #(.DATA_WIDTH(DATA_WIDTH)) endpoint (.*);
    axi_direct_memory #(.DATA_WIDTH(DATA_WIDTH),.WORDS(SIM_WORDS),.BASE(SIM_PHYSICAL_BASE)) sim_memory (.*);
endmodule
