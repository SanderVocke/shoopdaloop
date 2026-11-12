// Generated functional simulation top; do not edit by hand.
// Generated integration wrapper; do not edit by hand.
`include "state_processor_defs.svh"

module media_store_axi #(
parameter integer REUSE_VIEWS = 0,
    parameter integer VIEWS = 32,
    parameter integer SLOTS = 1024,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer TABLE_PAGES = (SLOTS/BLOCK_CHUNKS+14)/15 + 8*VIEWS + 16,
    parameter integer TABLE_LINES = 8, parameter integer REF_LINES = 4, parameter integer STACK_LINES = 2,
    parameter integer READ_BUFFERS = 4,
    parameter integer WRITE_BUFFERS = 4, parameter integer BURST_MEMORY = 1,
    parameter integer VIEW_BITS = (VIEWS < 2) ? 1 : $clog2(VIEWS),
    parameter integer SLOT_BITS = (SLOTS/BLOCK_CHUNKS < 2) ? 1 : $clog2(SLOTS/BLOCK_CHUNKS),
    parameter integer TABLE_BITS = VIEW_BITS + SLOT_BITS,
    parameter integer AXI_DATA_WIDTH = 32,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000
) (
    input wire clk,
    input wire reset,
    input wire [63:0] arena_base,
    input wire hint_valid,
    output wire hint_ready,
    input wire [63:0] hint_view,
    input wire [63:0] hint_position,
    input wire [63:0] hint_length,
    input wire [31:0] hint_count,
    input wire cmd_valid,
    output wire cmd_ready,
    input wire [3:0] cmd_opcode,
    input wire [63:0] cmd_view,
    input wire [63:0] cmd_start,
    input wire [63:0] cmd_length,
    input wire [31:0] cmd_count,
    output wire busy,
    output wire done,
    output wire [7:0] error_code,
    output wire memory_fault,
    output wire [31:0] result_view,
    output wire result_empty,
    input wire write_valid,
    output wire write_ready,
    input wire [31:0] write_data,
    output wire read_valid,
    input wire read_ready,
    output wire [31:0] read_data,
    input wire cfg_valid,
    output wire cfg_ready,
    input wire cfg_write,
    input wire [2:0] cfg_space,
    input wire [31:0] cfg_address,
    input wire [159:0] cfg_write_data,
    output wire cfg_response_valid,
    output wire cfg_response_error,
    output wire [159:0] cfg_read_data,
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
    wire [63:0] arena_bytes = 64'(SLOTS)*64'(CHUNK_FRAMES)*4;
    wire [31:0] physical_base = PHYSICAL_BASE;
    media_store #(
        .REUSE_VIEWS(REUSE_VIEWS),
        .VIEWS(VIEWS),
        .SLOTS(SLOTS),
        .CHUNK_FRAMES(CHUNK_FRAMES),
        .BLOCK_CHUNKS(BLOCK_CHUNKS),
        .TABLE_PAGES(TABLE_PAGES),
        .TABLE_LINES(TABLE_LINES),
        .REF_LINES(REF_LINES),
        .STACK_LINES(STACK_LINES),
        .READ_BUFFERS(READ_BUFFERS),
        .WRITE_BUFFERS(WRITE_BUFFERS),
        .BURST_MEMORY(BURST_MEMORY),
        .VIEW_BITS(VIEW_BITS),
        .SLOT_BITS(SLOT_BITS),
        .TABLE_BITS(TABLE_BITS)) core (.*);
    localparam [63:0] STORAGE_WORDS = ((64'(SLOTS)*64'(CHUNK_FRAMES)+15)/16)*16 + 2*((64'(SLOTS/BLOCK_CHUNKS)+15)/16)*16 + 64'(TABLE_PAGES)*16;
    initial if ({32'b0,PHYSICAL_BASE}+STORAGE_WORDS*4 > 64'h100000000) $fatal(1,"DDR arena exceeds AXI address range");
    shared_axi #(.DATA_WIDTH(AXI_DATA_WIDTH),.BURST_MEMORY(BURST_MEMORY),.MAX_SAMPLE_WORDS(32'(CHUNK_FRAMES)),.MAX_METADATA_WORDS(32'd16)) memory_endpoint (.arena_base(memory_arena_base),.arena_bytes(STORAGE_WORDS*4),.sample_bytes(arena_bytes),.*);
    axi_direct_memory #(.DATA_WIDTH(AXI_DATA_WIDTH),.WORDS(32'(STORAGE_WORDS)),.BASE(PHYSICAL_BASE)) sim_memory (.*);
endmodule
