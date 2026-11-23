`include "state_processor_defs.svh"

module audio_pipeline #(
    parameter integer PROGRAM_DEPTH = 32,
    parameter integer LIVE_LOADING = 0,
    parameter integer PROGRAM_REGIONS = LIVE_LOADING != 0 ? 2 : 1,
    parameter integer STATE_DEPTH = 16,
    parameter integer JOB_DEPTH = 32,
    parameter integer READ_HINT_DEPTH = JOB_DEPTH,
    parameter integer LOOP_DEPTH = STATE_DEPTH,
    parameter integer EXTERNAL_IO = 0,
    parameter integer ENABLE_MIDI = 0,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer CHUNK_FRAMES = 1,
    parameter integer ENABLE_MEDIA = 0,
    parameter integer DDR_PROGRAM = 0,
    parameter integer JOB_FIFO_DEPTH = JOB_DEPTH > 512 ? 512 : JOB_DEPTH,
    parameter integer SPLIT_MIDI_JOBS = 0,
    parameter integer MIDI_JOB_FIFO_DEPTH = 128
) (
    output wire midi_job_valid, input wire midi_job_ready,
    output wire [255:0] midi_job_data,
    input wire midi_job_done, input wire [3:0] midi_job_error,
    output wire [3:0] midi_error_code,
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
    input wire [31:0] run_sync_count,
    input wire run_program_region,
    input wire switch_valid, output wire switch_ready,
    input wire [63:0] switch_expected, switch_revision,
    input wire switch_region,
    input wire [15:0] switch_instructions, switch_sync_count, switch_states, switch_buffers,
    input wire [43:0] switch_future,
    output reg switch_response_valid, input wire switch_response_ready,
    output reg switch_response_error,
    output reg [63:0] switch_first_frame,
    output reg [63:0] active_revision,
    output wire handoff_request, input wire handoff_complete,
    input wire [31:0] run_state_count,
    input wire [31:0] run_quantum,
    input wire [31:0] run_chunk_frames,
    input wire [15:0] capture_credits,
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
    input wire cfg_loop,
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
    output wire [63:0] sync_frame,
    output wire sync_event_valid,
    input wire sync_event_ready,
    output wire [255:0] sync_event_data,
    output wire media_cmd_valid, input wire media_cmd_ready,
    output wire [3:0] media_cmd_opcode, output wire [63:0] media_cmd_view,
    output wire [63:0] media_cmd_start, output wire [63:0] media_cmd_length,
    output wire [31:0] media_cmd_count,
    input wire media_done, input wire [7:0] media_error, input wire media_fault,
    output wire media_write_valid, input wire media_write_ready, output wire [31:0] media_write_data,
    input wire media_read_valid, output wire media_read_ready, input wire [31:0] media_read_data,
    output wire media_hint_valid, input wire media_hint_ready,
    output wire [63:0] media_hint_view, output wire [63:0] media_hint_position, output wire [63:0] media_hint_length,
    output wire [31:0] media_hint_count,
    output wire memory_program_valid, input wire memory_program_ready,
    output wire memory_program_write,
    output wire [15:0] memory_program_address, output wire [31:0] memory_program_count,
    output wire [31:0] memory_program_write_data,
    output wire memory_program_write_valid, input wire memory_program_write_ready,
    input wire memory_program_read_valid, output wire memory_program_read_ready,
    input wire [31:0] memory_program_read_data,
    input wire memory_program_response_valid, output wire memory_program_response_ready,
    input wire memory_program_response_error
);
    typedef enum logic [2:0] {IDLE, STATE_START, STATE_WAIT, JOB_START, JOB_WAIT} phase_t;
    reg [5:0] chunk_shift;
    function automatic [5:0] shift_for(input [31:0] frames);
        begin
            shift_for = 0;
            for (integer bit_index=0; bit_index<32; bit_index=bit_index+1)
                if (frames[bit_index]) shift_for = 6'(bit_index);
        end
    endfunction
    reg [31:0] chunk_frames;
    (* fsm_encoding = "one_hot" *) phase_t phase;
    wire state_run_ready, job_run_ready, state_cfg_ready, job_cfg_ready;
    wire [7:0] state_error, job_error;
    wire job_accept;
    wire sample_busy, sample_done;
    localparam SPLIT_JOBS = ENABLE_MIDI != 0 && SPLIT_MIDI_JOBS != 0;
    wire sp_job_valid, sp_job_ready;
    wire [31:0] sp_queued_jobs, audio_queued_jobs, midi_queued_jobs;
    wire sample_job_valid, sample_midi_valid;
    wire [`SHOOP_SP_JOB_BITS-1:0] sample_job_data, sample_midi_data;
    wire [3:0] sample_midi_error;
    wire midi_lane_failed, midi_lane_complete, midi_lane_busy;
    reg state_finished, job_finished;
    wire state_complete = state_finished || state_done;
    wire jobs_complete = job_finished || sample_done;
    wire state_failed = state_complete && state_error != 0;
    wire jobs_failed = jobs_complete && job_error != 0;
    wire media_failed = ENABLE_MEDIA != 0 && media_fault;
    wire abort_state = phase != IDLE
        && (media_failed || (phase == JOB_WAIT && (jobs_failed || midi_lane_failed)));
    wire stop_jobs = state_failed || media_failed || jobs_failed || midi_lane_failed;
    wire stream_done = (SPLIT_JOBS ? stop_jobs : state_failed)
        || (state_complete && audio_queued_jobs == 0);
    reg [31:0] quantum, buffers, inputs, outputs;
    reg [31:0] state_instructions, state_sync_count, state_count;
    reg state_region;
    wire state_admission = run_valid && run_ready;
    wire unused_pc;
    wire [31:0] state_pc;
    reg active_region, handoff_pending, handoff_drained;
    reg [15:0] active_instructions, active_sync_count, active_states, active_buffers;
    reg future_pending;
    reg [43:0] future_switch;
    reg [63:0] future_revision;
    reg [15:0] future_instructions, future_sync_count;
    wire future_switch_ready;
    wire guarded_switch = switch_future[43:12] != 0;
    wire restart_preview = future_pending && (&future_switch[43:12]);
    wire bad_switch = switch_expected != active_revision || switch_revision <= active_revision
        || (active_revision != 0 && switch_region == active_region)
        || (guarded_switch && (active_revision == 0 || switch_states != active_states
            || switch_buffers != active_buffers
            || 32'(switch_future[11:0]) >= LOOP_DEPTH));
    wire [31:0] selected_instructions = LIVE_LOADING != 0 ? {16'b0,active_instructions} : run_instruction_count;
    wire [31:0] selected_sync_count = LIVE_LOADING != 0 ? {16'b0,active_sync_count} : run_sync_count;
    wire [31:0] selected_states = LIVE_LOADING != 0 ? {16'b0,active_states} : run_state_count;
    wire [31:0] selected_buffers = LIVE_LOADING != 0 ? {16'b0,active_buffers} : run_buffers;
    wire boundary_ready = (phase == IDLE) && state_run_ready && job_run_ready && !reset && ((ENABLE_MEDIA == 0) || !media_fault);
    assign switch_ready = LIVE_LOADING != 0 && run_valid && boundary_ready
        && !handoff_pending && !future_pending && !switch_response_valid;
    assign handoff_request = LIVE_LOADING != 0 && handoff_pending && handoff_drained;
    wire read_intent_valid, late_hint_valid, late_hint_ready, hint_push_ready, hint_pop_valid;
    wire [`SHOOP_SP_JOB_BITS-1:0] read_intent_data, hint_pop_data;
    wire [63:0] late_hint_view, late_hint_position, late_hint_length;
    wire [$clog2(READ_HINT_DEPTH+1)-1:0] hint_count;
    wire unused_hint_count = &{1'b0,hint_count,hint_pop_data[63:1],hint_push_data[63:0]};
    wire [`SHOOP_SP_JOB_BITS-1:0] late_hint_data = {late_hint_length,late_hint_position,late_hint_view,56'b0,`SHOOP_SP_JOB_READ};
    reg hint_stage_valid;
    reg [`SHOOP_SP_JOB_BITS-1:0] hint_push_data;
    wire hint_stage_ready = !hint_stage_valid || hint_push_ready;
    wire hint_stage_input = ENABLE_MEDIA != 0 && (read_intent_valid || (late_hint_valid && phase == JOB_WAIT));
    wire [63:0] hint_overflow_mask = ~(64'hffffffffffffffff >> chunk_shift);
    wire hint_push_in_range = ((hint_push_data[191:128] | hint_push_data[255:192]) & hint_overflow_mask) == 0;
    wire [`SHOOP_SP_JOB_BITS-1:0] translated_hint = {hint_push_data[255:192] << chunk_shift,
        hint_push_data[191:128] << chunk_shift,hint_push_data[127:64],63'b0,hint_push_in_range};
    wire hint_in_range = hint_pop_data[0];
    assign late_hint_ready = phase != JOB_WAIT || (hint_stage_ready && !read_intent_valid);
    assign media_hint_valid = ENABLE_MEDIA != 0 && hint_pop_valid && hint_in_range && !reset;
    assign media_hint_view = hint_pop_data[127:64];
    assign media_hint_position = hint_pop_data[191:128];
    assign media_hint_length = hint_pop_data[255:192];
    assign media_hint_count = quantum;

    sp_job_fifo #(.DEPTH(READ_HINT_DEPTH), .COUNT_BITS($clog2(READ_HINT_DEPTH+1))) read_intents (
        .clk(clk), .reset(reset),
        .push_valid(hint_stage_valid), .push_ready(hint_push_ready),
        .push_data(translated_hint),
        .pop_valid(hint_pop_valid), .pop_ready((media_hint_ready || !hint_in_range) && !reset),
        .pop_data(hint_pop_data), .count(hint_count)
    );
    always @(posedge clk) begin
        if (reset) begin
            hint_stage_valid <= 0;
            hint_push_data <= 0;
        end else if (hint_stage_ready) begin
            hint_stage_valid <= hint_stage_input;
            if (hint_stage_input) hint_push_data <= read_intent_valid ? read_intent_data : late_hint_data;
        end
    end
    assign unused_pc = &{1'b0, state_pc};
    assign busy = phase != IDLE;
    assign run_ready = boundary_ready && !(LIVE_LOADING != 0 && (handoff_pending || (switch_valid && !switch_response_valid)));
    assign cfg_ready = state_cfg_ready && (phase == IDLE || (PROGRAM_REGIONS == 2 && cfg_valid && cfg_write))
        && !(PROGRAM_REGIONS == 2 && state_admission);
    assign audio_cfg_ready = (phase == IDLE) && job_cfg_ready;
    assign monitor_valid = sp_job_valid && (!SPLIT_JOBS || sp_job_ready);
    assign monitor_ready = sp_job_ready;
    assign queued_jobs = audio_queued_jobs + midi_queued_jobs;
    assign jobs_busy = sample_busy || midi_lane_busy;
    assign jobs_done = SPLIT_JOBS
        ? (phase == JOB_WAIT && state_complete && jobs_complete && midi_lane_complete) : sample_done;

    generate if (SPLIT_JOBS) begin : split_jobs
        wire [7:0] kind = monitor_data[7:0];
        wire to_midi = kind >= `SHOOP_SP_MIDI_INPUT && kind <= `SHOOP_SP_MIDI_MERGE_OUTPUT;
        wire audio_push_ready, audio_valid, midi_push_ready, midi_valid;
        wire [`SHOOP_SP_JOB_BITS-1:0] audio_data, midi_data;
        wire [$clog2(JOB_FIFO_DEPTH+1)-1:0] audio_count;
        wire [$clog2(MIDI_JOB_FIFO_DEPTH+1)-1:0] midi_count;
        wire flush = phase != IDLE && stop_jobs;
        reg midi_active;
        reg [3:0] midi_error;
        wire midi_error_reply = midi_active && midi_job_done && midi_job_error != 0;

        assign sp_job_ready = phase != IDLE && !stop_jobs && (to_midi ? midi_push_ready : audio_push_ready);
        assign drain_valid = 0;
        assign audio_queued_jobs = 32'(audio_count);
        assign midi_queued_jobs = 32'(midi_count);
        assign sample_job_valid = audio_valid && !stop_jobs;
        assign sample_job_data = audio_data;
        assign midi_job_valid = phase == JOB_WAIT && job_enable && midi_valid && !midi_active && !stop_jobs;
        assign midi_job_data = midi_data;
        assign midi_error_code = midi_error;
        assign midi_lane_failed = midi_error != 0 || midi_error_reply;
        assign midi_lane_busy = midi_active;
        assign midi_lane_complete = !midi_active && (midi_count == 0 || stop_jobs);

        sp_job_fifo #(.DEPTH(JOB_FIFO_DEPTH)) audio_queue (
            .clk(clk), .reset(reset || flush),
            .push_valid(sp_job_valid && !to_midi && phase != IDLE && !stop_jobs),
            .push_ready(audio_push_ready), .push_data(monitor_data),
            .pop_valid(audio_valid), .pop_ready(job_accept && !stop_jobs), .pop_data(audio_data), .count(audio_count)
        );
        sp_job_fifo #(.DEPTH(MIDI_JOB_FIFO_DEPTH)) midi_queue (
            .clk(clk), .reset(reset || flush),
            .push_valid(sp_job_valid && to_midi && phase != IDLE && !stop_jobs),
            .push_ready(midi_push_ready), .push_data(monitor_data),
            .pop_valid(midi_valid), .pop_ready(midi_job_valid && midi_job_ready), .pop_data(midi_data), .count(midi_count)
        );

        always @(posedge clk) begin
            if (reset || state_admission) begin
                midi_active <= 0;
                midi_error <= 0;
            end else if (midi_job_valid && midi_job_ready) midi_active <= 1;
            else if (midi_active && midi_job_done) begin
                midi_active <= 0;
                midi_error <= midi_job_error;
            end
        end
    end else begin : serial_jobs
        assign sp_job_ready = job_accept || ((phase == IDLE) && drain_ready && !reset);
        assign drain_valid = (phase == IDLE) && monitor_valid && !reset;
        assign audio_queued_jobs = sp_queued_jobs;
        assign midi_queued_jobs = 0;
        assign sample_job_valid = monitor_valid;
        assign sample_job_data = monitor_data;
        assign midi_job_valid = sample_midi_valid;
        assign midi_job_data = sample_midi_data;
        assign midi_error_code = sample_midi_error;
        assign midi_lane_failed = 0;
        assign midi_lane_complete = 1;
        assign midi_lane_busy = 0;
    end endgenerate

    state_processor #(
        .PROGRAM_DEPTH(PROGRAM_DEPTH), .PROGRAM_REGIONS(PROGRAM_REGIONS), .STATE_DEPTH(STATE_DEPTH),
        .JOB_DEPTH(JOB_FIFO_DEPTH), .DDR_PROGRAM(DDR_PROGRAM), .STREAM_JOBS(1), .LOOP_DEPTH(LOOP_DEPTH),
        .EXTERNAL_JOB_FIFO(32'(SPLIT_JOBS))
    ) state_engine (
        .clk(clk), .reset(reset), .abort_run(abort_state),
        .run_valid(phase == STATE_START && !media_failed), .run_ready(state_run_ready),
        .run_instruction_count(state_instructions), .run_sync_count(state_sync_count), .run_program_region(state_region), .run_state_count(state_count),
        .run_quantum(quantum), .run_chunk_frames(chunk_frames),
        .capture_credits(capture_credits),
        .future_switch_valid(future_pending), .future_switch(future_switch), .future_switch_ready(future_switch_ready),
        .busy(state_busy), .done(state_done), .error_code(state_error), .instruction_pc(state_pc),
        .cfg_valid(cfg_valid && (phase == IDLE || (PROGRAM_REGIONS == 2 && cfg_write)) && !(PROGRAM_REGIONS == 2 && state_admission)), .cfg_ready(state_cfg_ready),
        .cfg_state(cfg_state), .cfg_loop(cfg_loop), .cfg_write(cfg_write), .cfg_address(cfg_address), .cfg_write_data(cfg_write_data),
        .cfg_response_valid(cfg_response_valid), .cfg_response_error(cfg_response_error), .cfg_read_data(cfg_read_data),
        .job_valid(sp_job_valid), .job_ready(sp_job_ready), .job_data(monitor_data), .queued_jobs(sp_queued_jobs),
        .read_intent_valid(read_intent_valid), .read_intent_data(read_intent_data),
        .sync_frame(sync_frame), .sync_event_valid(sync_event_valid), .sync_event_ready(sync_event_ready), .sync_event_data(sync_event_data),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data),
        .memory_program_valid(memory_program_valid),
        .memory_program_ready(memory_program_ready),
        .memory_program_write(memory_program_write),
        .memory_program_address(memory_program_address),
        .memory_program_count(memory_program_count),
        .memory_program_write_data(memory_program_write_data),
        .memory_program_write_valid(memory_program_write_valid),
        .memory_program_write_ready(memory_program_write_ready),
        .memory_program_read_valid(memory_program_read_valid),
        .memory_program_read_ready(memory_program_read_ready),
        .memory_program_read_data(memory_program_read_data),
        .memory_program_response_valid(memory_program_response_valid),
        .memory_program_response_ready(memory_program_response_ready),
        .memory_program_response_error(memory_program_response_error)
    );
    job_processor #(.EXTERNAL_IO(EXTERNAL_IO),
        .MAX_FRAMES(MAX_FRAMES), .BUFFERS(BUFFERS), .INPUT_PORTS(INPUT_PORTS), .OUTPUT_PORTS(OUTPUT_PORTS),
        .CHUNK_FRAMES(CHUNK_FRAMES), .ENABLE_MEDIA(ENABLE_MEDIA), .ENABLE_MIDI(SPLIT_JOBS ? 0 : ENABLE_MIDI), .STREAM_JOBS(1)
    ) sample_engine (
        .midi_job_valid(sample_midi_valid), .midi_job_ready(!SPLIT_JOBS && midi_job_ready), .midi_job_data(sample_midi_data),
        .midi_job_done(!SPLIT_JOBS && midi_job_done), .midi_job_error(SPLIT_JOBS ? 4'b0 : midi_job_error), .midi_error_code(sample_midi_error),
        .sample_input_read(sample_input_read), .sample_input_address(sample_input_address), .sample_input_data(sample_input_data),
        .sample_output_write(sample_output_write), .sample_output_address(sample_output_address), .sample_output_data(sample_output_data),
        .clk(clk), .reset(reset),
        .run_valid(phase == JOB_START && !state_failed && !media_failed), .run_ready(job_run_ready),
        .run_quantum(quantum), .run_chunk_frames(chunk_frames), .run_buffers(buffers), .run_inputs(inputs), .run_outputs(outputs),
        .run_jobs(32'd0), .job_stream_done(stream_done),
        .busy(sample_busy), .done(sample_done), .error_code(job_error), .buffer_ready(buffer_ready),
        .job_valid(sample_job_valid), .job_ready(job_accept), .job_enable(job_enable), .job_data(sample_job_data),
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
            active_revision <= 0; active_region <= 0; handoff_pending <= 0; handoff_drained <= 0;
            active_instructions <= 0; active_sync_count <= 0; active_states <= 0; active_buffers <= 0;
            switch_first_frame <= 0; switch_response_valid <= 0; switch_response_error <= 0;
            future_pending <= 0; future_switch <= 0; future_revision <= 0;
            future_instructions <= 0; future_sync_count <= 0;
        end else if (LIVE_LOADING != 0) begin
            if (handoff_pending && hint_count == 0 && !hint_stage_valid && queued_jobs == 0) handoff_drained <= 1;
            if (switch_response_valid && switch_response_ready) switch_response_valid <= 0;
            if (switch_valid && switch_ready) begin
                switch_first_frame <= sync_frame;
                switch_response_error <= bad_switch;
                if (bad_switch) switch_response_valid <= 1;
                else if (guarded_switch) begin
                    future_pending <= 1;
                    future_switch <= switch_future;
                    future_revision <= switch_revision;
                    future_instructions <= switch_instructions;
                    future_sync_count <= switch_sync_count;
                    switch_response_valid <= 1;
                end
                else begin
                    active_revision <= switch_revision; active_region <= switch_region;
                    active_instructions <= switch_instructions; active_sync_count <= switch_sync_count;
                    active_states <= switch_states; active_buffers <= switch_buffers;
                    handoff_pending <= 1; handoff_drained <= 0;
                end
            end
            if (future_pending && phase != IDLE) begin
                if (stop_jobs) future_pending <= 0;
                else if (phase == JOB_WAIT && state_complete && jobs_complete && midi_lane_complete) begin
                    future_pending <= 0;
                    if (future_switch_ready) begin
                        active_revision <= future_revision;
                        active_region <= !active_region;
                        active_instructions <= future_instructions;
                        active_sync_count <= future_sync_count;
                    end
                end
            end
            if (handoff_request && handoff_complete) begin
                handoff_pending <= 0; handoff_drained <= 0; switch_response_valid <= 1;
            end
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            done <= 0;
            error_code <= 0;
            error_stage <= 0;
            state_finished <= 0; job_finished <= 0;
            quantum <= 0; chunk_frames <= 0; chunk_shift <= 0;
            state_instructions <= 0; state_sync_count <= 0; state_count <= 0; state_region <= 0;
            buffers <= 0;
            inputs <= 0;
            outputs <= 0;
        end else begin
            done <= 0;
            if (state_done) state_finished <= 1;
            if (sample_done) job_finished <= 1;
            if (media_failed && phase != IDLE && phase != JOB_WAIT && !state_busy) begin
                error_code <= 8; error_stage <= 2; done <= 1; phase <= IDLE;
            end else case (phase)
                IDLE: if (run_valid && run_ready) begin
                    state_instructions <= restart_preview ? {16'b0,future_instructions} : selected_instructions;
                    state_sync_count <= restart_preview ? {16'b0,future_sync_count} : selected_sync_count;
                    state_count <= selected_states;
                    state_region <= LIVE_LOADING != 0 ? active_region ^ restart_preview : run_program_region;
                    quantum <= run_quantum; chunk_frames <= run_chunk_frames; chunk_shift <= shift_for(run_chunk_frames);
                    buffers <= selected_buffers;
                    inputs <= run_inputs;
                    outputs <= run_outputs;
                    error_code <= 0;
                    error_stage <= 0;
                    state_finished <= 0; job_finished <= 0;
                    phase <= STATE_START;
                end
                STATE_START: if (state_run_ready) phase <= STATE_WAIT;
                STATE_WAIT: if (state_done || (monitor_valid && !media_failed)) begin
                    if (state_done && state_error != 0) begin
                        error_code <= state_error;
                        error_stage <= 1;
                        done <= 1;
                        phase <= IDLE;
                    end else
                        phase <= JOB_START;
                end
                JOB_START: if (state_failed) begin
                    error_code <= state_error;
                    error_stage <= 1;
                    done <= 1;
                    phase <= IDLE;
                end else if (job_run_ready && !media_failed) phase <= JOB_WAIT;
                JOB_WAIT: if (state_complete && jobs_complete && midi_lane_complete) begin
                    if (media_failed) begin
                        error_code <= 8;
                        error_stage <= 2;
                    end else if (midi_lane_failed) begin
                        error_code <= 4; error_stage <= 2;
                    end else if (state_failed && !jobs_failed) begin
                        error_code <= state_error;
                        error_stage <= 1;
                    end else begin
                        error_code <= job_error;
                        error_stage <= 2;
                    end
                    done <= 1;
                    phase <= IDLE;
                end
                default: phase <= IDLE;
            endcase
        end
    end
endmodule
