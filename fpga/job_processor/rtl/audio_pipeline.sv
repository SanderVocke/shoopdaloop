`include "state_processor_defs.svh"

module audio_pipeline #(
    parameter integer PROGRAM_DEPTH = 32,
    parameter integer STATE_DEPTH = 16,
    parameter integer JOB_DEPTH = 32,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer ENABLE_MEDIA = 0
) (
    input wire clk,
    input wire reset,
    input wire run_valid,
    output wire run_ready,
    input wire [31:0] run_instruction_count,
    input wire [31:0] run_state_count,
    input wire [31:0] run_quantum,
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
    output wire loop_read_valid,
    input wire loop_read_ready,
    output wire [31:0] loop_read_slot,
    input wire loop_response_valid,
    input wire loop_response_error,
    input wire [162:0] loop_response_data,
    input wire run_external_jobs,
    output wire external_job_valid,
    input wire external_job_ready,
    input wire external_done_valid,
    output wire external_done_ready,
    input wire [7:0] external_error,
    output wire media_cmd_valid, input wire media_cmd_ready,
    output wire [2:0] media_cmd_opcode, output wire [63:0] media_cmd_view,
    output wire [63:0] media_cmd_start, output wire [63:0] media_cmd_length,
    output wire [31:0] media_cmd_count,
    input wire media_done, input wire [7:0] media_error,
    output wire media_write_valid, input wire media_write_ready, output wire [31:0] media_write_data,
    input wire media_read_valid, output wire media_read_ready, input wire [31:0] media_read_data
);
    typedef enum logic [2:0] {IDLE, STATE_WAIT, JOB_START, JOB_WAIT, EXTERNAL_WAIT} phase_t;
    phase_t phase;
    wire state_run_ready, job_run_ready, state_cfg_ready, job_cfg_ready;
    wire [7:0] state_error, job_error;
    wire job_accept;
    wire sample_busy, sample_done;
    reg external_jobs;
    reg [31:0] quantum, buffers, inputs, outputs;
    wire unused_pc;
    wire [31:0] state_pc;
    assign unused_pc = &{1'b0, state_pc};
    assign busy = phase != IDLE;
    assign run_ready = (phase == IDLE) && state_run_ready && job_run_ready && !reset;
    assign cfg_ready = (phase == IDLE) && state_cfg_ready;
    assign audio_cfg_ready = (phase == IDLE) && job_cfg_ready;
    assign monitor_ready = job_accept || ((phase == IDLE) && drain_ready && !reset)
        || ((phase == EXTERNAL_WAIT) && external_job_ready && !reset);
    assign drain_valid = (phase == IDLE) && monitor_valid && !reset;
    assign external_job_valid = phase == EXTERNAL_WAIT && monitor_valid && !reset;
    assign external_done_ready = phase == EXTERNAL_WAIT && queued_jobs == 0 && !reset;
    assign jobs_busy = sample_busy || phase == EXTERNAL_WAIT;
    assign jobs_done = sample_done || (external_done_valid && external_done_ready);

    state_processor #(
        .PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH), .JOB_DEPTH(JOB_DEPTH)
    ) state_engine (
        .clk(clk), .reset(reset), .run_valid(run_valid && run_ready), .run_ready(state_run_ready),
        .run_instruction_count(run_instruction_count), .run_state_count(run_state_count),
        .run_quantum(run_quantum), .run_job_budget(run_job_budget),
        .busy(state_busy), .done(state_done), .error_code(state_error), .instruction_pc(state_pc),
        .cfg_valid(cfg_valid && (phase == IDLE)), .cfg_ready(state_cfg_ready),
        .cfg_state(cfg_state), .cfg_write(cfg_write), .cfg_address(cfg_address), .cfg_write_data(cfg_write_data),
        .cfg_response_valid(cfg_response_valid), .cfg_response_error(cfg_response_error), .cfg_read_data(cfg_read_data),
        .job_valid(monitor_valid), .job_ready(monitor_ready), .job_data(monitor_data), .queued_jobs(queued_jobs),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data)
    );
    job_processor #(
        .MAX_FRAMES(MAX_FRAMES), .BUFFERS(BUFFERS), .INPUT_PORTS(INPUT_PORTS), .OUTPUT_PORTS(OUTPUT_PORTS),
        .ENABLE_MEDIA(ENABLE_MEDIA)
    ) sample_engine (
        .clk(clk), .reset(reset), .run_valid(phase == JOB_START), .run_ready(job_run_ready),
        .run_quantum(quantum), .run_buffers(buffers), .run_inputs(inputs), .run_outputs(outputs),
        .run_jobs(queued_jobs), .busy(sample_busy), .done(sample_done), .error_code(job_error), .buffer_ready(buffer_ready),
        .job_valid(monitor_valid), .job_ready(job_accept), .job_enable(job_enable), .job_data(monitor_data),
        .cfg_valid(audio_cfg_valid && (phase == IDLE)), .cfg_ready(job_cfg_ready),
        .cfg_space(audio_cfg_space), .cfg_write(audio_cfg_write), .cfg_address(audio_cfg_address),
        .cfg_write_data(audio_cfg_write_data), .cfg_response_valid(audio_cfg_response_valid),
        .cfg_response_error(audio_cfg_response_error), .cfg_read_data(audio_cfg_read_data),
        .media_cmd_valid(media_cmd_valid), .media_cmd_ready(media_cmd_ready), .media_cmd_opcode(media_cmd_opcode),
        .media_cmd_view(media_cmd_view), .media_cmd_start(media_cmd_start), .media_cmd_length(media_cmd_length),
        .media_cmd_count(media_cmd_count), .media_done(media_done), .media_error(media_error),
        .media_write_valid(media_write_valid), .media_write_ready(media_write_ready), .media_write_data(media_write_data),
        .media_read_valid(media_read_valid), .media_read_ready(media_read_ready), .media_read_data(media_read_data)
    );

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            done <= 0;
            error_code <= 0;
            error_stage <= 0;
            quantum <= 0;
            buffers <= 0;
            inputs <= 0;
            outputs <= 0;
            external_jobs <= 0;
        end else begin
            done <= 0;
            case (phase)
                IDLE: if (run_valid && run_ready) begin
                    quantum <= run_quantum;
                    buffers <= run_buffers;
                    inputs <= run_inputs;
                    outputs <= run_outputs;
                    external_jobs <= run_external_jobs;
                    error_code <= 0;
                    error_stage <= 0;
                    phase <= STATE_WAIT;
                end
                STATE_WAIT: if (state_done) begin
                    if (state_error != 0) begin
                        error_code <= state_error;
                        error_stage <= 1;
                        done <= 1;
                        phase <= IDLE;
                    end else
                        phase <= external_jobs ? EXTERNAL_WAIT : JOB_START;
                end
                JOB_START: if (job_run_ready) phase <= JOB_WAIT;
                JOB_WAIT: if (jobs_done) begin
                    error_code <= job_error;
                    error_stage <= 2;
                    done <= 1;
                    phase <= IDLE;
                end
                EXTERNAL_WAIT: if (external_done_valid && external_done_ready) begin
                    error_code <= external_error;
                    error_stage <= 2;
                    done <= 1;
                    phase <= IDLE;
                end
                default: phase <= IDLE;
            endcase
        end
    end
endmodule
