`include "state_processor_defs.svh"

module audio_pipeline #(
    parameter integer PROGRAM_DEPTH = 32,
    parameter integer STATE_DEPTH = 16,
    parameter integer JOB_DEPTH = 32,
    parameter integer EXTERNAL_IO = 0,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer CHUNK_FRAMES = 1,
    parameter integer ENABLE_MEDIA = 0
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
    input wire [163:0] loop_response_data,
    output wire media_cmd_valid, input wire media_cmd_ready,
    output wire [3:0] media_cmd_opcode, output wire [63:0] media_cmd_view,
    output wire [63:0] media_cmd_start, output wire [63:0] media_cmd_length,
    output wire [31:0] media_cmd_count,
    input wire media_done, input wire [7:0] media_error, input wire media_fault,
    output wire media_write_valid, input wire media_write_ready, output wire [31:0] media_write_data,
    input wire media_read_valid, output wire media_read_ready, input wire [31:0] media_read_data,
    output wire media_hint_valid, input wire media_hint_ready,
    output wire [63:0] media_hint_view, output wire [63:0] media_hint_position, output wire [63:0] media_hint_length,
    output wire [31:0] media_hint_count
);
    typedef enum logic [2:0] {IDLE, STATE_WAIT, JOB_START, JOB_WAIT} phase_t;
    reg [5:0] chunk_shift;
    function automatic [5:0] shift_for(input [31:0] frames);
        begin
            shift_for = 0;
            for (integer bit_index=0; bit_index<32; bit_index=bit_index+1)
                if (frames[bit_index]) shift_for = 6'(bit_index);
        end
    endfunction
    reg [31:0] chunk_frames;
    phase_t phase;
    wire state_run_ready, job_run_ready, state_cfg_ready, job_cfg_ready;
    wire [7:0] state_error, job_error;
    wire job_accept;
    wire sample_busy, sample_done;
    reg [31:0] quantum, buffers, inputs, outputs;
    wire unused_pc;
    wire [31:0] state_pc;
    wire read_intent_valid, late_hint_valid, late_hint_ready, hint_push_ready, hint_pop_valid;
    wire [`SHOOP_SP_JOB_BITS-1:0] read_intent_data, hint_pop_data;
    wire [63:0] late_hint_view, late_hint_position, late_hint_length;
    wire [$clog2(JOB_DEPTH+1)-1:0] hint_count;
    wire unused_hint_count = &{1'b0,hint_count,hint_pop_data[63:1],hint_push_data[63:0]};
    wire [`SHOOP_SP_JOB_BITS-1:0] late_hint_data = {late_hint_length,late_hint_position,late_hint_view,56'b0,`SHOOP_SP_JOB_READ};
    wire [`SHOOP_SP_JOB_BITS-1:0] hint_push_data = read_intent_valid ? read_intent_data : late_hint_data;
    wire hint_push_in_range = hint_push_data[191:128] <= (64'hffffffffffffffff >> chunk_shift)
        && hint_push_data[255:192] <= (64'hffffffffffffffff >> chunk_shift);
    wire [`SHOOP_SP_JOB_BITS-1:0] translated_hint = {hint_push_data[255:192] << chunk_shift,
        hint_push_data[191:128] << chunk_shift,hint_push_data[127:64],63'b0,hint_push_in_range};
    wire hint_in_range = hint_pop_data[0];
    assign late_hint_ready = phase != JOB_WAIT || (hint_push_ready && !read_intent_valid);
    assign media_hint_valid = ENABLE_MEDIA != 0 && hint_pop_valid && hint_in_range && busy && !reset;
    assign media_hint_view = hint_pop_data[127:64];
    assign media_hint_position = hint_pop_data[191:128];
    assign media_hint_length = hint_pop_data[255:192];
    assign media_hint_count = quantum;

    sp_job_fifo #(.DEPTH(JOB_DEPTH), .COUNT_BITS($clog2(JOB_DEPTH+1))) read_intents (
        .clk(clk), .reset(reset || (run_valid && run_ready)),
        .push_valid(ENABLE_MEDIA != 0 && (read_intent_valid || (late_hint_valid && phase == JOB_WAIT))), .push_ready(hint_push_ready),
        .push_data(translated_hint),
        .pop_valid(hint_pop_valid), .pop_ready((media_hint_ready || !hint_in_range) && busy && !reset),
        .pop_data(hint_pop_data), .count(hint_count)
    );
    assign unused_pc = &{1'b0, state_pc};
    assign busy = phase != IDLE;
    assign run_ready = (phase == IDLE) && state_run_ready && job_run_ready && !reset && ((ENABLE_MEDIA == 0) || !media_fault);
    assign cfg_ready = (phase == IDLE) && state_cfg_ready;
    assign audio_cfg_ready = (phase == IDLE) && job_cfg_ready;
    assign monitor_ready = job_accept || ((phase == IDLE) && drain_ready && !reset);
    assign drain_valid = (phase == IDLE) && monitor_valid && !reset;
    assign jobs_busy = sample_busy;
    assign jobs_done = sample_done;

    state_processor #(
        .PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH), .JOB_DEPTH(JOB_DEPTH)
    ) state_engine (
        .clk(clk), .reset(reset), .run_valid(run_valid && run_ready), .run_ready(state_run_ready),
        .run_instruction_count(run_instruction_count), .run_state_count(run_state_count),
        .run_quantum(run_quantum), .run_chunk_frames(run_chunk_frames), .run_job_budget(run_job_budget),
        .busy(state_busy), .done(state_done), .error_code(state_error), .instruction_pc(state_pc),
        .cfg_valid(cfg_valid && (phase == IDLE)), .cfg_ready(state_cfg_ready),
        .cfg_state(cfg_state), .cfg_write(cfg_write), .cfg_address(cfg_address), .cfg_write_data(cfg_write_data),
        .cfg_response_valid(cfg_response_valid), .cfg_response_error(cfg_response_error), .cfg_read_data(cfg_read_data),
        .job_valid(monitor_valid), .job_ready(monitor_ready), .job_data(monitor_data), .queued_jobs(queued_jobs),
        .read_intent_valid(read_intent_valid), .read_intent_data(read_intent_data),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data)
    );
    job_processor #(.EXTERNAL_IO(EXTERNAL_IO),
        .MAX_FRAMES(MAX_FRAMES), .BUFFERS(BUFFERS), .INPUT_PORTS(INPUT_PORTS), .OUTPUT_PORTS(OUTPUT_PORTS),
        .CHUNK_FRAMES(CHUNK_FRAMES), .ENABLE_MEDIA(ENABLE_MEDIA)
    ) sample_engine (
        .sample_input_read(sample_input_read), .sample_input_address(sample_input_address), .sample_input_data(sample_input_data),
        .sample_output_write(sample_output_write), .sample_output_address(sample_output_address), .sample_output_data(sample_output_data),
        .clk(clk), .reset(reset), .run_valid(phase == JOB_START), .run_ready(job_run_ready),
        .run_quantum(quantum), .run_chunk_frames(chunk_frames), .run_buffers(buffers), .run_inputs(inputs), .run_outputs(outputs),
        .run_jobs(queued_jobs), .busy(sample_busy), .done(sample_done), .error_code(job_error), .buffer_ready(buffer_ready),
        .job_valid(monitor_valid), .job_ready(job_accept), .job_enable(job_enable), .job_data(monitor_data),
        .cfg_valid(audio_cfg_valid && (phase == IDLE)), .cfg_ready(job_cfg_ready),
        .cfg_space(audio_cfg_space), .cfg_write(audio_cfg_write), .cfg_address(audio_cfg_address),
        .cfg_write_data(audio_cfg_write_data), .cfg_response_valid(audio_cfg_response_valid),
        .cfg_response_error(audio_cfg_response_error), .cfg_read_data(audio_cfg_read_data),
        .media_cmd_valid(media_cmd_valid), .media_cmd_ready(media_cmd_ready), .media_cmd_opcode(media_cmd_opcode),
        .media_cmd_view(media_cmd_view), .media_cmd_start(media_cmd_start), .media_cmd_length(media_cmd_length),
        .media_cmd_count(media_cmd_count), .media_done(media_done), .media_error(media_error), .media_fault(media_fault),
        .media_write_valid(media_write_valid), .media_write_ready(media_write_ready), .media_write_data(media_write_data),
        .media_read_valid(media_read_valid), .media_read_ready(media_read_ready), .media_read_data(media_read_data),
        .media_hint_valid(late_hint_valid), .media_hint_ready(late_hint_ready), .media_hint_view(late_hint_view),
        .media_hint_position(late_hint_position), .media_hint_length(late_hint_length)
    );

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            done <= 0;
            error_code <= 0;
            error_stage <= 0;
            quantum <= 0; chunk_frames <= 0; chunk_shift <= 0;
            buffers <= 0;
            inputs <= 0;
            outputs <= 0;
        end else begin
            done <= 0;
            if ((ENABLE_MEDIA != 0) && media_fault && phase != IDLE) begin
                error_code <= 8; error_stage <= 2; done <= 1; phase <= IDLE;
            end else case (phase)
                IDLE: if (run_valid && run_ready) begin
                    quantum <= run_quantum; chunk_frames <= run_chunk_frames; chunk_shift <= shift_for(run_chunk_frames);
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
