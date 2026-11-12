// Generated functional simulation top; do not edit by hand.
// Generated integration wrapper; do not edit by hand.
`include "state_processor_defs.svh"

module chunk_memory_axi #(
parameter integer SLOTS = 32,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer READ_BUFFERS = 4,
    parameter integer WRITE_BUFFERS = 4,
    parameter integer BURST_MEMORY = 1,
    parameter integer RETIRE_CHUNKS = 1,
    parameter integer AXI_DATA_WIDTH = 32,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000
) (
    input wire clk,
    input wire reset,
    input wire [63:0] arena_base,
    input wire cmd_valid,
    output wire cmd_ready,
    input wire [2:0] cmd_opcode,
    input wire [31:0] cmd_id,
    input wire [31:0] cmd_chunk,
    input wire [31:0] cmd_offset,
    input wire [31:0] cmd_count,
    input wire write_valid,
    output wire write_ready,
    input wire [31:0] write_data,
    output wire read_valid,
    input wire read_ready,
    output wire [31:0] read_data,
    output wire read_last,
    output wire response_valid,
    input wire response_ready,
    output wire [31:0] response_id,
    output wire [7:0] response_error,
    input wire prefetch_valid,
    output wire prefetch_ready,
    input wire [31:0] prefetch_chunk,
    output wire busy,
    output wire fault,
    output wire m_axi_awid,
    output wire [31:0] m_axi_awaddr,
    output wire [3:0] m_axi_awlen,
    output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst,
    output wire m_axi_awvalid,
    output wire m_axi_awready,
    output wire [AXI_DATA_WIDTH-1:0] m_axi_wdata,
    output wire [AXI_DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wid,
    output wire m_axi_wlast,
    output wire m_axi_wvalid,
    output wire m_axi_wready,
    output wire m_axi_bid,
    output wire [1:0] m_axi_bresp,
    output wire m_axi_bvalid,
    output wire m_axi_bready,
    output wire m_axi_arid,
    output wire [31:0] m_axi_araddr,
    output wire [3:0] m_axi_arlen,
    output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst,
    output wire m_axi_arvalid,
    output wire m_axi_arready,
    output wire m_axi_rid,
    output wire [AXI_DATA_WIDTH-1:0] m_axi_rdata,
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
    wire memory_valid;
    wire memory_ready;
    wire memory_write;
    wire [63:0] memory_address;
    wire [31:0] memory_write_data;
    wire [63:0] memory_arena_base;
    wire [31:0] memory_count;
    wire memory_write_valid;
    wire memory_write_ready;
    wire memory_read_valid;
    wire memory_read_ready;
    wire memory_response_valid;
    wire memory_response_ready;
    wire memory_response_error;
    wire [31:0] memory_read_data;
    wire [63:0] arena_bytes = 64'(SLOTS)*64'(CHUNK_FRAMES)*4;
    wire [31:0] physical_base = PHYSICAL_BASE;
    chunk_memory #(
        .SLOTS(SLOTS),
        .CHUNK_FRAMES(CHUNK_FRAMES),
        .READ_BUFFERS(READ_BUFFERS),
        .WRITE_BUFFERS(WRITE_BUFFERS),
        .BURST_MEMORY(BURST_MEMORY),
        .RETIRE_CHUNKS(RETIRE_CHUNKS)) core (.*);
    generate if (BURST_MEMORY != 0) begin : bursts
        chunk_axi #(.DATA_WIDTH(AXI_DATA_WIDTH)) memory_endpoint (.arena_base(memory_arena_base),.*);
    end else begin : words
        wire unused_stream = &{1'b0,memory_count,memory_write_valid,memory_read_ready};
        assign memory_write_ready = 0;
        assign memory_read_valid = 0;
        word_axi #(.DATA_WIDTH(AXI_DATA_WIDTH)) memory_endpoint (.arena_base(memory_arena_base),.*);
    end endgenerate
    axi_direct_memory #(.DATA_WIDTH(AXI_DATA_WIDTH),.WORDS(SLOTS*CHUNK_FRAMES),.BASE(PHYSICAL_BASE)) sim_memory (.*);
endmodule
