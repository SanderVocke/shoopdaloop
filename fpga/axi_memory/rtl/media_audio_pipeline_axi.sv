// Generated integration wrapper; do not edit by hand.
`include "state_processor_defs.svh"

module media_audio_pipeline_axi #(
    parameter integer EXTERNAL_IO = 0,
    parameter integer PROGRAM_DEPTH = 32, parameter integer STATE_DEPTH = 16,
    parameter integer JOB_DEPTH = 32, parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16, parameter integer INPUT_PORTS = 2, parameter integer OUTPUT_PORTS = 2,
    parameter integer VIEWS = 32, parameter integer SLOTS = 1024, parameter integer CHUNK_FRAMES = 32,
    parameter integer MEMORY_RESPONSE_CYCLES = 1,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer TABLE_PAGES = (SLOTS/BLOCK_CHUNKS+14)/15 + 8*VIEWS + 16,
    parameter integer TABLE_LINES = 8, parameter integer REF_LINES = 4, parameter integer STACK_LINES = 2,
    parameter integer READ_BUFFERS = 4, parameter integer WRITE_BUFFERS = 4, parameter integer BURST_MEMORY = 1,
    parameter integer AXI_DATA_WIDTH = 32,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000
) (
    output wire sample_input_read,
    output wire [31:0] sample_input_address,
    input wire [31:0] sample_input_data,
    output wire sample_output_write,
    output wire [31:0] sample_output_address,
    output wire [31:0] sample_output_data,
    input wire clk,
    input wire reset,
    input wire run_valid,
    output wire run_ready,
    input wire [31:0] run_instruction_count,
    input wire [31:0] run_state_count,
    input wire [31:0] run_quantum,
    input wire [31:0] run_chunk_frames,
    input wire [31:0] run_job_budget,
    input wire [31:0] run_buffers,
    input wire [31:0] run_inputs,
    input wire [31:0] run_outputs,
    input wire job_enable,
    output wire busy,
    output wire done,
    output wire [7:0] error_code,
    output wire [1:0] error_stage,
    output wire state_busy,
    output wire state_done,
    output wire jobs_busy,
    output wire jobs_done,
    output wire [BUFFERS-1:0] buffer_ready,
    input wire cfg_valid,
    output wire cfg_ready,
    input wire cfg_state,
    input wire cfg_write,
    input wire [15:0] cfg_address,
    input wire [`SHOOP_SP_STATE_BITS-1:0] cfg_write_data,
    output wire cfg_response_valid,
    output wire cfg_response_error,
    output wire [`SHOOP_SP_STATE_BITS-1:0] cfg_read_data,
    input wire audio_cfg_valid,
    output wire audio_cfg_ready,
    input wire [1:0] audio_cfg_space,
    input wire audio_cfg_write,
    input wire [31:0] audio_cfg_address,
    input wire [31:0] audio_cfg_write_data,
    output wire audio_cfg_response_valid,
    output wire audio_cfg_response_error,
    output wire [31:0] audio_cfg_read_data,
    output wire monitor_valid,
    output wire monitor_ready,
    output wire [`SHOOP_SP_JOB_BITS-1:0] monitor_data,
    output wire [31:0] queued_jobs,
    output wire drain_valid,
    input wire drain_ready,
    output wire loop_read_valid,
    input wire loop_read_ready,
    output wire [31:0] loop_read_slot,
    input wire loop_response_valid,
    input wire loop_response_error,
    input wire [163:0] loop_response_data,
    input wire [63:0] arena_base,
    input wire store_cmd_valid,
    output wire store_cmd_ready,
    input wire [3:0] store_cmd_opcode,
    input wire [63:0] store_cmd_view,
    input wire [63:0] store_cmd_start,
    input wire [63:0] store_cmd_length,
    input wire [31:0] store_cmd_count,
    output wire store_busy,
    output wire store_done,
    output wire [7:0] store_error_code,
    output wire [31:0] store_result_view,
    output wire store_result_empty,
    input wire store_write_valid,
    output wire store_write_ready,
    input wire [31:0] store_write_data,
    output wire store_read_valid,
    input wire store_read_ready,
    output wire [31:0] store_read_data,
    input wire store_cfg_valid,
    output wire store_cfg_ready,
    input wire store_cfg_write,
    input wire [2:0] store_cfg_space,
    input wire [31:0] store_cfg_address,
    input wire [159:0] store_cfg_write_data,
    output wire store_cfg_response_valid,
    output wire store_cfg_response_error,
    output wire [159:0] store_cfg_read_data,
    output wire memory_fault,
    output wire m_axi_awid,
    output wire [31:0] m_axi_awaddr,
    output wire [7:0] m_axi_awlen,
    output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst,
    output wire m_axi_awvalid,
    input wire m_axi_awready,
    output wire [AXI_DATA_WIDTH-1:0] m_axi_wdata,
    output wire [AXI_DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wlast,
    output wire m_axi_wvalid,
    input wire m_axi_wready,
    input wire m_axi_bid,
    input wire [1:0] m_axi_bresp,
    input wire m_axi_bvalid,
    output wire m_axi_bready,
    output wire m_axi_arid,
    output wire [31:0] m_axi_araddr,
    output wire [7:0] m_axi_arlen,
    output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst,
    output wire m_axi_arvalid,
    input wire m_axi_arready,
    input wire m_axi_rid,
    input wire [AXI_DATA_WIDTH-1:0] m_axi_rdata,
    input wire [1:0] m_axi_rresp,
    input wire m_axi_rlast,
    input wire m_axi_rvalid,
    output wire m_axi_rready
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
    media_audio_pipeline #(
        .EXTERNAL_IO(EXTERNAL_IO),
        .PROGRAM_DEPTH(PROGRAM_DEPTH),
        .STATE_DEPTH(STATE_DEPTH),
        .JOB_DEPTH(JOB_DEPTH),
        .MAX_FRAMES(MAX_FRAMES),
        .BUFFERS(BUFFERS),
        .INPUT_PORTS(INPUT_PORTS),
        .OUTPUT_PORTS(OUTPUT_PORTS),
        .VIEWS(VIEWS),
        .SLOTS(SLOTS),
        .CHUNK_FRAMES(CHUNK_FRAMES),
        .MEMORY_RESPONSE_CYCLES(MEMORY_RESPONSE_CYCLES),
        .BLOCK_CHUNKS(BLOCK_CHUNKS),
        .TABLE_PAGES(TABLE_PAGES),
        .TABLE_LINES(TABLE_LINES),
        .REF_LINES(REF_LINES),
        .STACK_LINES(STACK_LINES),
        .READ_BUFFERS(READ_BUFFERS),
        .WRITE_BUFFERS(WRITE_BUFFERS),
        .BURST_MEMORY(BURST_MEMORY)) core (.*);
    localparam [63:0] STORAGE_WORDS = ((64'(SLOTS)*64'(CHUNK_FRAMES)+15)/16)*16 + 2*((64'(SLOTS/BLOCK_CHUNKS)+15)/16)*16 + 64'(TABLE_PAGES)*16;
    initial if ({32'b0,PHYSICAL_BASE}+STORAGE_WORDS*4 > 64'h100000000) $fatal(1,"DDR arena exceeds AXI address range");
    shared_axi #(.DATA_WIDTH(AXI_DATA_WIDTH),.BURST_MEMORY(BURST_MEMORY),.MAX_SAMPLE_WORDS(32'(CHUNK_FRAMES))) memory_endpoint (.arena_base(memory_arena_base),.arena_bytes(STORAGE_WORDS*4),.sample_bytes(arena_bytes),.*);
endmodule
