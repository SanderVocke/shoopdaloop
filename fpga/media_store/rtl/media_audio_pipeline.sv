`include "state_processor_defs.svh"

module media_audio_pipeline #(
    parameter integer EXTERNAL_IO = 0,
    parameter integer PROGRAM_DEPTH = 32, parameter integer STATE_DEPTH = 16,
    parameter integer JOB_DEPTH = 32, parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16, parameter integer INPUT_PORTS = 2, parameter integer OUTPUT_PORTS = 2,
    parameter integer REUSE_VIEWS = 0,
    parameter integer VIEWS = 32, parameter integer SLOTS = 1024, parameter integer CHUNK_FRAMES = 32,
    parameter integer MEMORY_RESPONSE_CYCLES = 1,
    parameter integer BLOCK_CHUNKS = 32,
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
    input wire clk, input wire reset,
    input wire run_valid, output wire run_ready,
    input wire [31:0] run_instruction_count, input wire [31:0] run_state_count,
    input wire [31:0] run_quantum,
    input wire [31:0] run_chunk_frames, input wire [31:0] run_job_budget,
    input wire [31:0] run_buffers, input wire [31:0] run_inputs, input wire [31:0] run_outputs,
    input wire job_enable, output wire busy, output wire done,
    output wire [7:0] error_code, output wire [1:0] error_stage,
    output wire state_busy, output wire state_done, output wire jobs_busy, output wire jobs_done,
    output wire [BUFFERS-1:0] buffer_ready,
    input wire cfg_valid, output wire cfg_ready, input wire cfg_state, input wire cfg_write,
    input wire [15:0] cfg_address, input wire [`SHOOP_SP_STATE_BITS-1:0] cfg_write_data,
    output wire cfg_response_valid, output wire cfg_response_error,
    output wire [`SHOOP_SP_STATE_BITS-1:0] cfg_read_data,
    input wire audio_cfg_valid, output wire audio_cfg_ready, input wire [1:0] audio_cfg_space,
    input wire audio_cfg_write, input wire [31:0] audio_cfg_address, input wire [31:0] audio_cfg_write_data,
    output wire audio_cfg_response_valid, output wire audio_cfg_response_error, output wire [31:0] audio_cfg_read_data,
    output wire monitor_valid, output wire monitor_ready, output wire [`SHOOP_SP_JOB_BITS-1:0] monitor_data,
    output wire [31:0] queued_jobs, output wire drain_valid, input wire drain_ready,
    output wire loop_read_valid, input wire loop_read_ready, output wire [31:0] loop_read_slot,
    input wire loop_response_valid, input wire loop_response_error, input wire [163:0] loop_response_data,
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
    wire pipeline_busy, pipeline_ready, state_cfg_ready, sample_cfg_ready;
    wire pipeline_done;
    reg completion_pending;
    wire job_cmd_valid, job_cmd_ready, job_write_valid, job_write_ready, job_read_valid, job_read_ready;
    wire [3:0] job_opcode;
    wire [63:0] job_view, job_start, job_length;
    wire [31:0] job_count, job_write_data, store_sample;
    wire internal_ready, internal_done, internal_cfg_ready, internal_write_ready, internal_read_valid;
    wire store_memory_valid, store_memory_ready, store_memory_write;
    wire [31:0] store_memory_count;
    wire store_memory_write_valid, store_memory_write_ready, store_memory_read_valid, store_memory_read_ready;
    wire [63:0] store_memory_address;
    wire [31:0] store_memory_write_data, store_memory_read_data;
    wire store_memory_response_valid, store_memory_response_ready, store_memory_response_error;
    wire hint_valid, hint_ready;
    wire [63:0] hint_view, hint_position, hint_length;
    wire [31:0] hint_count;
    assign busy = pipeline_busy || store_busy;
    assign done = (pipeline_done || completion_pending) && !store_busy;
    always @(posedge clk) begin
        if (reset) completion_pending <= 0;
        else completion_pending <= (pipeline_done || completion_pending) && !done;
    end
    assign run_ready = pipeline_ready && internal_ready && !store_cmd_valid && !store_cfg_valid;
    assign cfg_ready = state_cfg_ready && !store_busy;
    assign audio_cfg_ready = sample_cfg_ready && !store_busy;
    assign store_cmd_ready = internal_ready && !pipeline_busy;
    assign job_cmd_ready = internal_ready && pipeline_busy;
    assign store_cfg_ready = internal_cfg_ready && !pipeline_busy;
    assign store_done = internal_done && !pipeline_busy;
    assign store_write_ready = internal_write_ready && !pipeline_busy;
    assign job_write_ready = internal_write_ready && pipeline_busy;
    assign store_read_valid = internal_read_valid && !pipeline_busy;
    assign job_read_valid = internal_read_valid && pipeline_busy;
    assign store_read_data = store_sample;

    audio_pipeline #(.EXTERNAL_IO(EXTERNAL_IO),
        .PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH), .JOB_DEPTH(JOB_DEPTH),
        .MAX_FRAMES(MAX_FRAMES), .BUFFERS(BUFFERS), .INPUT_PORTS(INPUT_PORTS), .OUTPUT_PORTS(OUTPUT_PORTS),
        .CHUNK_FRAMES(CHUNK_FRAMES), .ENABLE_MEDIA(1)
    ) pipeline (
        .sample_input_read(sample_input_read), .sample_input_address(sample_input_address), .sample_input_data(sample_input_data),
        .sample_output_write(sample_output_write), .sample_output_address(sample_output_address), .sample_output_data(sample_output_data),
        .clk(clk), .reset(reset), .run_valid(run_valid && run_ready), .run_ready(pipeline_ready),
        .run_instruction_count(run_instruction_count), .run_state_count(run_state_count),
        .run_quantum(run_quantum), .run_chunk_frames(run_chunk_frames), .run_job_budget(run_job_budget), .run_buffers(run_buffers),
        .run_inputs(run_inputs), .run_outputs(run_outputs), .job_enable(job_enable),
        .busy(pipeline_busy), .done(pipeline_done), .error_code(error_code), .error_stage(error_stage),
        .state_busy(state_busy), .state_done(state_done), .jobs_busy(jobs_busy), .jobs_done(jobs_done), .buffer_ready(buffer_ready),
        .cfg_valid(cfg_valid && !store_busy), .cfg_ready(state_cfg_ready), .cfg_state(cfg_state), .cfg_write(cfg_write),
        .cfg_address(cfg_address), .cfg_write_data(cfg_write_data), .cfg_response_valid(cfg_response_valid),
        .cfg_response_error(cfg_response_error), .cfg_read_data(cfg_read_data),
        .audio_cfg_valid(audio_cfg_valid && !store_busy), .audio_cfg_ready(sample_cfg_ready),
        .audio_cfg_space(audio_cfg_space), .audio_cfg_write(audio_cfg_write), .audio_cfg_address(audio_cfg_address),
        .audio_cfg_write_data(audio_cfg_write_data), .audio_cfg_response_valid(audio_cfg_response_valid),
        .audio_cfg_response_error(audio_cfg_response_error), .audio_cfg_read_data(audio_cfg_read_data),
        .monitor_valid(monitor_valid), .monitor_ready(monitor_ready), .monitor_data(monitor_data),
        .queued_jobs(queued_jobs), .drain_valid(drain_valid), .drain_ready(drain_ready),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data),
        .media_cmd_valid(job_cmd_valid), .media_cmd_ready(job_cmd_ready), .media_cmd_opcode(job_opcode),
        .media_cmd_view(job_view), .media_cmd_start(job_start), .media_cmd_length(job_length), .media_cmd_count(job_count),
        .media_done(internal_done), .media_error(store_error_code), .media_fault(memory_fault),
        .media_write_valid(job_write_valid), .media_write_ready(job_write_ready), .media_write_data(job_write_data),
        .media_read_valid(job_read_valid), .media_read_ready(job_read_ready), .media_read_data(store_sample),
        .media_hint_valid(hint_valid), .media_hint_ready(hint_ready), .media_hint_view(hint_view),
        .media_hint_position(hint_position), .media_hint_length(hint_length), .media_hint_count(hint_count)
    );

    media_store #(.REUSE_VIEWS(REUSE_VIEWS), .VIEWS(VIEWS), .SLOTS(SLOTS), .CHUNK_FRAMES(CHUNK_FRAMES), .READ_BUFFERS(READ_BUFFERS), .WRITE_BUFFERS(WRITE_BUFFERS), .BURST_MEMORY(BURST_MEMORY), .BLOCK_CHUNKS(BLOCK_CHUNKS),.TABLE_PAGES(TABLE_PAGES), .TABLE_LINES(TABLE_LINES), .REF_LINES(REF_LINES), .STACK_LINES(STACK_LINES)) media (
        .clk(clk), .reset(reset), .arena_base(arena_base),
        .hint_valid(hint_valid), .hint_ready(hint_ready), .hint_view(hint_view), .hint_position(hint_position), .hint_length(hint_length), .hint_count(hint_count),
        .cmd_valid(pipeline_busy ? job_cmd_valid : store_cmd_valid), .cmd_ready(internal_ready),
        .cmd_opcode(pipeline_busy ? job_opcode : store_cmd_opcode), .cmd_view(pipeline_busy ? job_view : store_cmd_view),
        .cmd_start(pipeline_busy ? job_start : store_cmd_start), .cmd_length(pipeline_busy ? job_length : store_cmd_length),
        .cmd_count(pipeline_busy ? job_count : store_cmd_count),
        .busy(store_busy), .done(internal_done), .error_code(store_error_code), .memory_meta_valid(memory_meta_valid), .memory_meta_ready(memory_meta_ready), .memory_meta_write(memory_meta_write), .memory_meta_address(memory_meta_address), .memory_meta_write_data(memory_meta_write_data), .memory_meta_count(memory_meta_count), .memory_meta_write_valid(memory_meta_write_valid), .memory_meta_write_ready(memory_meta_write_ready), .memory_meta_read_valid(memory_meta_read_valid), .memory_meta_read_ready(memory_meta_read_ready), .memory_meta_response_valid(memory_meta_response_valid), .memory_meta_response_ready(memory_meta_response_ready), .memory_meta_response_error(memory_meta_response_error), .memory_meta_read_data(memory_meta_read_data), .memory_fault(memory_fault),
        .result_view(store_result_view), .result_empty(store_result_empty),
        .write_valid(pipeline_busy ? job_write_valid : store_write_valid), .write_ready(internal_write_ready),
        .write_data(pipeline_busy ? job_write_data : store_write_data),
        .read_valid(internal_read_valid), .read_ready(pipeline_busy ? job_read_ready : store_read_ready), .read_data(store_sample),
        .cfg_valid(store_cfg_valid && !pipeline_busy), .cfg_ready(internal_cfg_ready), .cfg_write(store_cfg_write),
        .cfg_space(store_cfg_space), .cfg_address(store_cfg_address), .cfg_write_data(store_cfg_write_data),
        .cfg_response_valid(store_cfg_response_valid), .cfg_response_error(store_cfg_response_error), .cfg_read_data(store_cfg_read_data),
        .memory_valid(store_memory_valid), .memory_ready(store_memory_ready), .memory_write(store_memory_write),
        .memory_address(store_memory_address), .memory_write_data(store_memory_write_data), .memory_arena_base(memory_arena_base),
        .memory_count(store_memory_count), .memory_write_valid(store_memory_write_valid),
        .memory_write_ready(store_memory_write_ready), .memory_read_valid(store_memory_read_valid),
        .memory_read_ready(store_memory_read_ready),
        .memory_response_valid(store_memory_response_valid), .memory_response_ready(store_memory_response_ready),
        .memory_response_error(store_memory_response_error), .memory_read_data(store_memory_read_data)
    );
    memory_delay #(.RESPONSE_CYCLES(MEMORY_RESPONSE_CYCLES)) delay (
        .clk(clk), .reset(reset),
        .request_valid(store_memory_valid), .request_ready(store_memory_ready),
        .request_count(store_memory_count),
        .request_write_valid(store_memory_write_valid), .request_write_ready(store_memory_write_ready),
        .response_read_valid(store_memory_read_valid), .response_read_ready(store_memory_read_ready),
        .request_write(store_memory_write), .request_address(store_memory_address), .request_write_data(store_memory_write_data),
        .response_valid(store_memory_response_valid), .response_ready(store_memory_response_ready),
        .response_error(store_memory_response_error), .response_read_data(store_memory_read_data),
        .memory_valid(memory_valid), .memory_ready(memory_ready), .memory_write(memory_write),
        .memory_address(memory_address), .memory_write_data(memory_write_data),
        .memory_count(memory_count), .memory_write_valid(memory_write_valid), .memory_write_ready(memory_write_ready),
        .memory_read_valid(memory_read_valid), .memory_read_ready(memory_read_ready),
        .memory_response_valid(memory_response_valid), .memory_response_ready(memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data)
    );
endmodule
