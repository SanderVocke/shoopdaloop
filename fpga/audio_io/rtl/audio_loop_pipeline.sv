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
    parameter integer VIEWS = 32,
    parameter integer SLOTS = 1024,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer MEMORY_RESPONSE_CYCLES = 1,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer RETIRED_TABLES = 256,
    parameter integer RECLAIM_BLOCKS = 20, parameter integer RECLAIM_CYCLES = 10000,
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
    wire core_run_valid;
    wire core_cfg_valid;
    wire core_cfg_ready;
    wire core_cfg_state;
    wire core_cfg_write;
    wire [15:0] core_cfg_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] core_cfg_write_data;
    wire core_cfg_response_valid;
    wire core_cfg_response_error;
    wire [`SHOOP_SP_STATE_BITS-1:0] core_cfg_read_data;
    wire core_command_valid;
    wire core_command_ready;
    wire [7:0] core_command_opcode;
    wire [31:0] core_command_index;
    wire [639:0] core_command_data;
    wire core_response_valid;
    wire core_response_ready;
    wire [3:0] core_response_error;
    wire [831:0] core_response_data;
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
        .run_valid(core_run_valid),
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
        .cfg_valid(core_cfg_valid),
        .cfg_ready(core_cfg_ready),
        .cfg_state(core_cfg_state),
        .cfg_write(core_cfg_write),
        .cfg_address(core_cfg_address),
        .cfg_write_data(core_cfg_write_data),
        .cfg_response_valid(core_cfg_response_valid),
        .cfg_response_error(core_cfg_response_error),
        .cfg_read_data(core_cfg_read_data),
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
    wire io_process_valid, io_process_ready, io_busy, fenced, control_fault;
    wire queued_command_valid, queued_command_ready, queued_response_ready;
    wire [7:0] queued_command_opcode;
    wire [31:0] queued_command_index;
    wire [639:0] queued_command_data;
    wire queued_cfg_valid, queued_cfg_ready, queued_cfg_state, queued_cfg_write;
    wire [15:0] queued_cfg_address;
    wire [128:0] queued_cfg_write_data;
    wire [31:0] process_epoch;
    wire unused_epoch = &{1'b0,process_epoch};
    assign session_reset_ready = quiesce && memory_quiescent && !enable;
    wire accepted_reset = session_reset && session_reset_ready;
    assign core_reset = reset || accepted_reset;
    assign core_maintenance_enable = enable && !quiesce && !recovery_requested;
    assign core_run_valid = io_process_valid && fenced && enable && !quiesce;
    assign io_process_ready = run_ready && fenced && enable && !quiesce;
    assign core_command_valid = enable ? queued_command_valid : command_valid;
    assign core_command_opcode = enable ? queued_command_opcode : command_opcode;
    assign core_command_index = enable ? queued_command_index : command_index;
    assign core_command_data = enable ? queued_command_data : command_data;
    assign command_ready = core_command_ready && !enable;
    assign queued_command_ready = core_command_ready && enable;
    assign response_valid = core_response_valid && !enable;
    assign response_error = core_response_error;
    assign response_data = core_response_data;
    assign core_response_ready = enable ? queued_response_ready : response_ready;
    assign core_cfg_valid = enable ? queued_cfg_valid : cfg_valid;
    assign core_cfg_state = enable ? queued_cfg_state : cfg_state;
    assign core_cfg_write = enable ? queued_cfg_write : cfg_write;
    assign core_cfg_address = enable ? queued_cfg_address : cfg_address;
    assign core_cfg_write_data = enable ? queued_cfg_write_data : cfg_write_data;
    assign cfg_ready = core_cfg_ready && !enable;
    assign queued_cfg_ready = core_cfg_ready && enable;
    assign cfg_response_valid = core_cfg_response_valid && !enable;
    assign cfg_response_error = core_cfg_response_error;
    assign cfg_read_data = core_cfg_read_data;

    audio_control #(.DEPTH(CONTROL_DEPTH)) controls (
        .clk(clk),.reset(core_reset),.enable(enable && !recovery_requested && !quiesce),
        .sequence_number(process_sequence),.busy(io_busy),.run_start(core_run_valid && run_ready),
        .fenced(fenced),.fault(control_fault),.control_valid(control_valid),.control_ready(control_ready),.control_data(control_data),
        .command_valid(queued_command_valid),.command_ready(queued_command_ready),
        .command_opcode(queued_command_opcode),.command_index(queued_command_index),.command_data(queued_command_data),
        .response_valid(core_response_valid),.response_ready(queued_response_ready),.response_error(core_response_error),
        .cfg_valid(queued_cfg_valid),.cfg_ready(queued_cfg_ready),.cfg_state(queued_cfg_state),.cfg_write(queued_cfg_write),
        .cfg_address(queued_cfg_address),.cfg_write_data(queued_cfg_write_data),
        .cfg_response_valid(core_cfg_response_valid),.cfg_response_error(core_cfg_response_error)
    );
    audio_io #(.Q(MAX_FRAMES),.P(P),.INPUTS(INPUT_PORTS),.OUTPUTS(OUTPUT_PORTS),
        .RX_BANKS(RX_BANKS),.TX_BANKS(TX_BANKS),.WATCHDOG_CYCLES(WATCHDOG_CYCLES)) io (
        .clk(clk),.audio_clk(audio_clk),.reset(reset),.session_reset(accepted_reset),.enable(enable),.epoch(epoch),
        .frame_tick(frame_tick),.frame_prepare(frame_prepare),.rx_samples(rx_samples),.tx_samples(tx_samples),.tx_valid(tx_valid),
        .audio_frame(audio_frame),.session_frame(session_frame),.process_valid(io_process_valid),.process_ready(io_process_ready),
        .process_sequence(process_sequence),.process_epoch(process_epoch),.process_done(done),
        .process_fault(faulted || control_fault || memory_fault || (enable &&
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
    assign memory_quiescent = reclaim_quiescent && outstanding_reads == 0 && outstanding_writes == 0 && !ar_exposed && !aw_exposed;
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
