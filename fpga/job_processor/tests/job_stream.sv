`include "state_processor_defs.svh"

module job_stream;
    localparam integer CONSTANTS = 2049;
    reg clk = 0;
    always #5 clk = ~clk;

    reg reset = 1;
    reg run_valid = 0, job_enable = 0, drain_ready = 0;
    reg [31:0] instruction_count = 0;
    wire run_ready, busy, done, state_busy, state_done, jobs_busy;
    wire [7:0] error_code;
    wire [1:0] error_stage;
    wire [31:0] queued_jobs;
    reg cfg_valid = 0;
    wire cfg_ready, cfg_response_valid, cfg_response_error;
    reg [15:0] cfg_address = 0;
    reg [128:0] cfg_write_data = 0;
    wire monitor_valid, monitor_ready;
    wire [255:0] monitor_data;
    wire loop_read_valid;
    reg loop_response_valid = 0;
    wire sample_output_write;
    wire [31:0] sample_output_data;

    audio_pipeline #(
        .PROGRAM_DEPTH(4096), .JOB_DEPTH(2048), .READ_HINT_DEPTH(256),
        .EXTERNAL_IO(1), .MAX_FRAMES(1), .BUFFERS(4096)
    ) dut (
        .clk(clk), .reset(reset),
        .run_valid(run_valid), .run_ready(run_ready),
        .run_instruction_count(instruction_count), .run_sync_count(32'd0), .run_program_region(1'b0),
        .switch_valid(1'b0), .switch_expected(64'd0), .switch_revision(64'd0), .switch_region(1'b0),
        .switch_instructions(16'd0), .switch_sync_count(16'd0), .switch_states(16'd0),
        .switch_buffers(16'd0), .switch_response_ready(1'b1), .handoff_complete(1'b0),
        .run_state_count(32'd0), .run_quantum(32'd1), .run_chunk_frames(32'd1), .capture_credits(16'd0),
        .run_buffers(32'(CONSTANTS)), .run_inputs(32'd0), .run_outputs(32'd1),
        .job_enable(job_enable), .busy(busy), .done(done),
        .state_busy(state_busy), .state_done(state_done), .jobs_busy(jobs_busy),
        .error_code(error_code), .error_stage(error_stage),
        .cfg_valid(cfg_valid), .cfg_ready(cfg_ready), .cfg_state(1'b0), .cfg_loop(1'b0), .cfg_write(1'b1),
        .cfg_address(cfg_address), .cfg_write_data(cfg_write_data),
        .cfg_response_valid(cfg_response_valid), .cfg_response_error(cfg_response_error),
        .audio_cfg_valid(1'b0), .audio_cfg_space(2'd0), .audio_cfg_write(1'b0),
        .audio_cfg_address(32'd0), .audio_cfg_write_data(32'd0),
        .monitor_valid(monitor_valid), .monitor_ready(monitor_ready), .monitor_data(monitor_data),
        .queued_jobs(queued_jobs), .drain_ready(drain_ready),
        .loop_read_valid(loop_read_valid), .loop_read_ready(1'b1), .loop_response_valid(loop_response_valid),
        .loop_response_error(1'b0), .loop_response_data(164'd1), .sync_event_ready(1'b1),
        .sample_input_data(32'd0), .sample_output_write(sample_output_write), .sample_output_data(sample_output_data),
        .media_cmd_ready(1'b0), .media_done(1'b0), .media_error(8'd0), .media_fault(1'b0),
        .media_write_ready(1'b0), .media_read_valid(1'b0), .media_read_data(32'd0), .media_hint_ready(1'b0),
        .midi_job_ready(1'b0), .midi_job_done(1'b0), .midi_job_error(4'd0)
    );

    reg check_stream = 0, overlapped = 0;
    integer consumed = 0, writes = 0;
    always @(posedge clk) if (check_stream) begin
        if (monitor_valid && monitor_ready) begin
            if (consumed < CONSTANTS) begin
                if (monitor_data[7:0] != `SHOOP_SP_JOB_CONSTANT || monitor_data[23:8] != 16'(consumed)
                    || monitor_data[95:64] != 32'(consumed+1))
                    $fatal(1, "Unexpected job at index %0d", consumed);
            end else if (consumed != CONSTANTS || monitor_data[7:0] != `SHOOP_SP_JOB_OUTPUT)
                $fatal(1, "Unexpected final job at index %0d", consumed);
            if (state_busy && jobs_busy) overlapped <= 1;
            consumed <= consumed + 1;
        end
        if (sample_output_write) begin
            if ((writes == 0 && sample_output_data != 0)
                || (writes == 1 && sample_output_data != 32'(CONSTANTS)) || writes > 1)
                $fatal(1, "Unexpected sample write %0d: %0d", writes, sample_output_data);
            writes <= writes + 1;
        end
    end

    task automatic program_word(input [15:0] address, input [95:0] word);
        cfg_address = address;
        cfg_write_data = {33'b0, word};
        cfg_valid = 1;
        do @(posedge clk); while (!cfg_ready);
        @(negedge clk);
        if (!cfg_response_valid || cfg_response_error) $fatal(1, "Program write failed");
        cfg_valid = 0;
    endtask

    task automatic start_run(input [31:0] instructions);
        instruction_count = instructions;
        run_valid = 1;
        do @(posedge clk); while (!run_ready);
        @(negedge clk);
        run_valid = 0;
    endtask

    task automatic wait_full;
        while (queued_jobs != 512) @(negedge clk);
        repeat (20) begin
            @(negedge clk);
            if (!state_busy || !jobs_busy || state_done || done || queued_jobs != 512)
                $fatal(1, "Producer or consumer failed to wait on a full FIFO");
        end
    endtask

    task automatic wait_done(input [7:0] expected_error, input [1:0] expected_stage);
        while (!done) @(negedge clk);
        if (busy || state_busy || jobs_busy || error_code != expected_error || error_stage != expected_stage)
            $fatal(1, "Completion %0d/%0d (expected %0d/%0d), busy %b%b%b",
                error_stage, error_code, expected_stage, expected_error, busy, state_busy, jobs_busy);
    endtask

    initial begin
        repeat (2) @(negedge clk);
        reset = 0;
        for (integer i = 0; i < CONSTANTS; i++)
            program_word(16'(i), {40'd0,16'(i),32'(i+1),`SHOOP_SP_PROCESS_CONSTANT});
        program_word(16'(CONSTANTS), {72'd0,16'(CONSTANTS-1),`SHOOP_SP_PROCESS_OUTPUT});

        check_stream = 1;
        start_run(CONSTANTS+1);
        wait_full();
        job_enable = 1;
        wait_done(0, 2);
        check_stream = 0;
        if (!overlapped || queued_jobs != 0 || consumed != CONSTANTS+1 || writes != 2)
            $fatal(1, "Incorrect streamed job count, sample count, or overlap");

        // A consumer error must release a producer blocked on a full FIFO.
        program_word(0, {40'd0,16'hffff,32'd1,`SHOOP_SP_PROCESS_CONSTANT});
        job_enable = 0;
        start_run(CONSTANTS+1);
        wait_full();
        job_enable = 1;
        wait_done(8'd1, 2);
        drain_ready = 1;
        while (queued_jobs != 0) @(negedge clk);
        drain_ready = 0;

        program_word(0, {40'd0,16'd0,32'd1,`SHOOP_SP_PROCESS_CONSTANT});
        program_word(8, {88'd0,8'hff});
        start_run(9);
        wait_done(`SHOOP_SP_INVALID_PROGRAM, 1);
        drain_ready = 1;
        while (queued_jobs != 0) @(negedge clk);
        drain_ready = 0;

        // The consumer waits through an empty FIFO while the producer reads a loop.
        program_word(1, {8'd0,16'd1,64'd0,`SHOOP_SP_LOGICAL_PLAYBACK});
        start_run(2);
        while (!loop_read_valid) @(negedge clk);
        @(negedge clk);
        while (queued_jobs != 0) @(negedge clk);
        repeat (20) begin
            @(negedge clk);
            if (!state_busy || !jobs_busy || done) $fatal(1, "Consumer finished during an empty FIFO gap");
        end
        loop_response_valid = 1;
        @(negedge clk);
        loop_response_valid = 0;
        wait_done(0, 2);

        start_run(1);
        wait_done(0, 2);
        start_run(0);
        wait_done(0, 2);
        $display("Concurrent jobs, full FIFO recovery, and run completion passed");
        $finish;
    end

    initial begin
        repeat (30_000) @(posedge clk);
        $fatal(1, "Timed out");
    end
endmodule
