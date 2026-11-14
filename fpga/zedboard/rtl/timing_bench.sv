// Generated benchmark interface; do not edit by hand.
`include "state_processor_defs.svh"
module timing_bench #(
    parameter integer LOOP_DEPTH = 16,
    parameter integer REQUEST_DEPTH = 128,
    parameter integer EVENT_DEPTH = 256,
    parameter integer PROGRAM_DEPTH = 64,
    parameter integer STATE_DEPTH = 32,
    parameter integer JOB_DEPTH = 80,
    parameter integer EXTERNAL_IO = 1,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 32,
    parameter integer INPUT_PORTS = 16,
    parameter integer OUTPUT_PORTS = 16,
    parameter integer REUSE_VIEWS = 0,
    parameter integer CAPTURE_CHANNELS = 0,
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
    parameter integer AXI_DATA_WIDTH = 64,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000,
    parameter integer INSTRUCTION_COUNT = 64, STATE_COUNT = 32, BUFFER_COUNT = 32, JOB_BUDGET = 80,
    parameter integer BOOT_WORDS = 166, EVENT_WORDS = 74,
    parameter BOOT_FILE = "fpga/audio_io/examples/default/boot.hex",
    parameter EVENT_FILE = "fpga/audio_io/examples/default/events.hex"
) (
    input wire clk, input wire reset, input wire start, input wire stop, input wire snapshot,
    input wire [31:0] quanta_limit, input wire [31:0] warmup_quanta,
    input wire [31:0] gap_cycles, input wire [31:0] watchdog_cycles,
    output wire [31:0] status, output reg [1023:0] statistics,
    output wire run_pulse, output wire done_pulse,
    output wire [63:0] debug_quantum, output wire [31:0] debug_cycles,
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
    wire sample_input_read;
    wire [31:0] sample_input_address;
    wire [31:0] sample_input_data;
    wire sample_output_write;
    wire [31:0] sample_output_address;
    wire [31:0] sample_output_data;
    wire core_reset;
    wire maintenance_enable;
    wire cut_snapshot_valid;
    wire [1727:0] cut_snapshot_data;
    wire [31:0] reclaim_pending;
    wire [31:0] reclaim_high_water;
    wire [63:0] reclaim_references;
    wire reclaim_active;
    wire reclaim_released;
    wire [31:0] reclaim_block;
    wire [63:0] reclaim_blocks;
    wire [63:0] reclaim_freed;
    wire [63:0] reclaim_cycles;
    wire reclaim_quiescent;
    wire [31:0] reclaim_pages_used;
    wire [31:0] reclaim_pages_available;
    wire [63:0] reclaim_pages_freed;
    wire [63:0] reclaim_capacity_failures;
    wire run_valid;
    wire run_ready;
    wire [31:0] run_instruction_count;
    wire [31:0] run_state_count;
    wire [31:0] run_quantum;
    wire [31:0] run_chunk_frames;
    wire [31:0] run_job_budget;
    wire [31:0] run_buffers;
    wire [31:0] run_inputs;
    wire [31:0] run_outputs;
    wire job_enable;
    wire busy;
    wire faulted;
    wire done;
    wire [7:0] error_code;
    wire [1:0] error_stage;
    wire state_busy;
    wire state_done;
    wire jobs_busy;
    wire jobs_done;
    wire [BUFFERS-1:0] buffer_ready;
    wire cfg_valid;
    wire cfg_ready;
    wire cfg_state;
    wire cfg_write;
    wire [15:0] cfg_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] cfg_write_data;
    wire cfg_response_valid;
    wire cfg_response_error;
    wire [`SHOOP_SP_STATE_BITS-1:0] cfg_read_data;
    wire audio_cfg_valid;
    wire audio_cfg_ready;
    wire [1:0] audio_cfg_space;
    wire audio_cfg_write;
    wire [31:0] audio_cfg_address;
    wire [31:0] audio_cfg_write_data;
    wire audio_cfg_response_valid;
    wire audio_cfg_response_error;
    wire [31:0] audio_cfg_read_data;
    wire monitor_valid;
    wire monitor_ready;
    wire [`SHOOP_SP_JOB_BITS-1:0] monitor_data;
    wire [31:0] queued_jobs;
    wire drain_valid;
    wire drain_ready;
    wire command_valid;
    wire command_ready;
    wire [7:0] command_opcode;
    wire [31:0] command_index;
    wire [639:0] command_data;
    wire response_valid;
    wire response_ready;
    wire [3:0] response_error;
    wire [831:0] response_data;
    wire transition_busy;
    wire [63:0] arena_base;
    wire store_cmd_valid;
    wire store_cmd_ready;
    wire [3:0] store_cmd_opcode;
    wire [63:0] store_cmd_view;
    wire [63:0] store_cmd_start;
    wire [63:0] store_cmd_length;
    wire [31:0] store_cmd_count;
    wire store_busy;
    wire store_done;
    wire [7:0] store_error_code;
    wire [31:0] store_result_view;
    wire store_result_empty;
    wire store_write_valid;
    wire store_write_ready;
    wire [31:0] store_write_data;
    wire store_read_valid;
    wire store_read_ready;
    wire [31:0] store_read_data;
    wire store_cfg_valid;
    wire store_cfg_ready;
    wire store_cfg_write;
    wire [2:0] store_cfg_space;
    wire [31:0] store_cfg_address;
    wire [159:0] store_cfg_write_data;
    wire store_cfg_response_valid;
    wire store_cfg_response_error;
    wire [159:0] store_cfg_read_data;
    wire memory_fault;
    wire core_m_axi_awid;
    wire [31:0] core_m_axi_awaddr;
    wire [3:0] core_m_axi_awlen;
    wire [2:0] core_m_axi_awsize;
    wire [1:0] core_m_axi_awburst;
    wire core_m_axi_awvalid;
    wire core_m_axi_awready;
    wire [AXI_DATA_WIDTH-1:0] core_m_axi_wdata;
    wire [AXI_DATA_WIDTH/8-1:0] core_m_axi_wstrb;
    wire core_m_axi_wid;
    wire core_m_axi_wlast;
    wire core_m_axi_wvalid;
    wire core_m_axi_wready;
    wire core_m_axi_bid;
    wire [1:0] core_m_axi_bresp;
    wire core_m_axi_bvalid;
    wire core_m_axi_bready;
    wire core_m_axi_arid;
    wire [31:0] core_m_axi_araddr;
    wire [3:0] core_m_axi_arlen;
    wire [2:0] core_m_axi_arsize;
    wire [1:0] core_m_axi_arburst;
    wire core_m_axi_arvalid;
    wire core_m_axi_arready;
    wire core_m_axi_rid;
    wire [AXI_DATA_WIDTH-1:0] core_m_axi_rdata;
    wire [1:0] core_m_axi_rresp;
    wire core_m_axi_rlast;
    wire core_m_axi_rvalid;
    wire core_m_axi_rready;
    loop_pipeline_axi #(
        .LOOP_DEPTH(LOOP_DEPTH),
        .REQUEST_DEPTH(REQUEST_DEPTH),
        .EVENT_DEPTH(EVENT_DEPTH),
        .PROGRAM_DEPTH(PROGRAM_DEPTH),
        .STATE_DEPTH(STATE_DEPTH),
        .JOB_DEPTH(JOB_DEPTH),
        .EXTERNAL_IO(EXTERNAL_IO),
        .MAX_FRAMES(MAX_FRAMES),
        .BUFFERS(BUFFERS),
        .INPUT_PORTS(INPUT_PORTS),
        .OUTPUT_PORTS(OUTPUT_PORTS),
        .REUSE_VIEWS(REUSE_VIEWS),
        .CAPTURE_CHANNELS(CAPTURE_CHANNELS),
        .VIEWS(VIEWS),
        .SLOTS(SLOTS),
        .CHUNK_FRAMES(CHUNK_FRAMES),
        .MEMORY_RESPONSE_CYCLES(MEMORY_RESPONSE_CYCLES),
        .BLOCK_CHUNKS(BLOCK_CHUNKS),
        .RETIRED_TABLES(RETIRED_TABLES),
        .RECLAIM_BLOCKS(RECLAIM_BLOCKS),
        .RECLAIM_CYCLES(RECLAIM_CYCLES),
        .RECLAIM_GUARD(RECLAIM_GUARD),
        .TABLE_PAGES(TABLE_PAGES),
        .TABLE_LINES(TABLE_LINES),
        .REF_LINES(REF_LINES),
        .STACK_LINES(STACK_LINES),
        .READ_BUFFERS(READ_BUFFERS),
        .WRITE_BUFFERS(WRITE_BUFFERS),
        .BURST_MEMORY(BURST_MEMORY),
        .AXI_DATA_WIDTH(AXI_DATA_WIDTH),
        .PHYSICAL_BASE(PHYSICAL_BASE)) core (
        .sample_input_read(sample_input_read),
        .sample_input_address(sample_input_address),
        .sample_input_data(sample_input_data),
        .sample_output_write(sample_output_write),
        .sample_output_address(sample_output_address),
        .sample_output_data(sample_output_data),
        .clk(clk),
        .reset(core_reset),
        .maintenance_enable(maintenance_enable),
        .cut_snapshot_valid(cut_snapshot_valid),
        .cut_snapshot_data(cut_snapshot_data),
        .reclaim_pending(reclaim_pending),
        .reclaim_high_water(reclaim_high_water),
        .reclaim_references(reclaim_references),
        .reclaim_active(reclaim_active),
        .reclaim_released(reclaim_released),
        .reclaim_block(reclaim_block),
        .reclaim_blocks(reclaim_blocks),
        .reclaim_freed(reclaim_freed),
        .reclaim_cycles(reclaim_cycles),
        .reclaim_quiescent(reclaim_quiescent),
        .reclaim_pages_used(reclaim_pages_used),
        .reclaim_pages_available(reclaim_pages_available),
        .reclaim_pages_freed(reclaim_pages_freed),
        .reclaim_capacity_failures(reclaim_capacity_failures),
        .run_valid(run_valid),
        .run_ready(run_ready),
        .run_instruction_count(run_instruction_count),
        .run_state_count(run_state_count),
        .run_quantum(run_quantum),
        .run_chunk_frames(run_chunk_frames),
        .run_job_budget(run_job_budget),
        .run_buffers(run_buffers),
        .run_inputs(run_inputs),
        .run_outputs(run_outputs),
        .job_enable(job_enable),
        .busy(busy),
        .faulted(faulted),
        .done(done),
        .error_code(error_code),
        .error_stage(error_stage),
        .state_busy(state_busy),
        .state_done(state_done),
        .jobs_busy(jobs_busy),
        .jobs_done(jobs_done),
        .buffer_ready(buffer_ready),
        .cfg_valid(cfg_valid),
        .cfg_ready(cfg_ready),
        .cfg_state(cfg_state),
        .cfg_write(cfg_write),
        .cfg_address(cfg_address),
        .cfg_write_data(cfg_write_data),
        .cfg_response_valid(cfg_response_valid),
        .cfg_response_error(cfg_response_error),
        .cfg_read_data(cfg_read_data),
        .audio_cfg_valid(audio_cfg_valid),
        .audio_cfg_ready(audio_cfg_ready),
        .audio_cfg_space(audio_cfg_space),
        .audio_cfg_write(audio_cfg_write),
        .audio_cfg_address(audio_cfg_address),
        .audio_cfg_write_data(audio_cfg_write_data),
        .audio_cfg_response_valid(audio_cfg_response_valid),
        .audio_cfg_response_error(audio_cfg_response_error),
        .audio_cfg_read_data(audio_cfg_read_data),
        .monitor_valid(monitor_valid),
        .monitor_ready(monitor_ready),
        .monitor_data(monitor_data),
        .queued_jobs(queued_jobs),
        .drain_valid(drain_valid),
        .drain_ready(drain_ready),
        .command_valid(command_valid),
        .command_ready(command_ready),
        .command_opcode(command_opcode),
        .command_index(command_index),
        .command_data(command_data),
        .response_valid(response_valid),
        .response_ready(response_ready),
        .response_error(response_error),
        .response_data(response_data),
        .transition_busy(transition_busy),
        .arena_base(arena_base),
        .store_cmd_valid(store_cmd_valid),
        .store_cmd_ready(store_cmd_ready),
        .store_cmd_opcode(store_cmd_opcode),
        .store_cmd_view(store_cmd_view),
        .store_cmd_start(store_cmd_start),
        .store_cmd_length(store_cmd_length),
        .store_cmd_count(store_cmd_count),
        .store_busy(store_busy),
        .store_done(store_done),
        .store_error_code(store_error_code),
        .store_result_view(store_result_view),
        .store_result_empty(store_result_empty),
        .store_write_valid(store_write_valid),
        .store_write_ready(store_write_ready),
        .store_write_data(store_write_data),
        .store_read_valid(store_read_valid),
        .store_read_ready(store_read_ready),
        .store_read_data(store_read_data),
        .store_cfg_valid(store_cfg_valid),
        .store_cfg_ready(store_cfg_ready),
        .store_cfg_write(store_cfg_write),
        .store_cfg_space(store_cfg_space),
        .store_cfg_address(store_cfg_address),
        .store_cfg_write_data(store_cfg_write_data),
        .store_cfg_response_valid(store_cfg_response_valid),
        .store_cfg_response_error(store_cfg_response_error),
        .store_cfg_read_data(store_cfg_read_data),
        .memory_fault(memory_fault),
        .m_axi_awid(core_m_axi_awid),
        .m_axi_awaddr(core_m_axi_awaddr),
        .m_axi_awlen(core_m_axi_awlen),
        .m_axi_awsize(core_m_axi_awsize),
        .m_axi_awburst(core_m_axi_awburst),
        .m_axi_awvalid(core_m_axi_awvalid),
        .m_axi_awready(core_m_axi_awready),
        .m_axi_wdata(core_m_axi_wdata),
        .m_axi_wstrb(core_m_axi_wstrb),
        .m_axi_wid(core_m_axi_wid),
        .m_axi_wlast(core_m_axi_wlast),
        .m_axi_wvalid(core_m_axi_wvalid),
        .m_axi_wready(core_m_axi_wready),
        .m_axi_bid(core_m_axi_bid),
        .m_axi_bresp(core_m_axi_bresp),
        .m_axi_bvalid(core_m_axi_bvalid),
        .m_axi_bready(core_m_axi_bready),
        .m_axi_arid(core_m_axi_arid),
        .m_axi_araddr(core_m_axi_araddr),
        .m_axi_arlen(core_m_axi_arlen),
        .m_axi_arsize(core_m_axi_arsize),
        .m_axi_arburst(core_m_axi_arburst),
        .m_axi_arvalid(core_m_axi_arvalid),
        .m_axi_arready(core_m_axi_arready),
        .m_axi_rid(core_m_axi_rid),
        .m_axi_rdata(core_m_axi_rdata),
        .m_axi_rresp(core_m_axi_rresp),
        .m_axi_rlast(core_m_axi_rlast),
        .m_axi_rvalid(core_m_axi_rvalid),
        .m_axi_rready(core_m_axi_rready)
    );
    `include "timing_body.svh"
endmodule
