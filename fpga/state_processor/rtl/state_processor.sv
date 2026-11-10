`include "state_processor_defs.svh"

module state_processor #(
    parameter integer PROGRAM_DEPTH = 256,
    parameter integer STATE_DEPTH = 256,
    parameter integer JOB_DEPTH = 256,
    parameter integer PROGRAM_ADDR_BITS = (PROGRAM_DEPTH > 1) ? $clog2(PROGRAM_DEPTH) : 1,
    parameter integer STATE_ADDR_BITS = (STATE_DEPTH > 1) ? $clog2(STATE_DEPTH) : 1,
    parameter integer JOB_COUNT_BITS = $clog2(JOB_DEPTH + 1)
) (
    input  wire                                clk,
    input  wire                                reset,
    input  wire                                run_valid,
    output wire                                run_ready,
    input  wire [31:0]                         run_instruction_count,
    input  wire [31:0]                         run_state_count,
    input  wire [31:0]                         run_quantum,
    input  wire [31:0]                         run_job_budget,
    output wire                                busy,
    output wire                                done,
    output wire [7:0]                          error_code,
    output wire [31:0]                         instruction_pc,
    input  wire                                cfg_valid,
    output wire                                cfg_ready,
    input  wire                                cfg_state,
    input  wire                                cfg_write,
    input  wire [15:0]                         cfg_address,
    input  wire [`SHOOP_SP_STATE_BITS-1:0]       cfg_write_data,
    output wire                                cfg_response_valid,
    output wire                                cfg_response_error,
    output wire [`SHOOP_SP_STATE_BITS-1:0]       cfg_read_data,
    output wire                                job_valid,
    input  wire                                job_ready,
    output wire [`SHOOP_SP_JOB_BITS-1:0]         job_data,
    output wire [31:0]                         queued_jobs
);
    initial begin
        if ((PROGRAM_DEPTH < 1) || (PROGRAM_DEPTH > 65536)
            || (STATE_DEPTH < 1) || (STATE_DEPTH > 65536)
            || (JOB_DEPTH < 1) || (JOB_DEPTH > 65536))
            $fatal(1, "Memory depths must be in 1..65536");
    end

    reg cfg_pending;
    reg cfg_response_state;
    reg cfg_response_write;
    reg cfg_error;
    wire cfg_accept = cfg_valid && cfg_ready;
    wire cfg_in_range = cfg_state
        ? ({16'b0, cfg_address} < 32'(STATE_DEPTH))
        : ({16'b0, cfg_address} < 32'(PROGRAM_DEPTH));
    wire cfg_program_access = cfg_accept && !cfg_state && cfg_in_range;
    wire cfg_state_access = cfg_accept && cfg_state && cfg_in_range;
    wire program_read_enable;
    wire [PROGRAM_ADDR_BITS-1:0] program_read_address;
    wire [`SHOOP_SP_INSTRUCTION_BITS-1:0] program_read_data;
    wire [PROGRAM_ADDR_BITS-1:0] program_memory_read_address = busy
        ? program_read_address : PROGRAM_ADDR_BITS'(cfg_address);
    wire state_read_enable;
    wire [STATE_ADDR_BITS-1:0] state_read_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] state_read_data;
    wire state_write_enable;
    wire [STATE_ADDR_BITS-1:0] state_write_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] state_write_data;
    wire [STATE_ADDR_BITS-1:0] state_memory_read_address = busy
        ? state_read_address : STATE_ADDR_BITS'(cfg_address);
    wire [STATE_ADDR_BITS-1:0] state_memory_write_address = busy
        ? state_write_address : STATE_ADDR_BITS'(cfg_address);
    wire engine_job_valid;
    wire engine_job_ready;
    wire [`SHOOP_SP_JOB_BITS-1:0] engine_job_data;
    wire fifo_valid;
    wire [JOB_COUNT_BITS-1:0] fifo_count;
    wire [31:0] queue_remaining = 32'(JOB_DEPTH) - queued_jobs;

    assign queued_jobs = 32'(fifo_count);
    assign cfg_ready = !busy && !cfg_pending && !reset;
    assign run_ready = !busy && !cfg_pending && !cfg_valid && !reset;
    assign cfg_response_valid = cfg_pending && !reset;
    assign cfg_response_error = cfg_error;
    assign cfg_read_data = (cfg_response_write || cfg_error) ? 129'b0
        : cfg_response_state ? state_read_data : {33'b0, program_read_data};
    assign job_valid = fifo_valid && !busy && !reset;

    always @(posedge clk) begin
        if (reset) begin
            cfg_pending <= 0;
            cfg_response_state <= 0;
            cfg_response_write <= 0;
            cfg_error <= 0;
        end else begin
            cfg_pending <= cfg_accept;
            if (cfg_accept) begin
                cfg_response_state <= cfg_state;
                cfg_response_write <= cfg_write;
                cfg_error <= !cfg_in_range;
            end
        end
    end

    sp_ram #(
        .WIDTH(`SHOOP_SP_INSTRUCTION_BITS), .DEPTH(PROGRAM_DEPTH), .ADDR_BITS(PROGRAM_ADDR_BITS)
    ) program_memory (
        .clk(clk),
        .read_enable(program_read_enable || (cfg_program_access && !cfg_write)),
        .read_address(program_memory_read_address),
        .read_data(program_read_data),
        .write_enable(cfg_program_access && cfg_write),
        .write_address(PROGRAM_ADDR_BITS'(cfg_address)),
        .write_data(cfg_write_data[`SHOOP_SP_INSTRUCTION_BITS-1:0])
    );

    sp_ram #(
        .WIDTH(`SHOOP_SP_STATE_BITS), .DEPTH(STATE_DEPTH), .ADDR_BITS(STATE_ADDR_BITS)
    ) state_memory (
        .clk(clk),
        .read_enable(state_read_enable || (cfg_state_access && !cfg_write)),
        .read_address(state_memory_read_address),
        .read_data(state_read_data),
        .write_enable(state_write_enable || (cfg_state_access && cfg_write)),
        .write_address(state_memory_write_address),
        .write_data(busy ? state_write_data : cfg_write_data)
    );

    sp_engine #(
        .PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH),
        .PROGRAM_ADDR_BITS(PROGRAM_ADDR_BITS), .STATE_ADDR_BITS(STATE_ADDR_BITS)
    ) engine (
        .clk(clk), .reset(reset), .start(run_valid && run_ready),
        .instruction_count(run_instruction_count), .state_count(run_state_count),
        .quantum(run_quantum), .job_budget(run_job_budget),
        .queued_jobs(queued_jobs), .queue_remaining(queue_remaining),
        .busy(busy), .done(done), .error_code(error_code), .instruction_pc(instruction_pc),
        .program_read_enable(program_read_enable), .program_read_address(program_read_address),
        .program_read_data(program_read_data),
        .state_read_enable(state_read_enable), .state_read_address(state_read_address),
        .state_read_data(state_read_data), .state_write_enable(state_write_enable),
        .state_write_address(state_write_address), .state_write_data(state_write_data),
        .job_valid(engine_job_valid), .job_ready(engine_job_ready), .job_data(engine_job_data)
    );

    sp_job_fifo #(.DEPTH(JOB_DEPTH), .COUNT_BITS(JOB_COUNT_BITS)) jobs (
        .clk(clk), .reset(reset),
        .push_valid(engine_job_valid), .push_ready(engine_job_ready), .push_data(engine_job_data),
        .pop_valid(fifo_valid), .pop_ready(job_ready && !busy && !reset),
        .pop_data(job_data), .count(fifo_count)
    );
endmodule
