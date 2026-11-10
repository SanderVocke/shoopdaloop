`include "state_processor_defs.svh"

module audio_pipeline #(
    parameter integer PROGRAM_DEPTH = 32,
    parameter integer STATE_DEPTH = 16,
    parameter integer JOB_DEPTH = 32,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2
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
    input wire drain_ready
);
    typedef enum logic [1:0] {IDLE, STATE_WAIT, JOB_START, JOB_WAIT} phase_t;
    phase_t phase;
    wire state_run_ready, job_run_ready, state_cfg_ready, job_cfg_ready;
    wire [7:0] state_error, job_error;
    wire job_accept;
    reg [31:0] quantum, buffers, inputs, outputs;
    wire unused_pc;
    wire [31:0] state_pc;
    assign unused_pc = &{1'b0, state_pc};
    assign busy = phase != IDLE;
    assign run_ready = (phase == IDLE) && state_run_ready && job_run_ready && !reset;
    assign cfg_ready = (phase == IDLE) && state_cfg_ready;
    assign audio_cfg_ready = (phase == IDLE) && job_cfg_ready;
    assign monitor_ready = job_accept || ((phase == IDLE) && drain_ready && !reset);
    assign drain_valid = (phase == IDLE) && monitor_valid && !reset;

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
        .job_valid(monitor_valid), .job_ready(monitor_ready), .job_data(monitor_data), .queued_jobs(queued_jobs)
    );
    job_processor #(
        .MAX_FRAMES(MAX_FRAMES), .BUFFERS(BUFFERS), .INPUT_PORTS(INPUT_PORTS), .OUTPUT_PORTS(OUTPUT_PORTS)
    ) sample_engine (
        .clk(clk), .reset(reset), .run_valid(phase == JOB_START), .run_ready(job_run_ready),
        .run_quantum(quantum), .run_buffers(buffers), .run_inputs(inputs), .run_outputs(outputs),
        .run_jobs(queued_jobs), .busy(jobs_busy), .done(jobs_done), .error_code(job_error), .buffer_ready(buffer_ready),
        .job_valid(monitor_valid), .job_ready(job_accept), .job_enable(job_enable), .job_data(monitor_data),
        .cfg_valid(audio_cfg_valid && (phase == IDLE)), .cfg_ready(job_cfg_ready),
        .cfg_space(audio_cfg_space), .cfg_write(audio_cfg_write), .cfg_address(audio_cfg_address),
        .cfg_write_data(audio_cfg_write_data), .cfg_response_valid(audio_cfg_response_valid),
        .cfg_response_error(audio_cfg_response_error), .cfg_read_data(audio_cfg_read_data)
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
        end else begin
            done <= 0;
            case (phase)
                IDLE: if (run_valid && run_ready) begin
                    quantum <= run_quantum;
                    buffers <= run_buffers;
                    inputs <= run_inputs;
                    outputs <= run_outputs;
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
                        phase <= JOB_START;
                end
                JOB_START: if (job_run_ready) phase <= JOB_WAIT;
                JOB_WAIT: if (jobs_done) begin
                    error_code <= job_error;
                    error_stage <= 2;
                    done <= 1;
                    phase <= IDLE;
                end
                default: phase <= IDLE;
            endcase
        end
    end
endmodule
