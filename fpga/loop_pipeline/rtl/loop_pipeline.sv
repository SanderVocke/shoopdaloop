`include "state_processor_defs.svh"

module loop_pipeline #(
    parameter integer LOOP_DEPTH = 8,
    parameter integer REQUEST_DEPTH = 16,
    parameter integer EVENT_DEPTH = 32,
    parameter integer PROGRAM_DEPTH = 64,
    parameter integer STATE_DEPTH = 32,
    parameter integer JOB_DEPTH = 64,
    parameter integer EXTERNAL_IO = 0,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
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
    parameter integer READ_BUFFERS = 4, parameter integer WRITE_BUFFERS = 4, parameter integer BURST_MEMORY = 0
) (
    output wire sample_input_read,
    output wire [31:0] sample_input_address,
    input wire [31:0] sample_input_data,
    output wire sample_output_write,
    output wire [31:0] sample_output_address,
    output wire [31:0] sample_output_data,
    input wire clk,
    input wire reset,
    input wire maintenance_enable,
    output wire cut_snapshot_valid, output wire [1727:0] cut_snapshot_data,
    output wire [31:0] reclaim_pending, output wire [31:0] reclaim_high_water,
    output wire [63:0] reclaim_references, output wire reclaim_active, output wire reclaim_released,
    output wire [31:0] reclaim_block, output wire [63:0] reclaim_blocks,
    output wire [63:0] reclaim_freed, output wire [63:0] reclaim_cycles,
    output wire reclaim_quiescent,
    output wire [31:0] reclaim_pages_used, output wire [31:0] reclaim_pages_available,
    output wire [63:0] reclaim_pages_freed, output wire [63:0] reclaim_capacity_failures,
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
    input wire store_cmd_valid, output wire store_cmd_ready, input wire [3:0] store_cmd_opcode,
    input wire [63:0] store_cmd_view, input wire [63:0] store_cmd_start, input wire [63:0] store_cmd_length,
    input wire [31:0] store_cmd_count,
    output wire store_busy, output wire store_done, output wire [7:0] store_error_code,
    output wire [31:0] store_result_view, output wire store_result_empty,
    input wire store_write_valid, output wire store_write_ready, input wire [31:0] store_write_data,
    output wire store_read_valid, input wire store_read_ready, output wire [31:0] store_read_data,
    input wire store_cfg_valid, output wire store_cfg_ready, input wire store_cfg_write,
    input wire [2:0] store_cfg_space, input wire [31:0] store_cfg_address, input wire [159:0] store_cfg_write_data,
    output wire store_cfg_response_valid, output wire store_cfg_response_error, output wire [159:0] store_cfg_read_data,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    output wire [63:0] memory_arena_base,
    output wire [31:0] memory_count,
    output wire memory_write_valid, input wire memory_write_ready,
    input wire memory_read_valid, output wire memory_read_ready,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data,
    output wire memory_meta_valid, input wire memory_meta_ready,
    output wire memory_meta_write, output wire [63:0] memory_meta_address,
    output wire [31:0] memory_meta_write_data,
    output wire [31:0] memory_meta_count,
    output wire memory_meta_write_valid, input wire memory_meta_write_ready,
    input wire memory_meta_read_valid, output wire memory_meta_read_ready,
    input wire memory_meta_response_valid, output wire memory_meta_response_ready,
    input wire memory_meta_response_error, input wire [31:0] memory_meta_read_data,
    output wire memory_fault
);
    typedef enum logic [4:0] { IDLE, SETTLE_SEND, SETTLE_WAIT, AUDIO_START,
        AUDIO_WAIT, FINISH_SEND, FINISH_WAIT, FAULT, HOST_SETTLE_WAIT, BOUNDARY,
        PRE_READ, PRE_WAIT, PRE_CUT, PRE_CUT_WAIT,
        POST_READ, POST_WAIT, POST_CUT, POST_CUT_WAIT } phase_t;
    phase_t phase;
    wire host_access = phase == IDLE || phase == FAULT || phase == HOST_SETTLE_WAIT || phase == BOUNDARY;
    wire coordinator_send = (phase == SETTLE_SEND || phase == FINISH_SEND) && !memory_fault;
    wire reserved_command = command_opcode == 9 || command_opcode == 11;
    wire tp_command_ready, tp_response_valid;
    wire tp_execute_ready, tp_execute_done;
    wire [3:0] tp_execute_error;
    wire [3:0] tp_response_error;
    wire [831:0] tp_response_data;
    wire tp_busy;
    wire [31:0] configured_quantum;
    wire loop_read_valid, loop_read_ready, loop_response_valid, loop_response_error;
    wire observe_valid=CAPTURE_CHANNELS != 0 && (phase == PRE_READ || phase == POST_READ);
    wire observe_ready, observe_response_valid, observe_response_error;
    wire [831:0] observe_data;
    wire [63:0] observe_frame;
    wire [1:0] observe_phase;
    reg [831:0] observed_state;
    reg [63:0] observed_frame;
    wire cut_request_ready, cut_response_valid, cut_fault, cut_busy;
    wire [3:0] cut_error;
    wire ledger_valid, ledger_ready, ledger_done;
    wire [3:0] ledger_opcode;
    wire [63:0] ledger_view, ledger_length, capture_roots, playback_roots;
    wire ledger_owns=CAPTURE_CHANNELS != 0 && (phase == PRE_CUT || phase == PRE_CUT_WAIT || phase == POST_CUT || phase == POST_CUT_WAIT);
    wire [7:0] internal_store_error;
    wire [31:0] internal_store_result;
    wire internal_store_done;
    wire unused_resident_ready,unused_resident_done;
    wire [3:0] unused_resident_error;
    wire [831:0] unused_resident_result;
    wire unused_cut=&{1'b0,unused_resident_ready,unused_resident_done,unused_resident_error,unused_resident_result,cut_busy,observe_phase,observed_state,observed_frame,ledger_ready,ledger_done};
    generate if (CAPTURE_CHANNELS != 0) begin : managed_media
        initial if (LOOP_DEPTH != 1 || REUSE_VIEWS == 0 || CHUNK_FRAMES != 32 || CAPTURE_CHANNELS > 2)
            $fatal(1,"Invalid application capture profile");
        media_cuts #(.CHANNELS(CAPTURE_CHANNELS)) ownership (
            .clk(clk),.reset(reset),.request_valid(phase == PRE_CUT || phase == POST_CUT),.request_ready(cut_request_ready),
            .request_opcode(3'b0),.request_latest(1'b0),.request_slot(32'b0),.request_generation(32'b0),
            .request_state(observed_state),.request_frame(observed_frame),.request_epoch(64'b0),
            .response_valid(cut_response_valid),.response_error(cut_error),.response_data(cut_snapshot_data),.busy(cut_busy),.fault(cut_fault),
            .capture_view0(capture_roots[31:0]),.capture_view1(capture_roots[63:32]),
            .playback_view0(playback_roots[31:0]),.playback_view1(playback_roots[63:32]),
            .media_valid(ledger_valid),.media_ready(ledger_ready),.media_opcode(ledger_opcode),.media_view(ledger_view),.media_length(ledger_length),
            .media_done(ledger_done),.media_error(internal_store_error),.media_result_view(internal_store_result)
        );
    end else begin : unmanaged_media
        assign cut_request_ready=0; assign cut_response_valid=0; assign cut_error=0; assign cut_fault=0; assign cut_busy=0;
        assign capture_roots=0; assign playback_roots=0; assign ledger_valid=0; assign ledger_opcode=0; assign ledger_view=0; assign ledger_length=0;
        assign cut_snapshot_data=0;
    end endgenerate
    assign cut_snapshot_valid=cut_response_valid && cut_error == 0;
    assign ledger_ready=host_store_cmd_ready && ledger_owns;
    assign ledger_done=internal_store_done && ledger_owns;
    assign store_done=internal_store_done && !ledger_owns;
    assign store_error_code=internal_store_error;
    assign store_result_view=internal_store_result;
    wire [31:0] loop_read_slot;
    wire [163:0] loop_response_data;
    wire audio_run_ready, audio_done, audio_busy, state_cfg_ready, sample_cfg_ready;
    wire [7:0] audio_error;
    wire [1:0] audio_error_stage;
    wire host_store_cmd_ready, host_store_cfg_ready;
    wire host_store_write_ready, host_store_read_valid;
    reg [63:0] quantum_arena;
    reg [31:0] instructions, states, quantum, chunk_frames, budget, buffers, inputs, outputs;
    reg fault_report_pending;
    wire configure = phase == IDLE || phase == FAULT || phase == BOUNDARY;
    reg output_valid, response_is_output;
    wire [31:0] sample_read_data;
    assign faulted = phase == FAULT;
    assign audio_cfg_read_data = response_is_output && !output_valid ? 32'b0 : sample_read_data;
    assign transition_busy = tp_busy;

    assign busy = (phase != IDLE && phase != FAULT) || audio_busy;
    assign run_ready = (phase == IDLE || phase == BOUNDARY) && tp_execute_ready
        && !cfg_valid && !audio_cfg_valid && !store_cmd_valid && !store_cfg_valid
        && !cfg_response_valid && !audio_cfg_response_valid && !store_cfg_response_valid && !reset;
    assign command_ready = host_access && !reserved_command && !(command_opcode == 16 && phase != IDLE) && tp_command_ready && !(run_valid && run_ready) && !reset;
    assign response_valid = tp_response_valid && !reset;
    assign response_error = tp_response_error;
    assign response_data = tp_response_data;
    assign cfg_ready = configure && state_cfg_ready;
    assign audio_cfg_ready = configure && sample_cfg_ready;
    assign store_cmd_ready = CAPTURE_CHANNELS == 0 && configure && host_store_cmd_ready;
    assign store_cfg_ready = CAPTURE_CHANNELS == 0 && configure && host_store_cfg_ready;
    assign store_write_ready = CAPTURE_CHANNELS == 0 && configure && host_store_write_ready;
    assign store_read_valid = CAPTURE_CHANNELS == 0 && configure && host_store_read_valid;

    transition_processor #(.LOOP_DEPTH(LOOP_DEPTH), .REQUEST_DEPTH(REQUEST_DEPTH), .EVENT_DEPTH(EVENT_DEPTH), .INTEGRATED(1)) timing (
        .clk(clk), .reset(reset),
        .command_valid(host_access && command_valid && !reserved_command && !(command_opcode == 16 && phase != IDLE) && !(run_valid && run_ready)),
        .command_ready(tp_command_ready),
        .command_opcode(command_opcode == 16 ? 8'd9 : command_opcode),
        .command_index(command_index),
        .command_data(command_data),
        .response_valid(tp_response_valid), .response_ready(response_ready),
        .execute_valid(coordinator_send), .execute_ready(tp_execute_ready),
        .execute_finish(phase == FINISH_SEND), .execute_done(tp_execute_done), .execute_error(tp_execute_error),
        .response_error(tp_response_error), .response_data(tp_response_data), .busy(tp_busy),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data),
        .observe_valid(observe_valid),.observe_ready(observe_ready),.observe_kind(2'b0),.observe_index(32'b0),
        .observe_response_valid(observe_response_valid),.observe_response_error(observe_response_error),
        .observe_frame(observe_frame),.observe_phase(observe_phase),.observe_data(observe_data),
        .resident_valid(1'b0),.resident_ready(unused_resident_ready),.resident_opcode(8'b0),.resident_index(32'b0),.resident_data(640'b0),
        .resident_done(unused_resident_done),.resident_error(unused_resident_error),.resident_result(unused_resident_result),
        .configured_quantum(configured_quantum)
    );
    wire maintenance_start = (run_valid && run_ready && phase != BOUNDARY)
        || (command_valid && command_ready && command_opcode == 16);
    wire iteration_waiting = ((phase == IDLE || phase == BOUNDARY) && run_valid)
        || (phase == IDLE && command_valid && command_opcode == 16);
    media_audio_pipeline #(.CAPTURE_CHANNELS(CAPTURE_CHANNELS), .MAINTENANCE_AT_RUN(0), .REUSE_VIEWS(REUSE_VIEWS), .EXTERNAL_IO(EXTERNAL_IO), .PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH), .JOB_DEPTH(JOB_DEPTH),
        .MAX_FRAMES(MAX_FRAMES), .BUFFERS(BUFFERS), .INPUT_PORTS(INPUT_PORTS), .OUTPUT_PORTS(OUTPUT_PORTS),
        .VIEWS(VIEWS), .SLOTS(SLOTS), .CHUNK_FRAMES(CHUNK_FRAMES),
        .MEMORY_RESPONSE_CYCLES(MEMORY_RESPONSE_CYCLES), .READ_BUFFERS(READ_BUFFERS), .WRITE_BUFFERS(WRITE_BUFFERS), .BURST_MEMORY(BURST_MEMORY), .BLOCK_CHUNKS(BLOCK_CHUNKS),.RETIRED_TABLES(RETIRED_TABLES),.RECLAIM_BLOCKS(RECLAIM_BLOCKS),.RECLAIM_CYCLES(RECLAIM_CYCLES),.RECLAIM_GUARD(RECLAIM_GUARD), .TABLE_PAGES(TABLE_PAGES), .TABLE_LINES(TABLE_LINES), .REF_LINES(REF_LINES), .STACK_LINES(STACK_LINES)) audio (
        .reclaim_pages_used(reclaim_pages_used),.reclaim_pages_available(reclaim_pages_available),
        .reclaim_pages_freed(reclaim_pages_freed),.reclaim_capacity_failures(reclaim_capacity_failures),
        .maintenance_tick(maintenance_start),.maintenance_enable(maintenance_enable && !iteration_waiting),.reclaim_pending(reclaim_pending),.reclaim_high_water(reclaim_high_water),
        .reclaim_references(reclaim_references),.reclaim_active(reclaim_active),.reclaim_released(reclaim_released),
        .reclaim_block(reclaim_block),.reclaim_blocks(reclaim_blocks),.reclaim_freed(reclaim_freed),
        .reclaim_cycles(reclaim_cycles),.reclaim_quiescent(reclaim_quiescent),
        .sample_input_read(sample_input_read), .sample_input_address(sample_input_address), .sample_input_data(sample_input_data),
        .sample_output_write(sample_output_write), .sample_output_address(sample_output_address), .sample_output_data(sample_output_data),
        .clk(clk), .reset(reset), .capture_roots(capture_roots), .playback_roots(playback_roots), .run_valid(phase == AUDIO_START), .run_ready(audio_run_ready),
        .run_instruction_count(instructions), .run_state_count(states), .run_quantum(quantum), .run_chunk_frames(chunk_frames), .run_job_budget(budget),
        .run_buffers(buffers), .run_inputs(inputs), .run_outputs(outputs), .job_enable(job_enable),
        .busy(audio_busy), .done(audio_done), .error_code(audio_error), .error_stage(audio_error_stage),
        .state_busy(state_busy), .state_done(state_done), .jobs_busy(jobs_busy), .jobs_done(jobs_done), .buffer_ready(buffer_ready),
        .cfg_valid(cfg_valid && configure), .cfg_ready(state_cfg_ready), .cfg_state(cfg_state), .cfg_write(cfg_write),
        .cfg_address(cfg_address), .cfg_write_data(cfg_write_data), .cfg_response_valid(cfg_response_valid),
        .cfg_response_error(cfg_response_error), .cfg_read_data(cfg_read_data),
        .audio_cfg_valid(audio_cfg_valid && configure), .audio_cfg_ready(sample_cfg_ready), .audio_cfg_space(audio_cfg_space),
        .audio_cfg_write(audio_cfg_write), .audio_cfg_address(audio_cfg_address), .audio_cfg_write_data(audio_cfg_write_data),
        .audio_cfg_response_valid(audio_cfg_response_valid), .audio_cfg_response_error(audio_cfg_response_error), .audio_cfg_read_data(sample_read_data),
        .monitor_valid(monitor_valid), .monitor_ready(monitor_ready), .monitor_data(monitor_data), .queued_jobs(queued_jobs),
        .drain_valid(drain_valid), .drain_ready(drain_ready),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data),
        .arena_base((phase == IDLE || phase == FAULT || phase == BOUNDARY) ? arena_base : quantum_arena),
        .store_cmd_valid(ledger_owns ? ledger_valid : CAPTURE_CHANNELS == 0 && store_cmd_valid && configure), .store_cmd_ready(host_store_cmd_ready),
        .store_cmd_opcode(ledger_owns ? ledger_opcode : store_cmd_opcode), .store_cmd_view(ledger_owns ? ledger_view : store_cmd_view),
        .store_cmd_start(ledger_owns ? 64'b0 : store_cmd_start), .store_cmd_length(ledger_owns ? ledger_length : store_cmd_length),
        .store_cmd_count(ledger_owns ? 32'b0 : store_cmd_count),
        .store_busy(store_busy), .store_done(internal_store_done), .store_error_code(internal_store_error),
        .store_result_view(internal_store_result), .store_result_empty(store_result_empty),
        .store_write_valid(CAPTURE_CHANNELS == 0 && store_write_valid && configure), .store_write_ready(host_store_write_ready),
        .store_write_data(store_write_data), .store_read_valid(host_store_read_valid),
        .store_read_ready(CAPTURE_CHANNELS == 0 && store_read_ready && configure), .store_read_data(store_read_data),
        .store_cfg_valid(CAPTURE_CHANNELS == 0 && store_cfg_valid && configure), .store_cfg_ready(host_store_cfg_ready), .store_cfg_write(store_cfg_write),
        .store_cfg_space(store_cfg_space), .store_cfg_address(store_cfg_address), .store_cfg_write_data(store_cfg_write_data),
        .store_cfg_response_valid(store_cfg_response_valid), .store_cfg_response_error(store_cfg_response_error),
        .store_cfg_read_data(store_cfg_read_data),
        .memory_valid(memory_valid), .memory_ready(memory_ready), .memory_write(memory_write),
        .memory_address(memory_address), .memory_write_data(memory_write_data), .memory_arena_base(memory_arena_base),
        .memory_count(memory_count), .memory_write_valid(memory_write_valid),
        .memory_write_ready(memory_write_ready), .memory_read_valid(memory_read_valid),
        .memory_read_ready(memory_read_ready),
        .memory_response_valid(memory_response_valid), .memory_response_ready(memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data), .memory_meta_valid(memory_meta_valid), .memory_meta_ready(memory_meta_ready), .memory_meta_write(memory_meta_write), .memory_meta_address(memory_meta_address), .memory_meta_write_data(memory_meta_write_data), .memory_meta_count(memory_meta_count), .memory_meta_write_valid(memory_meta_write_valid), .memory_meta_write_ready(memory_meta_write_ready), .memory_meta_read_valid(memory_meta_read_valid), .memory_meta_read_ready(memory_meta_read_ready), .memory_meta_response_valid(memory_meta_response_valid), .memory_meta_response_ready(memory_meta_response_ready), .memory_meta_response_error(memory_meta_response_error), .memory_meta_read_data(memory_meta_read_data), .memory_fault(memory_fault)
    );

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; done <= 0; error_code <= 0; error_stage <= 0;
            output_valid <= 0; response_is_output <= 0;
            instructions <= 0; states <= 0; quantum <= 0; chunk_frames <= 0; budget <= 0;
            buffers <= 0; inputs <= 0; outputs <= 0; quantum_arena <= 0;
            fault_report_pending <= 0; observed_state<=0; observed_frame<=0;
        end else begin
            done <= 0;
            if (audio_cfg_valid && audio_cfg_ready) response_is_output <= audio_cfg_space == 1 && !audio_cfg_write;
            if (memory_fault && phase != FAULT) begin
                error_code <= 8; error_stage <= 3; fault_report_pending <= 1; output_valid <= 0; phase <= FAULT;
            end else case (phase)
                HOST_SETTLE_WAIT: if (tp_response_valid) begin
                    if (tp_response_error != 0) begin
                        error_stage <= 1; error_code <= {4'b0,tp_response_error};
                        output_valid <= 0; done <= 1; phase <= FAULT;
                    end else phase <= BOUNDARY;
                end
                IDLE, BOUNDARY: if (run_valid && run_ready) begin
                    error_code <= 0; error_stage <= 0; output_valid <= 0;
                    if (configured_quantum == 0 || run_quantum != configured_quantum) begin
                        error_code <= 1; error_stage <= 1; done <= 1;
                    end else if (run_instruction_count > PROGRAM_DEPTH || run_state_count > STATE_DEPTH
                        || run_job_budget > JOB_DEPTH || queued_jobs != 0) begin
                        error_code <= run_job_budget > JOB_DEPTH || queued_jobs != 0 ? 8'd1 : 8'd2;
                        error_stage <= 2; done <= 1;
                    end else begin
                        instructions <= run_instruction_count; states <= run_state_count; quantum <= run_quantum; chunk_frames <= run_chunk_frames;
                        budget <= run_job_budget; buffers <= run_buffers; inputs <= run_inputs; outputs <= run_outputs;
                        quantum_arena <= arena_base;
                        phase <= phase == BOUNDARY ? (CAPTURE_CHANNELS == 0 ? AUDIO_START : PRE_READ) : SETTLE_SEND;
                    end
                end
                else if (phase == IDLE && command_valid && command_ready && command_opcode == 16) phase <= HOST_SETTLE_WAIT;
                SETTLE_SEND: if (tp_execute_ready) phase <= SETTLE_WAIT;
                SETTLE_WAIT: if (tp_execute_done) begin
                    if (tp_execute_error != 0) begin
                        error_stage <= 1; error_code <= {4'b0,tp_execute_error}; done <= 1; phase <= FAULT;
                    end else phase <= CAPTURE_CHANNELS == 0 ? AUDIO_START : PRE_READ;
                end
                PRE_READ, POST_READ: if (observe_ready) phase<=phase == PRE_READ ? PRE_WAIT : POST_WAIT;
                PRE_WAIT, POST_WAIT: if (observe_response_valid) begin
                    if (observe_response_error) begin error_stage<=1; error_code<=3; done<=1; phase<=FAULT; end
                    else begin
                        observed_state<=observe_data; observed_frame<=observe_frame;
                        phase<=phase == PRE_WAIT ? PRE_CUT : POST_CUT;
                    end
                end
                PRE_CUT, POST_CUT: if (cut_request_ready) phase<=phase == PRE_CUT ? PRE_CUT_WAIT : POST_CUT_WAIT;
                PRE_CUT_WAIT, POST_CUT_WAIT: if (cut_response_valid || cut_fault) begin
                    if (cut_error != 0 || cut_fault) begin error_stage<=3; error_code<={4'b0,cut_error}; done<=1; phase<=FAULT; end
                    else if (phase == PRE_CUT_WAIT) phase<=AUDIO_START;
                    else begin error_stage<=0; error_code<=0; done<=1; output_valid<=1; phase<=IDLE; end
                end
                AUDIO_START: if (audio_run_ready) phase <= AUDIO_WAIT;
                AUDIO_WAIT: if (audio_done) begin
                    if (audio_error != 0) begin
                        error_stage <= audio_error_stage == 1 ? 2'd2 : 2'd3;
                        error_code <= audio_error; done <= 1; phase <= FAULT;
                    end else phase <= FINISH_SEND;
                end
                FINISH_SEND: if (tp_execute_ready) phase <= FINISH_WAIT;
                FINISH_WAIT: if (tp_execute_done) begin
                    error_stage <= tp_execute_error == 0 ? 2'd0 : 2'd1;
                    error_code <= {4'b0,tp_execute_error}; done <= CAPTURE_CHANNELS == 0 || tp_execute_error != 0;
                    output_valid <= tp_execute_error == 0 && CAPTURE_CHANNELS == 0;
                    phase <= tp_execute_error != 0 ? FAULT : CAPTURE_CHANNELS == 0 ? IDLE : POST_READ;
                end
                FAULT: if (fault_report_pending && !audio_busy) begin done <= 1; fault_report_pending <= 0; end
                default: phase <= FAULT;
            endcase
        end
    end
endmodule
