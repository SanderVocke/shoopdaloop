`include "state_processor_defs.svh"

module loop_pipeline #(
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
    parameter integer SLOTS = 32,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer MEMORY_RESPONSE_CYCLES = 1,
    parameter integer READ_BUFFERS = 4, parameter integer WRITE_BUFFERS = 4
) (
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
    input wire run_external_jobs,
    output wire external_job_valid,
    input wire external_job_ready,
    input wire external_done_valid,
    output wire external_done_ready,
    input wire [7:0] external_error,
    input wire command_valid,
    output wire command_ready,
    input wire [7:0] command_opcode,
    input wire [31:0] command_index,
    input wire [639:0] command_data,
    output wire response_valid,
    input wire response_ready,
    output wire [3:0] response_error,
    output wire [831:0] response_data,
    output wire preflight_valid,
    input wire preflight_ready,
    input wire preflight_retry,
    output reg [3:0] preflight_error,
    output reg [127:0] boundary_token,
    output wire transition_busy,
    input wire [63:0] arena_base,
    input wire store_cmd_valid, output wire store_cmd_ready, input wire [2:0] store_cmd_opcode,
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
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data,
    output wire memory_fault
);
    typedef enum logic [3:0] { IDLE, PREVIEW_SEND, PREVIEW_WAIT, PREFLIGHT,
        COMMIT_SEND, COMMIT_WAIT, AUDIO_START, AUDIO_WAIT, FINISH_SEND, FINISH_WAIT, FAULT } phase_t;
    phase_t phase;
    wire host_access = phase == IDLE || phase == PREFLIGHT || phase == FAULT;
    wire coordinator_send = (phase == PREVIEW_SEND || phase == COMMIT_SEND || phase == FINISH_SEND) && !memory_fault;
    wire coordinator_wait = phase == PREVIEW_WAIT || phase == COMMIT_WAIT || phase == FINISH_WAIT;
    wire reserved_command = command_opcode == 9 || command_opcode == 10 || command_opcode == 11;
    wire tp_command_ready, tp_response_valid;
    wire [3:0] tp_response_error;
    wire [831:0] tp_response_data;
    wire tp_busy;
    wire [31:0] configured_quantum;
    wire loop_read_valid, loop_read_ready, loop_response_valid, loop_response_error;
    wire [31:0] loop_read_slot;
    wire [162:0] loop_response_data;
    wire audio_run_ready, audio_done, audio_busy, state_cfg_ready, sample_cfg_ready;
    wire [7:0] audio_error;
    wire [1:0] audio_error_stage;
    wire host_store_cmd_ready, host_store_cfg_ready;
    wire host_store_write_ready, host_store_read_valid;
    reg [63:0] quantum_arena;
    reg [31:0] instructions, states, quantum, chunk_frames, budget, buffers, inputs, outputs;
    reg external_jobs;
    reg fault_report_pending;
    wire configure = phase == IDLE || phase == PREFLIGHT || phase == FAULT;
    assign transition_busy = tp_busy;

    assign busy = (phase != IDLE && phase != FAULT) || audio_busy;
    assign run_ready = phase == IDLE && audio_run_ready && tp_command_ready && !command_valid && !reset;
    assign preflight_valid = phase == PREFLIGHT && audio_run_ready && tp_command_ready
        && !command_valid && !cfg_valid && !audio_cfg_valid && !store_cmd_valid && !store_cfg_valid && !reset;
    assign command_ready = host_access && !reserved_command && tp_command_ready && !reset;
    assign response_valid = host_access && tp_response_valid && !reset;
    assign response_error = tp_response_error;
    assign response_data = tp_response_data;
    assign cfg_ready = configure && state_cfg_ready;
    assign audio_cfg_ready = configure && sample_cfg_ready;
    assign store_cmd_ready = configure && host_store_cmd_ready;
    assign store_cfg_ready = configure && host_store_cfg_ready;
    assign store_write_ready = configure && host_store_write_ready;
    assign store_read_valid = configure && host_store_read_valid;

    transition_processor #(.LOOP_DEPTH(LOOP_DEPTH), .REQUEST_DEPTH(REQUEST_DEPTH), .EVENT_DEPTH(EVENT_DEPTH)) timing (
        .clk(clk), .reset(reset),
        .command_valid(coordinator_send || (host_access && command_valid && !reserved_command)),
        .command_ready(tp_command_ready),
        .command_opcode(coordinator_send ? (phase == PREVIEW_SEND ? 8'd9 : phase == COMMIT_SEND ? 8'd10 : 8'd11) : command_opcode),
        .command_index(coordinator_send ? 32'b0 : command_index),
        .command_data(coordinator_send ? {512'b0,boundary_token} : command_data),
        .response_valid(tp_response_valid), .response_ready(coordinator_wait || (host_access && response_ready)),
        .response_error(tp_response_error), .response_data(tp_response_data), .busy(tp_busy),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data),
        .configured_quantum(configured_quantum)
    );
    media_audio_pipeline #(.PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH), .JOB_DEPTH(JOB_DEPTH),
        .MAX_FRAMES(MAX_FRAMES), .BUFFERS(BUFFERS), .INPUT_PORTS(INPUT_PORTS), .OUTPUT_PORTS(OUTPUT_PORTS),
        .VIEWS(VIEWS), .SLOTS(SLOTS), .CHUNK_FRAMES(CHUNK_FRAMES),
        .MEMORY_RESPONSE_CYCLES(MEMORY_RESPONSE_CYCLES), .READ_BUFFERS(READ_BUFFERS), .WRITE_BUFFERS(WRITE_BUFFERS)) audio (
        .clk(clk), .reset(reset), .run_valid(phase == AUDIO_START), .run_ready(audio_run_ready),
        .run_instruction_count(instructions), .run_state_count(states), .run_quantum(quantum), .run_chunk_frames(chunk_frames), .run_job_budget(budget),
        .run_buffers(buffers), .run_inputs(inputs), .run_outputs(outputs), .job_enable(job_enable),
        .run_external_jobs(external_jobs), .external_job_valid(external_job_valid), .external_job_ready(external_job_ready),
        .external_done_valid(external_done_valid), .external_done_ready(external_done_ready), .external_error(external_error),
        .busy(audio_busy), .done(audio_done), .error_code(audio_error), .error_stage(audio_error_stage),
        .state_busy(state_busy), .state_done(state_done), .jobs_busy(jobs_busy), .jobs_done(jobs_done), .buffer_ready(buffer_ready),
        .cfg_valid(cfg_valid && configure), .cfg_ready(state_cfg_ready), .cfg_state(cfg_state), .cfg_write(cfg_write),
        .cfg_address(cfg_address), .cfg_write_data(cfg_write_data), .cfg_response_valid(cfg_response_valid),
        .cfg_response_error(cfg_response_error), .cfg_read_data(cfg_read_data),
        .audio_cfg_valid(audio_cfg_valid && configure), .audio_cfg_ready(sample_cfg_ready), .audio_cfg_space(audio_cfg_space),
        .audio_cfg_write(audio_cfg_write), .audio_cfg_address(audio_cfg_address), .audio_cfg_write_data(audio_cfg_write_data),
        .audio_cfg_response_valid(audio_cfg_response_valid), .audio_cfg_response_error(audio_cfg_response_error), .audio_cfg_read_data(audio_cfg_read_data),
        .monitor_valid(monitor_valid), .monitor_ready(monitor_ready), .monitor_data(monitor_data), .queued_jobs(queued_jobs),
        .drain_valid(drain_valid), .drain_ready(drain_ready),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data),
        .arena_base((phase == IDLE || phase == FAULT) ? arena_base : quantum_arena),
        .store_cmd_valid(store_cmd_valid && configure), .store_cmd_ready(host_store_cmd_ready),
        .store_cmd_opcode(store_cmd_opcode), .store_cmd_view(store_cmd_view),
        .store_cmd_start(store_cmd_start), .store_cmd_length(store_cmd_length), .store_cmd_count(store_cmd_count),
        .store_busy(store_busy), .store_done(store_done), .store_error_code(store_error_code),
        .store_result_view(store_result_view), .store_result_empty(store_result_empty),
        .store_write_valid(store_write_valid && configure), .store_write_ready(host_store_write_ready),
        .store_write_data(store_write_data), .store_read_valid(host_store_read_valid),
        .store_read_ready(store_read_ready && configure), .store_read_data(store_read_data),
        .store_cfg_valid(store_cfg_valid && configure), .store_cfg_ready(host_store_cfg_ready), .store_cfg_write(store_cfg_write),
        .store_cfg_space(store_cfg_space), .store_cfg_address(store_cfg_address), .store_cfg_write_data(store_cfg_write_data),
        .store_cfg_response_valid(store_cfg_response_valid), .store_cfg_response_error(store_cfg_response_error),
        .store_cfg_read_data(store_cfg_read_data),
        .memory_valid(memory_valid), .memory_ready(memory_ready), .memory_write(memory_write),
        .memory_address(memory_address), .memory_write_data(memory_write_data), .memory_arena_base(memory_arena_base),
        .memory_response_valid(memory_response_valid), .memory_response_ready(memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data), .memory_fault(memory_fault)
    );

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; done <= 0; error_code <= 0; error_stage <= 0;
            preflight_error <= 0; boundary_token <= 0;
            instructions <= 0; states <= 0; quantum <= 0; chunk_frames <= 0; budget <= 0;
            buffers <= 0; inputs <= 0; outputs <= 0; external_jobs <= 0; quantum_arena <= 0;
            fault_report_pending <= 0;
        end else begin
            done <= 0;
            if (memory_fault && phase != FAULT) begin
                error_code <= 8; error_stage <= 3; fault_report_pending <= 1; phase <= FAULT;
            end else case (phase)
                IDLE: if (run_valid && run_ready) begin
                    error_code <= 0; error_stage <= 0;
                    if (configured_quantum == 0 || run_quantum != configured_quantum) begin
                        error_code <= 1; error_stage <= 1; done <= 1;
                    end else if (run_instruction_count > PROGRAM_DEPTH || run_state_count > STATE_DEPTH
                        || run_job_budget > JOB_DEPTH || queued_jobs != 0) begin
                        error_code <= run_job_budget > JOB_DEPTH || queued_jobs != 0 ? 8'd1 : 8'd2;
                        error_stage <= 2; done <= 1;
                    end else begin
                        instructions <= run_instruction_count; states <= run_state_count; quantum <= run_quantum; chunk_frames <= run_chunk_frames;
                        budget <= run_job_budget; buffers <= run_buffers; inputs <= run_inputs; outputs <= run_outputs;
                        external_jobs <= run_external_jobs; quantum_arena <= arena_base; phase <= PREVIEW_SEND;
                    end
                end
                PREVIEW_SEND: if (tp_command_ready) phase <= PREVIEW_WAIT;
                PREVIEW_WAIT: if (tp_response_valid) begin
                    boundary_token <= tp_response_data[127:0]; preflight_error <= tp_response_error; phase <= PREFLIGHT;
                end
                PREFLIGHT: if (preflight_valid && preflight_ready) begin
                    if (preflight_retry) phase <= PREVIEW_SEND;
                    else if (preflight_error != 0) begin
                        error_stage <= 1; error_code <= {4'b0,preflight_error}; done <= 1; phase <= IDLE;
                    end else phase <= COMMIT_SEND;
                end
                COMMIT_SEND: if (tp_command_ready) phase <= COMMIT_WAIT;
                COMMIT_WAIT: if (tp_response_valid) begin
                    if (tp_response_error != 0) begin
                        error_stage <= 1; error_code <= {4'b0,tp_response_error}; done <= 1; phase <= FAULT;
                    end else phase <= AUDIO_START;
                end
                AUDIO_START: if (audio_run_ready) phase <= AUDIO_WAIT;
                AUDIO_WAIT: if (audio_done) begin
                    if (audio_error != 0) begin
                        error_stage <= audio_error_stage == 1 ? 2'd2 : 2'd3;
                        error_code <= audio_error; done <= 1; phase <= FAULT;
                    end else phase <= FINISH_SEND;
                end
                FINISH_SEND: if (tp_command_ready) phase <= FINISH_WAIT;
                FINISH_WAIT: if (tp_response_valid) begin
                    error_stage <= tp_response_error == 0 ? 2'd0 : 2'd1;
                    error_code <= {4'b0,tp_response_error}; done <= 1;
                    phase <= tp_response_error == 0 ? IDLE : FAULT;
                end
                FAULT: if (fault_report_pending && !audio_busy) begin done <= 1; fault_report_pending <= 0; end
                default: phase <= FAULT;
            endcase
        end
    end
endmodule
