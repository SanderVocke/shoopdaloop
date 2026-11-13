// Generated integration testbench; do not edit by hand.
`timescale 1ns/1ps
`include "state_processor_defs.svh"

module loop_pipeline_axi_tb #(
parameter integer EXTERNAL_IO = 0,
    parameter integer LOOP_DEPTH = 8,
    parameter integer REQUEST_DEPTH = 16,
    parameter integer EVENT_DEPTH = 32,
    parameter integer PROGRAM_DEPTH = 64,
    parameter integer STATE_DEPTH = 32,
    parameter integer JOB_DEPTH = 64,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer VIEWS = 32,
    parameter integer SLOTS = 1024,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer MEMORY_RESPONSE_CYCLES = 1,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer RETIRED_TABLES = 256,
    parameter integer RECLAIM_BLOCKS = 0, parameter integer RECLAIM_CYCLES = 5000,
    parameter integer RECLAIM_GUARD = 1024,
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
    output reg clk,
    input wire reset,
    input wire maintenance_enable,
    output wire [31:0] reclaim_pending,
    output wire [31:0] reclaim_high_water,
    output wire [63:0] reclaim_references,
    output wire reclaim_active,
    output wire reclaim_released,
    output wire [31:0] reclaim_block,
    output wire [63:0] reclaim_blocks,
    output wire [63:0] reclaim_freed,
    output wire [63:0] reclaim_cycles,
    output wire reclaim_quiescent,
    output wire [31:0] reclaim_pages_used,
    output wire [31:0] reclaim_pages_available,
    output wire [63:0] reclaim_pages_freed,
    output wire [63:0] reclaim_capacity_failures,
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
    output wire faulted,
    output reg done,
    output reg [7:0] error_code,
    output reg [1:0] error_stage,
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
    input wire command_valid,
    output wire command_ready,
    input wire [7:0] command_opcode,
    input wire [31:0] command_index,
    input wire [639:0] command_data,
    output wire response_valid,
    input wire response_ready,
    output wire [3:0] response_error,
    output wire [831:0] response_data,
    output wire transition_busy,
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
    output wire [63:0] debug_frame,
    output wire debug_shared_read,
    output wire m_axi_awid,
    output wire [31:0] m_axi_awaddr,
    output wire [3:0] m_axi_awlen,
    output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst,
    output wire m_axi_awvalid,
    input wire m_axi_awready,
    output wire [AXI_DATA_WIDTH-1:0] m_axi_wdata,
    output wire [AXI_DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wid,
    output wire m_axi_wlast,
    output wire m_axi_wvalid,
    input wire m_axi_wready,
    input wire m_axi_bid,
    input wire [1:0] m_axi_bresp,
    input wire m_axi_bvalid,
    output wire m_axi_bready,
    output wire m_axi_arid,
    output wire [31:0] m_axi_araddr,
    output wire [3:0] m_axi_arlen,
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
    wire [31:0] memory_meta_read_data = core.memory_meta_read_data;
    wire memory_meta_response_error = core.memory_meta_response_error;
    wire memory_meta_response_ready = core.memory_meta_response_ready;
    wire memory_meta_response_valid = core.memory_meta_response_valid;
    wire memory_meta_read_ready = core.memory_meta_read_ready;
    wire memory_meta_read_valid = core.memory_meta_read_valid;
    wire memory_meta_write_ready = core.memory_meta_write_ready;
    wire memory_meta_write_valid = core.memory_meta_write_valid;
    wire [31:0] memory_meta_count = core.memory_meta_count;
    wire [31:0] memory_meta_write_data = core.memory_meta_write_data;
    wire [63:0] memory_meta_address = core.memory_meta_address;
    wire memory_meta_write = core.memory_meta_write;
    wire memory_meta_ready = core.memory_meta_ready;
    wire memory_meta_valid = core.memory_meta_valid;
    wire [31:0] memory_read_data = core.memory_read_data;
    wire memory_response_error = core.memory_response_error;
    wire memory_response_ready = core.memory_response_ready;
    wire memory_response_valid = core.memory_response_valid;
    wire memory_read_ready = core.memory_read_ready;
    wire memory_read_valid = core.memory_read_valid;
    wire memory_write_ready = core.memory_write_ready;
    wire memory_write_valid = core.memory_write_valid;
    wire [31:0] memory_count = core.memory_count;
    wire [63:0] memory_arena_base = core.memory_arena_base;
    wire [31:0] memory_write_data = core.memory_write_data;
    wire [63:0] memory_address = core.memory_address;
    wire memory_write = core.memory_write;
    wire memory_ready = core.memory_ready;
    wire memory_valid = core.memory_valid;
    initial clk=0;
    always #5 clk<=~clk;
    loop_pipeline_axi #(.EXTERNAL_IO(EXTERNAL_IO),.LOOP_DEPTH(LOOP_DEPTH),.REQUEST_DEPTH(REQUEST_DEPTH),.EVENT_DEPTH(EVENT_DEPTH),
        .PROGRAM_DEPTH(PROGRAM_DEPTH),.STATE_DEPTH(STATE_DEPTH),.JOB_DEPTH(JOB_DEPTH),
        .MAX_FRAMES(MAX_FRAMES),.BUFFERS(BUFFERS),.INPUT_PORTS(INPUT_PORTS),.OUTPUT_PORTS(OUTPUT_PORTS),
        .VIEWS(VIEWS),.SLOTS(SLOTS),.CHUNK_FRAMES(CHUNK_FRAMES),
        .MEMORY_RESPONSE_CYCLES(MEMORY_RESPONSE_CYCLES),.READ_BUFFERS(READ_BUFFERS),.WRITE_BUFFERS(WRITE_BUFFERS), .BURST_MEMORY(BURST_MEMORY), .BLOCK_CHUNKS(BLOCK_CHUNKS),.RETIRED_TABLES(RETIRED_TABLES),.RECLAIM_BLOCKS(RECLAIM_BLOCKS),.RECLAIM_CYCLES(RECLAIM_CYCLES),.RECLAIM_GUARD(RECLAIM_GUARD),.TABLE_PAGES(TABLE_PAGES), .TABLE_LINES(TABLE_LINES), .REF_LINES(REF_LINES), .STACK_LINES(STACK_LINES),
        .AXI_DATA_WIDTH(AXI_DATA_WIDTH),.PHYSICAL_BASE(PHYSICAL_BASE)) core (.*);
    assign debug_frame=core.core.timing.frame;
    assign debug_shared_read=core.core.loop_read_valid && core.core.loop_read_ready;
endmodule
