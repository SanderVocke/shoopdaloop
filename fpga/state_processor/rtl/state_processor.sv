`include "state_processor_defs.svh"

module state_processor #(
    parameter integer PROGRAM_DEPTH = 256,
    parameter integer PROGRAM_REGIONS = 1,
    parameter integer STATE_DEPTH = 256,
    parameter integer JOB_DEPTH = 256,
    parameter integer LOOP_DEPTH = STATE_DEPTH,
    parameter integer PROGRAM_ADDR_BITS = (PROGRAM_DEPTH > 1) ? $clog2(PROGRAM_DEPTH) : 1,
    parameter integer STATE_ADDR_BITS = (STATE_DEPTH > 1) ? $clog2(STATE_DEPTH) : 1,
    parameter integer STREAM_JOBS = 0,
    parameter integer DDR_PROGRAM = 0,
    parameter integer EXTERNAL_JOB_FIFO = 0,
    parameter integer JOB_COUNT_BITS = $clog2(JOB_DEPTH + 1)
) (
    input  wire                                clk,
    input  wire                                reset,
    input  wire                                abort_run,
    input  wire                                run_valid,
    output wire                                run_ready,
    input  wire [31:0]                         run_instruction_count,
    input wire [31:0] run_sync_count,
    input wire run_program_region,
    input wire future_switch_valid,
    input wire [43:0] future_switch,
    output wire future_switch_ready,
    input  wire [31:0]                         run_state_count,
    input  wire [31:0]                         run_quantum,
    input wire [31:0] run_chunk_frames,
    input wire [15:0] capture_credits,
    output wire                                busy,
    output wire                                done,
    output wire [7:0]                          error_code,
    output wire [31:0]                         instruction_pc,
    input  wire                                cfg_valid,
    output wire                                cfg_ready,
    input  wire                                cfg_state,
    input wire cfg_loop,
    input  wire                                cfg_write,
    input  wire [15:0]                         cfg_address,
    input  wire [`SHOOP_SP_STATE_BITS-1:0]       cfg_write_data,
    output wire                                cfg_response_valid,
    output wire                                cfg_response_error,
    output wire [`SHOOP_SP_STATE_BITS-1:0]       cfg_read_data,
    output wire                                job_valid,
    input  wire                                job_ready,
    output wire [`SHOOP_SP_JOB_BITS-1:0]         job_data,
    output wire [31:0]                         queued_jobs,
    output wire read_intent_valid,
    output wire [`SHOOP_SP_JOB_BITS-1:0] read_intent_data,
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
    initial begin
        if ((PROGRAM_DEPTH < 1) || (PROGRAM_DEPTH*PROGRAM_REGIONS > 65536)
            || (PROGRAM_REGIONS != 1 && PROGRAM_REGIONS != 2)
            || (STATE_DEPTH < 1) || (STATE_DEPTH > 65536)
            || (JOB_DEPTH < 1) || (JOB_DEPTH > 65536) || LOOP_DEPTH < 1 || LOOP_DEPTH > 4096)
            $fatal(1, "Memory depths must be in 1..65536");
    end

    localparam integer LOOP_MEMORY_BITS = $clog2(LOOP_DEPTH*16);
    reg cfg_response_loop;
    wire start = run_valid && run_ready;
    wire sync_fault;
    wire loop_ram_read, loop_ram_write;
    wire [LOOP_MEMORY_BITS-1:0] loop_ram_read_address, loop_ram_write_address;
    wire [63:0] loop_ram_read_data, loop_ram_write_data;
    reg cfg_pending;
    reg cfg_response_state;
    reg cfg_response_write;
    reg cfg_error;
    wire cfg_accept = cfg_valid && cfg_ready;
    wire cfg_in_range = cfg_loop ? ({16'b0,cfg_address} < 32'(LOOP_DEPTH*16)) : cfg_state
        ? ({16'b0, cfg_address} < 32'(STATE_DEPTH))
        : ({16'b0, cfg_address} < 32'(PROGRAM_DEPTH*PROGRAM_REGIONS));
    wire cfg_program_access = cfg_accept && !cfg_loop && !cfg_state && ({16'b0,cfg_address} < 32'(PROGRAM_DEPTH*PROGRAM_REGIONS));
    wire cfg_state_access = cfg_accept && !cfg_loop && cfg_state && ({16'b0,cfg_address} < 32'(STATE_DEPTH));
    wire cfg_loop_access = cfg_accept && cfg_loop && ({16'b0,cfg_address} < 32'(LOOP_DEPTH*16));
    wire program_read_enable, program_available, program_error;
    wire program_cfg_ready, program_cfg_busy, program_cfg_response, program_cfg_error;
    wire [95:0] program_cfg_data;
    wire external_cfg = DDR_PROGRAM != 0 && !cfg_loop && !cfg_state && cfg_in_range;
    wire [PROGRAM_ADDR_BITS-1:0] program_read_address;
    wire [`SHOOP_SP_INSTRUCTION_BITS-1:0] program_read_data;
    localparam integer PROGRAM_MEMORY_BITS = $clog2(PROGRAM_DEPTH*PROGRAM_REGIONS) > 0 ? $clog2(PROGRAM_DEPTH*PROGRAM_REGIONS) : 1;
    reg program_region;
    wire [PROGRAM_MEMORY_BITS-1:0] program_memory_read_address = busy
        ? PROGRAM_MEMORY_BITS'(program_read_address) + (program_region ? PROGRAM_MEMORY_BITS'(PROGRAM_DEPTH) : 0)
        : PROGRAM_MEMORY_BITS'(cfg_address);
    wire state_read_enable;
    wire [STATE_ADDR_BITS-1:0] state_read_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] state_read_data;
    wire state_write_enable;
    wire [STATE_ADDR_BITS-1:0] state_write_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] state_write_data;
    wire [STATE_ADDR_BITS-1:0] state_memory_read_address = busy
        ? state_read_address : STATE_ADDR_BITS'(cfg_address);
    wire [STATE_ADDR_BITS-1:0] state_memory_write_address = state_write_enable
        ? state_write_address : STATE_ADDR_BITS'(cfg_address);
    wire engine_job_valid;
    wire engine_job_ready;
    wire [`SHOOP_SP_JOB_BITS-1:0] engine_job_data;
    wire fifo_valid;
    wire [JOB_COUNT_BITS-1:0] fifo_count;
    wire jobs_visible = (STREAM_JOBS != 0 || !busy) && !reset;

    assign queued_jobs = 32'(fifo_count);
    assign read_intent_valid = engine_job_valid && engine_job_ready && engine_job_data[7:0] == `SHOOP_SP_JOB_READ;
    assign read_intent_data = engine_job_data;
    assign cfg_ready = !cfg_pending && !program_cfg_busy && !program_cfg_response
        && (!external_cfg || program_cfg_ready) && !reset && (PROGRAM_REGIONS == 1 || !run_valid) && (!busy ||
        (PROGRAM_REGIONS == 2 && cfg_valid && cfg_write && (cfg_loop ? !loop_ram_write : cfg_state ? !state_write_enable : 1'b1)));
    assign run_ready = !busy && !cfg_pending && !program_cfg_busy && !program_cfg_response && (PROGRAM_REGIONS == 2 || !cfg_valid) && !reset && !sync_fault;
    assign cfg_response_valid = (cfg_pending || program_cfg_response) && !reset;
    assign cfg_response_error = program_cfg_response ? program_cfg_error : cfg_error;
    assign cfg_read_data = program_cfg_response ? ((cfg_response_write || program_cfg_error) ? 129'b0 : {33'b0,program_cfg_data})
        : (cfg_response_write || cfg_error) ? 129'b0
        : cfg_response_loop ? {65'b0,loop_ram_read_data} : cfg_response_state ? state_read_data : {33'b0, program_read_data};
    assign job_valid = fifo_valid && jobs_visible;

    always @(posedge clk) begin
        if (reset) begin
            program_region <= 0;
            cfg_pending <= 0; cfg_response_loop <= 0;
            cfg_response_state <= 0;
            cfg_response_write <= 0;
            cfg_error <= 0;
        end else begin
            if (start) program_region <= PROGRAM_REGIONS == 2 && run_program_region;
            cfg_pending <= cfg_accept && !external_cfg;
            if (cfg_accept) begin
                cfg_response_state <= cfg_state; cfg_response_loop <= cfg_loop;
                cfg_response_write <= cfg_write;
                cfg_error <= !cfg_in_range;
            end
        end
    end

    generate if (DDR_PROGRAM != 0) begin : ddr_program
        wire [15:0] region_address = PROGRAM_REGIONS == 2 && run_program_region
            ? 16'(PROGRAM_DEPTH) : 16'd0;
        wire [15:0] prefetched_address;
        wire prefetched_valid;
        // A composite may skip spans or entire parent headers. Consume the
        // intervening rows without returning them to the engine.
        assign program_available = prefetched_valid && prefetched_address == 16'(program_memory_read_address);
        sp_program_prefetch prefetch (
            .clk(clk), .reset(reset), .active(busy),
            .restart(start), .restart_address(region_address),
            .restart_count(run_instruction_count <= 32'(PROGRAM_DEPTH) ? 17'(run_instruction_count) : 17'd0),
            .take_instruction(program_read_enable),
            .instruction_valid(prefetched_valid), .instruction_address(prefetched_address), .instruction_data(program_read_data),
            .instruction_error(program_error),
            .cfg_valid(cfg_program_access), .cfg_ready(program_cfg_ready), .cfg_busy(program_cfg_busy),
            .cfg_write(cfg_write), .cfg_address(cfg_address), .cfg_write_data(cfg_write_data[95:0]),
            .cfg_response_valid(program_cfg_response), .cfg_response_error(program_cfg_error),
            .cfg_read_data(program_cfg_data),
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
    end else begin : local_program
        assign program_available = 1;
        assign program_error = 0;
        assign program_cfg_ready = 1;
        assign program_cfg_busy = 0;
        assign program_cfg_response = 0;
        assign program_cfg_error = 0;
        assign program_cfg_data = 0;
        assign memory_program_valid = 0;
        assign memory_program_write = 0;
        assign memory_program_address = 0;
        assign memory_program_count = 0;
        assign memory_program_write_data = 0;
        assign memory_program_write_valid = 0;
        assign memory_program_read_ready = 0;
        assign memory_program_response_ready = 0;
        wire unused_memory_program = &{1'b0,memory_program_ready,memory_program_write_ready,memory_program_read_valid,memory_program_read_data,memory_program_response_valid,memory_program_response_error};
        sp_ram #(
            .WIDTH(`SHOOP_SP_INSTRUCTION_BITS), .DEPTH(PROGRAM_DEPTH*PROGRAM_REGIONS), .ADDR_BITS(PROGRAM_MEMORY_BITS)
        ) program_memory (
            .clk(clk),
            .read_enable(program_read_enable || (cfg_program_access && !cfg_write)),
            .read_address(program_memory_read_address),
            .read_data(program_read_data),
            .write_enable(cfg_program_access && cfg_write),
            .write_address(PROGRAM_MEMORY_BITS'(cfg_address)),
            .write_data(cfg_write_data[`SHOOP_SP_INSTRUCTION_BITS-1:0])
        );
    end endgenerate

    sp_ram #(
        .WIDTH(`SHOOP_SP_STATE_BITS), .DEPTH(STATE_DEPTH), .ADDR_BITS(STATE_ADDR_BITS)
    ) state_memory (
        .clk(clk),
        .read_enable(state_read_enable || (cfg_state_access && !cfg_write)),
        .read_address(state_memory_read_address),
        .read_data(state_read_data),
        .write_enable(state_write_enable || (cfg_state_access && cfg_write)),
        .write_address(state_memory_write_address),
        .write_data(state_write_enable ? state_write_data : cfg_write_data)
    );

    sp_engine #(
        .PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH), .LOOP_DEPTH(LOOP_DEPTH), .LOOP_MEMORY_BITS(LOOP_MEMORY_BITS),
        .PROGRAM_ADDR_BITS(PROGRAM_ADDR_BITS), .STATE_ADDR_BITS(STATE_ADDR_BITS),
        .STREAM_JOBS(STREAM_JOBS)
    ) engine (
        .clk(clk), .reset(reset), .start(start),
        .program_changed(PROGRAM_REGIONS == 2 && run_program_region != program_region),
        .future_switch_valid(future_switch_valid), .future_switch(future_switch), .future_switch_ready(future_switch_ready),
        .instruction_count(run_instruction_count), .sync_count(run_sync_count), .state_count(run_state_count),
        .quantum(run_quantum), .chunk_frames(run_chunk_frames), .capture_credits(capture_credits),
        .queued_jobs(queued_jobs), .abort_run(STREAM_JOBS != 0 && abort_run),
        .busy(busy), .done(done), .error_code(error_code), .instruction_pc(instruction_pc),
        .program_read_enable(program_read_enable), .program_read_address(program_read_address),
        .program_available(program_available), .program_error(program_error),
        .program_read_data(program_read_data),
        .state_read_enable(state_read_enable), .state_read_address(state_read_address),
        .state_read_data(state_read_data), .state_write_enable(state_write_enable),
        .state_write_address(state_write_address), .state_write_data(state_write_data),
        .job_valid(engine_job_valid), .job_ready(engine_job_ready), .job_data(engine_job_data),
        .loop_read_valid(loop_read_valid), .loop_read_ready(loop_read_ready), .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid), .loop_response_error(loop_response_error), .loop_response_data(loop_response_data),
        .loop_ram_read_enable(loop_ram_read), .loop_ram_read_address(loop_ram_read_address), .loop_ram_read_data(loop_ram_read_data),
        .loop_ram_write_enable(loop_ram_write), .loop_ram_write_address(loop_ram_write_address), .loop_ram_write_data(loop_ram_write_data),
        .sync_event_valid(sync_event_valid), .sync_event_ready(sync_event_ready), .sync_event_data(sync_event_data),
        .sync_frame(sync_frame), .faulted(sync_fault)
    );

    sp_ram #(.WIDTH(64), .DEPTH(LOOP_DEPTH*16), .ADDR_BITS(LOOP_MEMORY_BITS)) loop_memory (
        .clk(clk), .read_enable(loop_ram_read || (cfg_loop_access && !cfg_write)),
        .read_address(busy ? loop_ram_read_address : LOOP_MEMORY_BITS'(cfg_address)),
        .read_data(loop_ram_read_data),
        .write_enable(loop_ram_write || (cfg_loop_access && cfg_write)),
        .write_address(loop_ram_write ? loop_ram_write_address : LOOP_MEMORY_BITS'(cfg_address)),
        .write_data(loop_ram_write ? loop_ram_write_data : cfg_write_data[63:0])
    );

    generate if (EXTERNAL_JOB_FIFO != 0) begin : external_jobs
        assign fifo_valid = engine_job_valid;
        assign engine_job_ready = job_ready && jobs_visible;
        assign job_data = engine_job_data;
        assign fifo_count = 0;
    end else begin : buffered_jobs
        sp_job_fifo #(.DEPTH(JOB_DEPTH), .COUNT_BITS(JOB_COUNT_BITS)) jobs (
            .clk(clk), .reset(reset),
            .push_valid(engine_job_valid), .push_ready(engine_job_ready), .push_data(engine_job_data),
            .pop_valid(fifo_valid), .pop_ready(job_ready && jobs_visible),
            .pop_data(job_data), .count(fifo_count)
        );
    end endgenerate
endmodule
