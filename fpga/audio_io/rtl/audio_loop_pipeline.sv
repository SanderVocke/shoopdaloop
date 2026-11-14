// Generated continuous-audio AXI wrapper; do not edit by hand.
`include "state_processor_defs.svh"

module audio_loop_pipeline #(
    parameter integer LOOP_DEPTH = 16,
    parameter integer REQUEST_DEPTH = 128,
    parameter integer EVENT_DEPTH = 256,
    parameter integer PROGRAM_DEPTH = 256,
    parameter integer STATE_DEPTH = 128,
    parameter integer JOB_DEPTH = 256,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 128,
    parameter integer INPUT_PORTS = 16,
    parameter integer OUTPUT_PORTS = 16,
    parameter integer REUSE_VIEWS = 0,
    parameter integer CAPTURE_CHANNELS = 0,
    parameter integer RESIDENT_ACTIONS = 0,
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
    parameter [31:0] PHYSICAL_BASE = 32'h01000000,
    parameter integer P = MAX_FRAMES, RX_BANKS = 3, TX_BANKS = 3,
    parameter integer WATCHDOG_CYCLES = 1000000, CONTROL_DEPTH = 128
) (
    input wire clk,
    input wire reset,
    output wire cut_snapshot_valid,
    output wire [1727:0] cut_snapshot_data,
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
    output wire m_axi_rready,
    input wire audio_clk, input wire session_reset, input wire enable,
    input wire quiesce, output wire memory_quiescent, output wire session_reset_ready,
    input wire [31:0] epoch, input wire frame_tick, input wire frame_prepare,
    input wire [INPUT_PORTS*32-1:0] rx_samples,
    output wire [OUTPUT_PORTS*32-1:0] tx_samples, output wire tx_valid,
    output wire [63:0] audio_frame, output wire [63:0] session_frame,
    input wire control_valid, output wire control_ready, input wire [745:0] control_data,
    output wire control_result_valid, input wire control_result_ready, output wire [1087:0] control_result_data,
    output wire recovery_requested, output wire [3:0] recovery_reason,
    output wire [63:0] failure_sequence, output wire [63:0] process_sequence,
    output wire [31:0] processed_quanta, output wire [31:0] played_quanta,
    output wire [31:0] missed_deadlines, output wire [31:0] discarded_quanta,
    output wire [31:0] input_losses, output wire [31:0] rx_high_water, output wire [31:0] tx_high_water
);
    wire sample_input_read;
    wire [31:0] sample_input_address;
    wire [31:0] sample_input_data;
    wire sample_output_write;
    wire [31:0] sample_output_address;
    wire [31:0] sample_output_data;
    wire core_reset;
    wire core_maintenance_enable;
    wire core_resident_command_valid;
    wire core_resident_command_ready;
    wire [7:0] core_resident_command_opcode;
    wire [31:0] core_resident_command_index;
    wire [639:0] core_resident_command_data;
    wire core_resident_command_done;
    wire [3:0] core_resident_command_error;
    wire [831:0] core_resident_command_result;
    wire core_resident_cut_valid;
    wire core_resident_cut_ready;
    wire [1:0] core_resident_cut_mode;
    wire core_resident_cut_done;
    wire [63:0] core_resident_epoch;
    wire core_resident_media_valid;
    wire core_resident_media_ready;
    wire [2:0] core_resident_media_opcode;
    wire core_resident_media_latest;
    wire [31:0] core_resident_media_slot;
    wire [31:0] core_resident_media_generation;
    wire core_resident_media_done;
    wire [3:0] core_resident_media_error;
    wire [1727:0] core_resident_media_data;
    wire core_run_valid;
    wire [31:0] core_run_instruction_count;
    wire [31:0] core_run_state_count;
    wire [31:0] core_run_job_budget;
    wire [31:0] core_run_buffers;
    wire core_cfg_valid;
    wire core_cfg_ready;
    wire core_cfg_state;
    wire core_cfg_write;
    wire [15:0] core_cfg_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] core_cfg_write_data;
    wire core_cfg_response_valid;
    wire core_cfg_response_error;
    wire [`SHOOP_SP_STATE_BITS-1:0] core_cfg_read_data;
    wire core_audio_cfg_valid;
    wire core_audio_cfg_ready;
    wire [1:0] core_audio_cfg_space;
    wire core_audio_cfg_write;
    wire [31:0] core_audio_cfg_address;
    wire [31:0] core_audio_cfg_write_data;
    wire core_audio_cfg_response_valid;
    wire core_audio_cfg_response_error;
    wire [31:0] core_audio_cfg_read_data;
    wire core_command_valid;
    wire core_command_ready;
    wire [7:0] core_command_opcode;
    wire [31:0] core_command_index;
    wire [639:0] core_command_data;
    wire core_response_valid;
    wire core_response_ready;
    wire [3:0] core_response_error;
    wire [831:0] core_response_data;
    wire core_store_cmd_valid;
    wire core_store_cmd_ready;
    wire [3:0] core_store_cmd_opcode;
    wire [63:0] core_store_cmd_view;
    wire [63:0] core_store_cmd_start;
    wire [63:0] core_store_cmd_length;
    wire [31:0] core_store_cmd_count;
    wire core_store_write_valid;
    wire core_store_write_ready;
    wire [31:0] core_store_write_data;
    wire core_store_read_valid;
    wire core_store_read_ready;
    wire [31:0] core_store_read_data;
    wire core_store_cfg_valid;
    wire core_store_cfg_ready;
    wire core_store_cfg_write;
    wire [2:0] core_store_cfg_space;
    wire [31:0] core_store_cfg_address;
    wire [159:0] core_store_cfg_write_data;
    wire core_store_cfg_response_valid;
    wire core_store_cfg_response_error;
    wire [159:0] core_store_cfg_read_data;
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
        .EXTERNAL_IO(1),
        .MAX_FRAMES(MAX_FRAMES),
        .BUFFERS(BUFFERS),
        .INPUT_PORTS(INPUT_PORTS),
        .OUTPUT_PORTS(OUTPUT_PORTS),
        .REUSE_VIEWS(REUSE_VIEWS),
        .CAPTURE_CHANNELS(CAPTURE_CHANNELS),
        .RESIDENT_ACTIONS(RESIDENT_ACTIONS),
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
        .maintenance_enable(core_maintenance_enable),
        .resident_command_valid(core_resident_command_valid),
        .resident_command_ready(core_resident_command_ready),
        .resident_command_opcode(core_resident_command_opcode),
        .resident_command_index(core_resident_command_index),
        .resident_command_data(core_resident_command_data),
        .resident_command_done(core_resident_command_done),
        .resident_command_error(core_resident_command_error),
        .resident_command_result(core_resident_command_result),
        .resident_cut_valid(core_resident_cut_valid),
        .resident_cut_ready(core_resident_cut_ready),
        .resident_cut_mode(core_resident_cut_mode),
        .resident_cut_done(core_resident_cut_done),
        .resident_epoch(core_resident_epoch),
        .resident_media_valid(core_resident_media_valid),
        .resident_media_ready(core_resident_media_ready),
        .resident_media_opcode(core_resident_media_opcode),
        .resident_media_latest(core_resident_media_latest),
        .resident_media_slot(core_resident_media_slot),
        .resident_media_generation(core_resident_media_generation),
        .resident_media_done(core_resident_media_done),
        .resident_media_error(core_resident_media_error),
        .resident_media_data(core_resident_media_data),
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
        .run_valid(core_run_valid),
        .run_ready(run_ready),
        .run_instruction_count(core_run_instruction_count),
        .run_state_count(core_run_state_count),
        .run_quantum(run_quantum),
        .run_chunk_frames(run_chunk_frames),
        .run_job_budget(core_run_job_budget),
        .run_buffers(core_run_buffers),
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
        .cfg_valid(core_cfg_valid),
        .cfg_ready(core_cfg_ready),
        .cfg_state(core_cfg_state),
        .cfg_write(core_cfg_write),
        .cfg_address(core_cfg_address),
        .cfg_write_data(core_cfg_write_data),
        .cfg_response_valid(core_cfg_response_valid),
        .cfg_response_error(core_cfg_response_error),
        .cfg_read_data(core_cfg_read_data),
        .audio_cfg_valid(core_audio_cfg_valid),
        .audio_cfg_ready(core_audio_cfg_ready),
        .audio_cfg_space(core_audio_cfg_space),
        .audio_cfg_write(core_audio_cfg_write),
        .audio_cfg_address(core_audio_cfg_address),
        .audio_cfg_write_data(core_audio_cfg_write_data),
        .audio_cfg_response_valid(core_audio_cfg_response_valid),
        .audio_cfg_response_error(core_audio_cfg_response_error),
        .audio_cfg_read_data(core_audio_cfg_read_data),
        .monitor_valid(monitor_valid),
        .monitor_ready(monitor_ready),
        .monitor_data(monitor_data),
        .queued_jobs(queued_jobs),
        .drain_valid(drain_valid),
        .drain_ready(drain_ready),
        .command_valid(core_command_valid),
        .command_ready(core_command_ready),
        .command_opcode(core_command_opcode),
        .command_index(core_command_index),
        .command_data(core_command_data),
        .response_valid(core_response_valid),
        .response_ready(core_response_ready),
        .response_error(core_response_error),
        .response_data(core_response_data),
        .transition_busy(transition_busy),
        .arena_base(arena_base),
        .store_cmd_valid(core_store_cmd_valid),
        .store_cmd_ready(core_store_cmd_ready),
        .store_cmd_opcode(core_store_cmd_opcode),
        .store_cmd_view(core_store_cmd_view),
        .store_cmd_start(core_store_cmd_start),
        .store_cmd_length(core_store_cmd_length),
        .store_cmd_count(core_store_cmd_count),
        .store_busy(store_busy),
        .store_done(store_done),
        .store_error_code(store_error_code),
        .store_result_view(store_result_view),
        .store_result_empty(store_result_empty),
        .store_write_valid(core_store_write_valid),
        .store_write_ready(core_store_write_ready),
        .store_write_data(core_store_write_data),
        .store_read_valid(core_store_read_valid),
        .store_read_ready(core_store_read_ready),
        .store_read_data(core_store_read_data),
        .store_cfg_valid(core_store_cfg_valid),
        .store_cfg_ready(core_store_cfg_ready),
        .store_cfg_write(core_store_cfg_write),
        .store_cfg_space(core_store_cfg_space),
        .store_cfg_address(core_store_cfg_address),
        .store_cfg_write_data(core_store_cfg_write_data),
        .store_cfg_response_valid(core_store_cfg_response_valid),
        .store_cfg_response_error(core_store_cfg_response_error),
        .store_cfg_read_data(core_store_cfg_read_data),
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
    wire io_process_valid, io_process_ready, io_busy, run_permitted, control_fault;
    wire queued_command_valid, queued_command_ready, queued_response_ready;
    reg external_command_pending;
    wire [7:0] queued_command_opcode;
    wire [31:0] queued_command_index;
    wire [639:0] queued_command_data;
    wire queued_cfg_valid, queued_cfg_ready, queued_cfg_state, queued_cfg_write;
    wire [7:0] queued_cfg_opcode;
    wire [15:0] queued_cfg_address;
    wire [639:0] queued_cfg_write_data;
    wire image_busy, image_fault, image_override, image_response_valid;
    wire [3:0] image_response_error;
    wire [128:0] image_response_data;
    wire image_request_ready;
    wire configuration_busy,configuration_fault,configuration_ready,configuration_done;
    wire [3:0] configuration_error;
    wire [128:0] configuration_result;
    wire configuration_image_valid,configuration_image_state;
    wire [7:0] configuration_image_opcode;
    wire [15:0] configuration_image_address;
    wire [639:0] configuration_image_data;
    wire [63:0] configuration_graph_epoch,configuration_topology_epoch;
    wire [31:0] configuration_smoothing_frames;
    wire configuration_smoothing_linear,configuration_loaded;
    wire unused_configuration_status=&{1'b0,configuration_topology_epoch,configuration_smoothing_frames,
        configuration_smoothing_linear,configuration_loaded};
    wire private_configuration=RESIDENT_ACTIONS != 0 && (queued_cfg_opcode == 49 || queued_cfg_opcode == 50);
    wire actions_request_ready;
    wire [31:0] image_instructions, image_states, image_jobs, image_buffers;
    wire image_cfg_valid, image_cfg_state, image_cfg_write, batch_fault;
    wire [15:0] image_cfg_address;
    wire [128:0] image_cfg_data;
    wire action_busy, action_fault, action_done, action_cfg_valid, action_cfg_ready, action_cfg_state, action_response_valid;
    wire [7:0] action_cfg_opcode;
    wire [15:0] action_cfg_address;
    wire [639:0] action_cfg_data;
    wire [3:0] action_response_error;
    wire [831:0] action_response_data;
    reg cut_reserved, cut_ready, post_action;
    wire native_fault;
    wire pre_action_start = io_process_valid && !io_busy && run_permitted && !action_busy && !cut_reserved
        && enable && !quiesce && !recovery_requested;
    assign core_resident_cut_ready=!action_busy && !cut_reserved && enable && !quiesce && !recovery_requested;
    wire post_action_start=core_resident_cut_valid && core_resident_cut_ready;
    wire action_start=pre_action_start || post_action_start;
    assign core_resident_cut_done=action_done && post_action;
    assign core_resident_epoch=configuration_graph_epoch;
    wire unused_resident_core=&{1'b0,core_resident_command_ready,core_resident_command_done,core_resident_command_error,core_resident_command_result,core_resident_cut_mode,core_resident_media_ready,core_resident_media_done,core_resident_media_error,core_resident_media_data};
    wire image_owns_cfg = enable || image_busy;
    assign control_fault = batch_fault || image_fault || action_fault || native_fault || configuration_fault;
    assign core_run_instruction_count = image_override ? image_instructions : run_instruction_count;
    assign core_run_state_count = image_override ? image_states : run_state_count;
    assign core_run_job_budget = image_override ? image_jobs : run_job_budget;
    assign core_run_buffers = image_override ? image_buffers : run_buffers;
    wire [31:0] process_epoch;
    wire unused_epoch = &{1'b0,process_epoch,queued_cfg_write};
    assign session_reset_ready = quiesce && memory_quiescent && !enable;
    wire accepted_reset = session_reset && session_reset_ready;
    assign core_reset = reset || accepted_reset;
    assign core_maintenance_enable = enable && !quiesce && !recovery_requested;
    assign core_run_valid = io_process_valid && run_permitted && cut_ready && enable && !quiesce;
    assign io_process_ready = run_ready && run_permitted && cut_ready && enable && !quiesce;
    always @(posedge clk) begin
        if (core_reset) begin cut_reserved<=0; cut_ready<=0; post_action<=0; end
        else begin
            if (action_start) begin post_action<=post_action_start; if (!post_action_start) cut_reserved<=1; end
            if (action_done && !post_action) cut_ready<=1;
            if (action_done && post_action) post_action<=0;
            if (core_run_valid && run_ready) begin cut_reserved<=0; cut_ready<=0; end
        end
    end
    assign core_command_valid = enable ? queued_command_valid : command_valid;
    assign core_command_opcode = enable ? queued_command_opcode : command_opcode;
    assign core_command_index = enable ? queued_command_index : command_index;
    assign core_command_data = enable ? queued_command_data : command_data;
    assign command_ready = core_command_ready && !enable;
    assign queued_command_ready = core_command_ready && enable;
    assign response_valid = core_response_valid && external_command_pending;
    assign response_error = core_response_error;
    assign response_data = core_response_data;
    assign core_response_ready = external_command_pending ? response_ready : queued_response_ready;
    always @(posedge clk) begin
        if (core_reset) external_command_pending<=0;
        else begin
            if (response_valid && response_ready) external_command_pending<=0;
            if (command_valid && command_ready) external_command_pending<=1;
        end
    end
    assign core_cfg_valid = image_owns_cfg ? image_cfg_valid : cfg_valid;
    assign core_cfg_state = image_owns_cfg ? image_cfg_state : cfg_state;
    assign core_cfg_write = image_owns_cfg ? image_cfg_write : cfg_write;
    assign core_cfg_address = image_owns_cfg ? image_cfg_address : cfg_address;
    assign core_cfg_write_data = image_owns_cfg ? image_cfg_data : cfg_write_data;
    assign cfg_ready = core_cfg_ready && !image_owns_cfg;
    assign cfg_response_valid = core_cfg_response_valid && !image_owns_cfg;
    assign cfg_response_error = core_cfg_response_error;
    assign cfg_read_data = core_cfg_read_data;

    assign core_audio_cfg_valid = audio_cfg_valid && !enable;
    assign audio_cfg_ready = core_audio_cfg_ready && !enable;
    assign core_audio_cfg_space = audio_cfg_space;
    assign core_audio_cfg_write = audio_cfg_write;
    assign core_audio_cfg_address = audio_cfg_address;
    assign core_audio_cfg_write_data = audio_cfg_write_data;
    assign audio_cfg_response_valid = core_audio_cfg_response_valid && !enable;
    assign audio_cfg_response_error = core_audio_cfg_response_error;
    assign audio_cfg_read_data = core_audio_cfg_read_data;
    assign core_store_cmd_valid = store_cmd_valid && !enable;
    assign store_cmd_ready = core_store_cmd_ready && !enable;
    assign core_store_cmd_opcode = store_cmd_opcode;
    assign core_store_cmd_view = store_cmd_view;
    assign core_store_cmd_start = store_cmd_start;
    assign core_store_cmd_length = store_cmd_length;
    assign core_store_cmd_count = store_cmd_count;
    assign core_store_write_valid = store_write_valid && !enable;
    assign store_write_ready = core_store_write_ready && !enable;
    assign core_store_write_data = store_write_data;
    assign store_read_valid = core_store_read_valid && !enable;
    assign core_store_read_ready = store_read_ready && !enable;
    assign store_read_data = core_store_read_data;
    assign core_store_cfg_valid = store_cfg_valid && !enable;
    assign store_cfg_ready = core_store_cfg_ready && !enable;
    assign core_store_cfg_write = store_cfg_write;
    assign core_store_cfg_space = store_cfg_space;
    assign core_store_cfg_address = store_cfg_address;
    assign core_store_cfg_write_data = store_cfg_write_data;
    assign store_cfg_response_valid = core_store_cfg_response_valid && !enable;
    assign store_cfg_response_error = core_store_cfg_response_error;
    assign store_cfg_read_data = core_store_cfg_read_data;

    wire action_resident_valid;
    wire [7:0] action_resident_opcode;
    wire [511:0] action_resident_data;
    wire [63:0] action_resident_sequence;
    wire [63:0] action_resident_frame;
    wire [47:0] action_resident_reference;
    wire [15:0] action_resident_slot;
    wire [31:0] action_resident_generation;
    wire [1:0] action_resident_mode;
    wire action_lookup_ready;
    wire action_lookup_done;
    wire [3:0] action_lookup_error;
    wire [767:0] action_lookup_data;
    wire action_update_ready;
    wire action_update_done;
    wire [3:0] action_update_error;
    wire unused_action_private=&{1'b0,action_resident_valid,action_resident_opcode,action_resident_data,action_resident_sequence,action_resident_frame,action_resident_reference,action_resident_slot,action_resident_generation,action_resident_mode,action_lookup_ready,action_lookup_done,action_lookup_error,action_lookup_data,action_update_ready,action_update_done,action_update_error};
    wire action_record_request_valid,action_record_request_ready,action_record_request_ack;
    wire [15:0] action_record_request_slot;
    wire [31:0] action_record_request_generation,action_record_request_epoch;
    wire [4:0] action_record_request_word;
    wire action_record_response_valid;
    wire [3:0] action_record_response_error;
    wire [63:0] action_record_response_data;
    wire unused_record_service=&{1'b0,action_record_request_valid,action_record_request_ack,action_record_request_slot,
        action_record_request_generation,action_record_request_epoch,action_record_request_word};
    wire action_resident_ready,action_resident_done,action_resident_pending;
    wire [3:0] action_resident_error;
    wire [511:0] action_resident_result;
    wire action_lookup_valid,action_update_valid;
    wire [15:0] action_lookup_slot,action_update_slot;
    wire [31:0] action_update_generation;
    wire [767:0] action_update_data;
    generate if (RESIDENT_ACTIONS != 0) begin : public_delivery
        wire observation_valid,observation_ready,observation_done;
        wire [7:0] observation_opcode;
        wire [511:0] observation_data,observation_result;
        wire [3:0] observation_error;
        wire observation_fault;
        wire observation_record_valid,observation_record_ready,observation_record_done;
        wire [15:0] observation_record_slot;
        wire [31:0] observation_record_generation;
        wire [1919:0] observation_record_data;
        wire [3:0] observation_record_error;
        wire [63:0] unused_observation_epoch;
        wire native_delivery_fault;
        wire configuration_native_valid,configuration_native_ready,configuration_native_done;
        wire [7:0] configuration_native_opcode;
        wire [511:0] configuration_native_data,configuration_native_result;
        wire configuration_tp_valid,native_tp_valid;
        wire [7:0] configuration_tp_opcode,native_tp_opcode;
        wire [639:0] configuration_tp_data,native_tp_data;
        wire [31:0] native_tp_index;
        wire unused_runtime_ready,unused_runtime_done;
        wire [3:0] unused_runtime_error;
        wire [128:0] unused_runtime_result;
        wire unused_configuration=&{1'b0,configuration_loaded,unused_runtime_ready,unused_runtime_done,unused_runtime_error,unused_runtime_result};
        assign native_fault=native_delivery_fault || observation_fault;
        audio_configuration #(.STATE_DEPTH(STATE_DEPTH)) configuration_owner (
            .clk(clk),.reset(core_reset),.configuration_valid(queued_cfg_valid && private_configuration),
            .configuration_ready(configuration_ready),.configuration_opcode(queued_cfg_opcode),.configuration_data(queued_cfg_write_data),
            .configuration_done(configuration_done),.configuration_error(configuration_error),.configuration_result(configuration_result),
            .request_valid(configuration_native_valid),.request_ready(configuration_native_ready),
            .request_opcode(configuration_native_opcode),.request_data(configuration_native_data),
            .response_valid(configuration_native_done),.response_data(configuration_native_result),
            .runtime_valid(1'b0),.runtime_ready(unused_runtime_ready),.runtime_data(512'b0),.runtime_done(unused_runtime_done),
            .runtime_error(unused_runtime_error),.runtime_result(unused_runtime_result),
            .busy(configuration_busy),.fault(configuration_fault),.graph_epoch(configuration_graph_epoch),.topology_epoch(configuration_topology_epoch),.loaded(configuration_loaded),
            .smoothing_frames(configuration_smoothing_frames),.smoothing_linear(configuration_smoothing_linear),
            .image_valid(configuration_image_valid),.image_ready(image_request_ready),.image_opcode(configuration_image_opcode),
            .image_state(configuration_image_state),.image_address(configuration_image_address),.image_data(configuration_image_data),
            .image_done(image_response_valid),.image_error(image_response_error),.image_result(image_response_data),
            .tp_valid(configuration_tp_valid),.tp_ready(core_resident_command_ready && configuration_busy),
            .tp_opcode(configuration_tp_opcode),.tp_data(configuration_tp_data),
            .tp_done(core_resident_command_done),.tp_error(core_resident_command_error),.tp_result(core_resident_command_result)
        );
        assign core_resident_command_valid=configuration_busy ? configuration_tp_valid : native_tp_valid;
        assign core_resident_command_opcode=configuration_busy ? configuration_tp_opcode : native_tp_opcode;
        assign core_resident_command_index=configuration_busy ? 32'b0 : native_tp_index;
        assign core_resident_command_data=configuration_busy ? configuration_tp_data : native_tp_data;
        audio_observations #(.SLOTS(CONTROL_DEPTH)) observations (
            .clk(clk),.reset(core_reset),.epoch(epoch),.request_valid(observation_valid),.request_ready(observation_ready),
            .request_opcode(observation_opcode),.request_data(observation_data),.request_slot(action_resident_slot),.request_generation(action_resident_generation),
            .cut_data(core_resident_media_data),.scalar_data({configuration_smoothing_linear,configuration_smoothing_frames,32'b0,32'h3f800000}),.topology_epoch(configuration_topology_epoch),
            .response_valid(observation_done),.response_error(observation_error),.response_data(observation_result),.fault(observation_fault),
            .ledger_valid(core_resident_media_valid),.ledger_ready(core_resident_media_ready),.ledger_opcode(core_resident_media_opcode),
            .ledger_latest(core_resident_media_latest),.ledger_slot(core_resident_media_slot),.ledger_generation(core_resident_media_generation),
            .ledger_epoch(unused_observation_epoch),.ledger_done(core_resident_media_done),.ledger_error(core_resident_media_error),
            .record_valid(observation_record_valid),.record_ready(observation_record_ready),.record_slot(observation_record_slot),
            .record_generation(observation_record_generation),.record_data(observation_record_data),.record_done(observation_record_done),.record_error(observation_record_error)
        );
        cut_records #(.SLOTS(CONTROL_DEPTH),.WORDS(30)) records (
            .clk(clk),.reset(core_reset),.epoch(epoch),.commit_valid(observation_record_valid),.commit_ready(observation_record_ready),
            .commit_slot(observation_record_slot),.commit_generation(observation_record_generation),.commit_data(observation_record_data),
            .commit_done(observation_record_done),.commit_error(observation_record_error),
            .request_valid(action_record_request_valid),.request_ready(action_record_request_ready),.request_ack(action_record_request_ack),
            .request_slot(action_record_request_slot),.request_generation(action_record_request_generation),.request_epoch(action_record_request_epoch),.request_word(action_record_request_word),
            .response_valid(action_record_response_valid),.response_error(action_record_response_error),.response_data(action_record_response_data)
        );
        audio_native #(.REQUESTS(REQUEST_DEPTH)) native (
            .clk(clk),.reset(core_reset),.request_valid(action_resident_valid),.request_ready(action_resident_ready),
            .request_opcode(action_resident_opcode),.request_flags(action_cfg_address),.request_mode(action_resident_mode),
            .request_data(action_resident_data),.request_sequence(action_resident_sequence),.request_frame(action_resident_frame),
            .request_reference(action_resident_reference),.response_valid(action_resident_done),.response_pending(action_resident_pending),
            .response_error(action_resident_error),.response_data(action_resident_result),.fault(native_delivery_fault),
            .observation_valid(observation_valid),.observation_ready(observation_ready),.observation_opcode(observation_opcode),.observation_data(observation_data),
            .observation_done(observation_done),.observation_error(observation_error),.observation_result(observation_result),
            .configuration_valid(configuration_native_valid),.configuration_ready(configuration_native_ready),
            .configuration_opcode(configuration_native_opcode),.configuration_data(configuration_native_data),
            .configuration_done(configuration_native_done),.configuration_result(configuration_native_result),
            .tp_valid(native_tp_valid),.tp_ready(core_resident_command_ready && !configuration_busy),.tp_opcode(native_tp_opcode),
            .tp_index(native_tp_index),.tp_data(native_tp_data),.tp_done(core_resident_command_done),
            .tp_error(core_resident_command_error),.tp_result(core_resident_command_result),
            .lookup_valid(action_lookup_valid),.lookup_ready(action_lookup_ready),.lookup_slot(action_lookup_slot),
            .lookup_done(action_lookup_done),.lookup_error(action_lookup_error),.lookup_data(action_lookup_data),
            .update_valid(action_update_valid),.update_ready(action_update_ready),.update_slot(action_update_slot),
            .update_generation(action_update_generation),.update_data(action_update_data),.update_done(action_update_done),.update_error(action_update_error)
        );
    end else begin : configuration_delivery
        assign configuration_busy=0;assign configuration_fault=0;assign configuration_ready=0;assign configuration_done=0;
        assign configuration_error=0;assign configuration_result=0;assign configuration_image_valid=0;
        assign configuration_image_opcode=0;assign configuration_image_state=0;assign configuration_image_address=0;assign configuration_image_data=0;
        assign configuration_graph_epoch=0;assign configuration_topology_epoch=0;assign configuration_smoothing_frames=0;
        assign configuration_smoothing_linear=0;assign configuration_loaded=0;
        assign native_fault=0;
        assign action_record_request_ready=0;assign action_record_response_valid=0;assign action_record_response_error=0;assign action_record_response_data=0;
        assign core_resident_media_valid=0;assign core_resident_media_opcode=0;assign core_resident_media_latest=0;
        assign core_resident_media_slot=0;assign core_resident_media_generation=0;
        assign core_resident_command_valid=0; assign core_resident_command_opcode=0;
        assign core_resident_command_index=0; assign core_resident_command_data=0;
        assign action_resident_ready=0; assign action_resident_done=0; assign action_resident_pending=0;
        assign action_resident_error=0; assign action_resident_result=0;
        assign action_lookup_valid=0; assign action_lookup_slot=0; assign action_update_valid=0;
        assign action_update_slot=0; assign action_update_generation=0; assign action_update_data=0;
    end endgenerate
    audio_actions #(.SLOTS(CONTROL_DEPTH),.QUANTUM(MAX_FRAMES),
        .MAX_WORK(RESIDENT_ACTIONS != 0 ? 32000 : 8000),.WATCHDOG(RESIDENT_ACTIONS != 0 ? 65536 : 16384),
        .ACTIVATION_WORK(RESIDENT_ACTIONS != 0 ? 48*STATE_DEPTH+6*PROGRAM_DEPTH+256 : 10*STATE_DEPTH+4*PROGRAM_DEPTH+32),.NATIVE_ACTIONS(RESIDENT_ACTIONS)) actions (
        .record_request_valid(action_record_request_valid),
        .record_request_ack(action_record_request_ack),
        .record_request_slot(action_record_request_slot),
        .record_request_generation(action_record_request_generation),
        .record_request_epoch(action_record_request_epoch),
        .record_request_word(action_record_request_word),
        .record_request_ready(action_record_request_ready),
        .record_response_valid(action_record_response_valid),
        .record_response_error(action_record_response_error),
        .record_response_data(action_record_response_data),

        .resident_valid(action_resident_valid),
        .resident_opcode(action_resident_opcode),
        .resident_data(action_resident_data),
        .resident_sequence(action_resident_sequence),
        .resident_frame(action_resident_frame),
        .resident_reference(action_resident_reference),
        .resident_slot(action_resident_slot),
        .resident_generation(action_resident_generation),
        .resident_mode(action_resident_mode),
        .lookup_ready(action_lookup_ready),
        .lookup_done(action_lookup_done),
        .lookup_error(action_lookup_error),
        .lookup_data(action_lookup_data),
        .update_ready(action_update_ready),
        .update_done(action_update_done),
        .update_error(action_update_error),
        .cut_mode(post_action_start ? core_resident_cut_mode : 2'b0),
        .resident_ready(action_resident_ready),
        .resident_done(action_resident_done),
        .resident_pending(action_resident_pending),
        .resident_error(action_resident_error),
        .resident_result(action_resident_result),
        .lookup_valid(action_lookup_valid),
        .lookup_slot(action_lookup_slot),
        .update_valid(action_update_valid),
        .update_slot(action_update_slot),
        .update_generation(action_update_generation),
        .update_data(action_update_data),

        .clk(clk),.reset(core_reset),.current_frame(process_sequence*64'(MAX_FRAMES)),
        .cut_start(action_start),.cut_done(action_done),.busy(action_busy),.fault(action_fault),
        .request_valid(queued_cfg_valid && !private_configuration),.request_ready(actions_request_ready),.request_opcode(queued_cfg_opcode),
        .request_state(queued_cfg_state),.request_address(queued_cfg_address),.request_data(queued_cfg_write_data),
        .response_valid(action_response_valid),.response_error(action_response_error),.response_data(action_response_data),
        .cfg_valid(action_cfg_valid),.cfg_ready(action_cfg_ready),.cfg_opcode(action_cfg_opcode),
        .cfg_state(action_cfg_state),.cfg_address(action_cfg_address),.cfg_data(action_cfg_data),
        .cfg_response_valid(image_response_valid),.cfg_response_error(image_response_error),.cfg_response_data(image_response_data)
    );
    assign queued_cfg_ready=private_configuration ? configuration_ready : actions_request_ready;
    assign action_cfg_ready=image_request_ready && !configuration_busy;
    audio_image #(.PROGRAM_DEPTH(PROGRAM_DEPTH),.STATE_DEPTH(STATE_DEPTH),.JOB_DEPTH(JOB_DEPTH),.BUFFERS(BUFFERS),
        .BANKS(RESIDENT_ACTIONS != 0 ? 2 : 1),.CANONICAL(RESIDENT_ACTIONS)) images (
        .clk(clk),.reset(core_reset),.execution_idle(!io_busy || (RESIDENT_ACTIONS != 0 && post_action && !state_busy && !jobs_busy)),.current_state_count(run_state_count),
        .request_valid(configuration_image_valid || (action_cfg_valid && !configuration_busy)),.request_ready(image_request_ready),
        .request_opcode(configuration_image_valid ? configuration_image_opcode : action_cfg_opcode),
        .request_state(configuration_image_valid ? configuration_image_state : action_cfg_state),
        .request_address(configuration_image_valid ? configuration_image_address : action_cfg_address),
        .request_data(configuration_image_valid ? configuration_image_data : action_cfg_data),
        .busy(image_busy),.response_valid(image_response_valid),.response_error(image_response_error),.response_data(image_response_data),.fault(image_fault),
        .active_override(image_override),.active_instructions(image_instructions),.active_states(image_states),.active_jobs(image_jobs),.active_buffers(image_buffers),
        .cfg_valid(image_cfg_valid),.cfg_ready(core_cfg_ready),.cfg_state(image_cfg_state),.cfg_write(image_cfg_write),
        .cfg_address(image_cfg_address),.cfg_write_data(image_cfg_data),.cfg_response_valid(core_cfg_response_valid),
        .cfg_response_error(core_cfg_response_error),.cfg_read_data(core_cfg_read_data)
    );
    audio_control #(.RESIDENT_ACTIONS(RESIDENT_ACTIONS),.DEPTH(CONTROL_DEPTH),.ACTION_SLOTS(CONTROL_DEPTH),.ACTIVATION_WORK(10*STATE_DEPTH+4*PROGRAM_DEPTH+32)) controls (
        .clk(clk),.reset(core_reset),.enable(enable && !recovery_requested && !quiesce),
        .sequence_number(process_sequence),.epoch(epoch),.busy(io_busy || action_busy || cut_reserved),.run_start(core_run_valid && run_ready),
        .admission_blocked(external_command_pending),
        .run_permitted(run_permitted),.fault(batch_fault),.control_valid(control_valid),.control_ready(control_ready),.control_data(control_data),
        .command_valid(queued_command_valid),.command_ready(queued_command_ready),
        .command_opcode(queued_command_opcode),.command_index(queued_command_index),.command_data(queued_command_data),
        .response_valid(core_response_valid),.response_ready(queued_response_ready),.response_error(core_response_error),
        .response_data(core_response_data),.result_valid(control_result_valid),.result_ready(control_result_ready),.result_data(control_result_data),
        .cfg_valid(queued_cfg_valid),.cfg_ready(queued_cfg_ready),.cfg_state(queued_cfg_state),.cfg_write(queued_cfg_write),
        .cfg_opcode(queued_cfg_opcode),.cfg_address(queued_cfg_address),.cfg_write_data(queued_cfg_write_data),
        .cfg_response_valid(action_response_valid || configuration_done),
        .cfg_response_error(configuration_done ? configuration_error : action_response_error),
        .cfg_read_data(configuration_done ? {703'b0,configuration_result} : action_response_data)
    );
    audio_io #(.Q(MAX_FRAMES),.P(P),.INPUTS(INPUT_PORTS),.OUTPUTS(OUTPUT_PORTS),
        .RX_BANKS(RX_BANKS),.TX_BANKS(TX_BANKS),.WATCHDOG_CYCLES(WATCHDOG_CYCLES)) io (
        .clk(clk),.audio_clk(audio_clk),.reset(reset),.session_reset(accepted_reset),.enable(enable),.epoch(epoch),
        .frame_tick(frame_tick),.frame_prepare(frame_prepare),.rx_samples(rx_samples),.tx_samples(tx_samples),.tx_valid(tx_valid),
        .audio_frame(audio_frame),.session_frame(session_frame),.process_valid(io_process_valid),.process_ready(io_process_ready),
        .process_sequence(process_sequence),.process_epoch(process_epoch),.process_done(done),
        .process_fault(faulted || control_fault || memory_fault || (done && error_code != 0) || (enable &&
            (run_quantum != MAX_FRAMES || run_chunk_frames != CHUNK_FRAMES || run_inputs != INPUT_PORTS || run_outputs != OUTPUT_PORTS))),
        .input_read(sample_input_read),.input_address(sample_input_address),.input_data(sample_input_data),
        .output_write(sample_output_write),.output_address(sample_output_address),.output_data(sample_output_data),
        .recovery_requested(recovery_requested),.recovery_reason(recovery_reason),.failure_sequence(failure_sequence),
        .processed_quanta(processed_quanta),.played_quanta(played_quanta),.missed_deadlines(missed_deadlines),
        .discarded_quanta(discarded_quanta),.input_losses(input_losses),.rx_high_water(rx_high_water),.tx_high_water(tx_high_water),.busy(io_busy)
    );
    initial if (MAX_FRAMES != CHUNK_FRAMES && MAX_FRAMES % CHUNK_FRAMES != 0)
        $fatal(1,"Quantum must contain whole media chunks");

    reg [15:0] outstanding_reads, outstanding_writes;
    reg ar_exposed, aw_exposed;
    wire isolate = (quiesce && reclaim_quiescent) || recovery_requested;
    assign m_axi_arvalid = core_m_axi_arvalid && (!isolate || ar_exposed);
    assign core_m_axi_arready = m_axi_arready && (!isolate || ar_exposed);
    assign m_axi_awvalid = core_m_axi_awvalid && (!isolate || aw_exposed);
    assign core_m_axi_awready = m_axi_awready && (!isolate || aw_exposed);
    assign m_axi_wvalid = core_m_axi_wvalid && outstanding_writes != 0;
    assign core_m_axi_wready = m_axi_wready && outstanding_writes != 0;
    assign memory_quiescent = (recovery_requested || reclaim_quiescent)
        && outstanding_reads == 0 && outstanding_writes == 0 && !ar_exposed && !aw_exposed;
    always @(posedge clk) begin
        if (reset) begin outstanding_reads<=0; outstanding_writes<=0; ar_exposed<=0; aw_exposed<=0; end
        else begin
            if (m_axi_arvalid) ar_exposed<=!m_axi_arready;
            if (m_axi_awvalid) aw_exposed<=!m_axi_awready;
            case ({m_axi_arvalid && m_axi_arready,m_axi_rvalid && m_axi_rready && m_axi_rlast})
                2'b10: outstanding_reads<=outstanding_reads+1'b1;
                2'b01: outstanding_reads<=outstanding_reads-1'b1;
                default: begin end
            endcase
            case ({m_axi_awvalid && m_axi_awready,m_axi_bvalid && m_axi_bready})
                2'b10: outstanding_writes<=outstanding_writes+1'b1;
                2'b01: outstanding_writes<=outstanding_writes-1'b1;
                default: begin end
            endcase
        end
    end
    assign m_axi_awid = core_m_axi_awid;
    assign m_axi_awaddr = core_m_axi_awaddr;
    assign m_axi_awlen = core_m_axi_awlen;
    assign m_axi_awsize = core_m_axi_awsize;
    assign m_axi_awburst = core_m_axi_awburst;
    assign m_axi_wid = core_m_axi_wid;
    assign m_axi_wdata = core_m_axi_wdata;
    assign m_axi_wstrb = core_m_axi_wstrb;
    assign m_axi_wlast = core_m_axi_wlast;
    assign core_m_axi_bid = m_axi_bid;
    assign core_m_axi_bresp = m_axi_bresp;
    assign core_m_axi_bvalid = m_axi_bvalid;
    assign m_axi_bready = core_m_axi_bready;
    assign m_axi_arid = core_m_axi_arid;
    assign m_axi_araddr = core_m_axi_araddr;
    assign m_axi_arlen = core_m_axi_arlen;
    assign m_axi_arsize = core_m_axi_arsize;
    assign m_axi_arburst = core_m_axi_arburst;
    assign core_m_axi_rid = m_axi_rid;
    assign core_m_axi_rdata = m_axi_rdata;
    assign core_m_axi_rresp = m_axi_rresp;
    assign core_m_axi_rlast = m_axi_rlast;
    assign core_m_axi_rvalid = m_axi_rvalid;
    assign m_axi_rready = core_m_axi_rready;

endmodule
