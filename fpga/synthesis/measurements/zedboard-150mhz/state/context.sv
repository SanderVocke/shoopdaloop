`include "state_processor_defs.svh"
module registered_core_context #(

    parameter integer PROGRAM_DEPTH = 256,
    parameter integer STATE_DEPTH = 256,
    parameter integer JOB_DEPTH = 256,
    parameter integer PROGRAM_ADDR_BITS = (PROGRAM_DEPTH > 1) ? $clog2(PROGRAM_DEPTH) : 1,
    parameter integer STATE_ADDR_BITS = (STATE_DEPTH > 1) ? $clog2(STATE_DEPTH) : 1,
    parameter integer JOB_COUNT_BITS = $clog2(JOB_DEPTH + 1)

) (input wire clk);
reg [7:0] startup = 0;
always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;
reg reset = 1;
always @(posedge clk) reset <= !(&startup);
localparam integer PORT_BITS_run_valid = 1;
localparam integer SOURCE_BITS_run_valid = PORT_BITS_run_valid < 32 ? 32 : PORT_BITS_run_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_valid-1:0] source_run_valid = SOURCE_BITS_run_valid'(3);
always @(posedge clk) source_run_valid <= {source_run_valid[SOURCE_BITS_run_valid-2:0],source_run_valid[SOURCE_BITS_run_valid-1]^source_run_valid[SOURCE_BITS_run_valid-2]};
wire  run_valid = PORT_BITS_run_valid'(source_run_valid);
wire  run_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_run_ready;
always @(posedge clk) unused_capture_run_ready <= run_ready;
localparam integer PORT_BITS_run_instruction_count = (31)+1;
localparam integer SOURCE_BITS_run_instruction_count = PORT_BITS_run_instruction_count < 32 ? 32 : PORT_BITS_run_instruction_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_instruction_count-1:0] source_run_instruction_count = SOURCE_BITS_run_instruction_count'(5);
always @(posedge clk) source_run_instruction_count <= {source_run_instruction_count[SOURCE_BITS_run_instruction_count-2:0],source_run_instruction_count[SOURCE_BITS_run_instruction_count-1]^source_run_instruction_count[SOURCE_BITS_run_instruction_count-2]};
wire [31:0] run_instruction_count = PORT_BITS_run_instruction_count'(source_run_instruction_count);
localparam integer PORT_BITS_run_state_count = (31)+1;
localparam integer SOURCE_BITS_run_state_count = PORT_BITS_run_state_count < 32 ? 32 : PORT_BITS_run_state_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_state_count-1:0] source_run_state_count = SOURCE_BITS_run_state_count'(6);
always @(posedge clk) source_run_state_count <= {source_run_state_count[SOURCE_BITS_run_state_count-2:0],source_run_state_count[SOURCE_BITS_run_state_count-1]^source_run_state_count[SOURCE_BITS_run_state_count-2]};
wire [31:0] run_state_count = PORT_BITS_run_state_count'(source_run_state_count);
localparam integer PORT_BITS_run_quantum = (31)+1;
localparam integer SOURCE_BITS_run_quantum = PORT_BITS_run_quantum < 32 ? 32 : PORT_BITS_run_quantum;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_quantum-1:0] source_run_quantum = SOURCE_BITS_run_quantum'(7);
always @(posedge clk) source_run_quantum <= {source_run_quantum[SOURCE_BITS_run_quantum-2:0],source_run_quantum[SOURCE_BITS_run_quantum-1]^source_run_quantum[SOURCE_BITS_run_quantum-2]};
wire [31:0] run_quantum = PORT_BITS_run_quantum'(source_run_quantum);
localparam integer PORT_BITS_run_chunk_frames = (31)+1;
localparam integer SOURCE_BITS_run_chunk_frames = PORT_BITS_run_chunk_frames < 32 ? 32 : PORT_BITS_run_chunk_frames;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_chunk_frames-1:0] source_run_chunk_frames = SOURCE_BITS_run_chunk_frames'(8);
always @(posedge clk) source_run_chunk_frames <= {source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-2:0],source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-1]^source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-2]};
wire [31:0] run_chunk_frames = PORT_BITS_run_chunk_frames'(source_run_chunk_frames);
localparam integer PORT_BITS_run_job_budget = (31)+1;
localparam integer SOURCE_BITS_run_job_budget = PORT_BITS_run_job_budget < 32 ? 32 : PORT_BITS_run_job_budget;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_job_budget-1:0] source_run_job_budget = SOURCE_BITS_run_job_budget'(9);
always @(posedge clk) source_run_job_budget <= {source_run_job_budget[SOURCE_BITS_run_job_budget-2:0],source_run_job_budget[SOURCE_BITS_run_job_budget-1]^source_run_job_budget[SOURCE_BITS_run_job_budget-2]};
wire [31:0] run_job_budget = PORT_BITS_run_job_budget'(source_run_job_budget);
wire  busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_busy;
always @(posedge clk) unused_capture_busy <= busy;
wire  done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_done;
always @(posedge clk) unused_capture_done <= done;
wire [7:0] error_code;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [7:0] unused_capture_error_code;
always @(posedge clk) unused_capture_error_code <= error_code;
wire [31:0] instruction_pc;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_instruction_pc;
always @(posedge clk) unused_capture_instruction_pc <= instruction_pc;
localparam integer PORT_BITS_cfg_valid = 1;
localparam integer SOURCE_BITS_cfg_valid = PORT_BITS_cfg_valid < 32 ? 32 : PORT_BITS_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_valid-1:0] source_cfg_valid = SOURCE_BITS_cfg_valid'(14);
always @(posedge clk) source_cfg_valid <= {source_cfg_valid[SOURCE_BITS_cfg_valid-2:0],source_cfg_valid[SOURCE_BITS_cfg_valid-1]^source_cfg_valid[SOURCE_BITS_cfg_valid-2]};
wire  cfg_valid = PORT_BITS_cfg_valid'(source_cfg_valid);
wire  cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_ready;
always @(posedge clk) unused_capture_cfg_ready <= cfg_ready;
localparam integer PORT_BITS_cfg_state = 1;
localparam integer SOURCE_BITS_cfg_state = PORT_BITS_cfg_state < 32 ? 32 : PORT_BITS_cfg_state;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_state-1:0] source_cfg_state = SOURCE_BITS_cfg_state'(16);
always @(posedge clk) source_cfg_state <= {source_cfg_state[SOURCE_BITS_cfg_state-2:0],source_cfg_state[SOURCE_BITS_cfg_state-1]^source_cfg_state[SOURCE_BITS_cfg_state-2]};
wire  cfg_state = PORT_BITS_cfg_state'(source_cfg_state);
localparam integer PORT_BITS_cfg_write = 1;
localparam integer SOURCE_BITS_cfg_write = PORT_BITS_cfg_write < 32 ? 32 : PORT_BITS_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write-1:0] source_cfg_write = SOURCE_BITS_cfg_write'(17);
always @(posedge clk) source_cfg_write <= {source_cfg_write[SOURCE_BITS_cfg_write-2:0],source_cfg_write[SOURCE_BITS_cfg_write-1]^source_cfg_write[SOURCE_BITS_cfg_write-2]};
wire  cfg_write = PORT_BITS_cfg_write'(source_cfg_write);
localparam integer PORT_BITS_cfg_address = (15)+1;
localparam integer SOURCE_BITS_cfg_address = PORT_BITS_cfg_address < 32 ? 32 : PORT_BITS_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_address-1:0] source_cfg_address = SOURCE_BITS_cfg_address'(18);
always @(posedge clk) source_cfg_address <= {source_cfg_address[SOURCE_BITS_cfg_address-2:0],source_cfg_address[SOURCE_BITS_cfg_address-1]^source_cfg_address[SOURCE_BITS_cfg_address-2]};
wire [15:0] cfg_address = PORT_BITS_cfg_address'(source_cfg_address);
localparam integer PORT_BITS_cfg_write_data = (`SHOOP_SP_STATE_BITS-1)+1;
localparam integer SOURCE_BITS_cfg_write_data = PORT_BITS_cfg_write_data < 32 ? 32 : PORT_BITS_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write_data-1:0] source_cfg_write_data = SOURCE_BITS_cfg_write_data'(19);
always @(posedge clk) source_cfg_write_data <= {source_cfg_write_data[SOURCE_BITS_cfg_write_data-2:0],source_cfg_write_data[SOURCE_BITS_cfg_write_data-1]^source_cfg_write_data[SOURCE_BITS_cfg_write_data-2]};
wire [`SHOOP_SP_STATE_BITS-1:0] cfg_write_data = PORT_BITS_cfg_write_data'(source_cfg_write_data);
wire  cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_response_valid;
always @(posedge clk) unused_capture_cfg_response_valid <= cfg_response_valid;
wire  cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_response_error;
always @(posedge clk) unused_capture_cfg_response_error <= cfg_response_error;
wire [`SHOOP_SP_STATE_BITS-1:0] cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [`SHOOP_SP_STATE_BITS-1:0] unused_capture_cfg_read_data;
always @(posedge clk) unused_capture_cfg_read_data <= cfg_read_data;
wire  job_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_job_valid;
always @(posedge clk) unused_capture_job_valid <= job_valid;
localparam integer PORT_BITS_job_ready = 1;
localparam integer SOURCE_BITS_job_ready = PORT_BITS_job_ready < 32 ? 32 : PORT_BITS_job_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_job_ready-1:0] source_job_ready = SOURCE_BITS_job_ready'(24);
always @(posedge clk) source_job_ready <= {source_job_ready[SOURCE_BITS_job_ready-2:0],source_job_ready[SOURCE_BITS_job_ready-1]^source_job_ready[SOURCE_BITS_job_ready-2]};
wire  job_ready = PORT_BITS_job_ready'(source_job_ready);
wire [`SHOOP_SP_JOB_BITS-1:0] job_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [`SHOOP_SP_JOB_BITS-1:0] unused_capture_job_data;
always @(posedge clk) unused_capture_job_data <= job_data;
wire [31:0] queued_jobs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_queued_jobs;
always @(posedge clk) unused_capture_queued_jobs <= queued_jobs;
wire  read_intent_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_read_intent_valid;
always @(posedge clk) unused_capture_read_intent_valid <= read_intent_valid;
wire [`SHOOP_SP_JOB_BITS-1:0] read_intent_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [`SHOOP_SP_JOB_BITS-1:0] unused_capture_read_intent_data;
always @(posedge clk) unused_capture_read_intent_data <= read_intent_data;
wire  loop_read_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_loop_read_valid;
always @(posedge clk) unused_capture_loop_read_valid <= loop_read_valid;
localparam integer PORT_BITS_loop_read_ready = 1;
localparam integer SOURCE_BITS_loop_read_ready = PORT_BITS_loop_read_ready < 32 ? 32 : PORT_BITS_loop_read_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_read_ready-1:0] source_loop_read_ready = SOURCE_BITS_loop_read_ready'(30);
always @(posedge clk) source_loop_read_ready <= {source_loop_read_ready[SOURCE_BITS_loop_read_ready-2:0],source_loop_read_ready[SOURCE_BITS_loop_read_ready-1]^source_loop_read_ready[SOURCE_BITS_loop_read_ready-2]};
wire  loop_read_ready = PORT_BITS_loop_read_ready'(source_loop_read_ready);
wire [31:0] loop_read_slot;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_loop_read_slot;
always @(posedge clk) unused_capture_loop_read_slot <= loop_read_slot;
localparam integer PORT_BITS_loop_response_valid = 1;
localparam integer SOURCE_BITS_loop_response_valid = PORT_BITS_loop_response_valid < 32 ? 32 : PORT_BITS_loop_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_response_valid-1:0] source_loop_response_valid = SOURCE_BITS_loop_response_valid'(32);
always @(posedge clk) source_loop_response_valid <= {source_loop_response_valid[SOURCE_BITS_loop_response_valid-2:0],source_loop_response_valid[SOURCE_BITS_loop_response_valid-1]^source_loop_response_valid[SOURCE_BITS_loop_response_valid-2]};
wire  loop_response_valid = PORT_BITS_loop_response_valid'(source_loop_response_valid);
localparam integer PORT_BITS_loop_response_error = 1;
localparam integer SOURCE_BITS_loop_response_error = PORT_BITS_loop_response_error < 32 ? 32 : PORT_BITS_loop_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_response_error-1:0] source_loop_response_error = SOURCE_BITS_loop_response_error'(33);
always @(posedge clk) source_loop_response_error <= {source_loop_response_error[SOURCE_BITS_loop_response_error-2:0],source_loop_response_error[SOURCE_BITS_loop_response_error-1]^source_loop_response_error[SOURCE_BITS_loop_response_error-2]};
wire  loop_response_error = PORT_BITS_loop_response_error'(source_loop_response_error);
localparam integer PORT_BITS_loop_response_data = (163)+1;
localparam integer SOURCE_BITS_loop_response_data = PORT_BITS_loop_response_data < 32 ? 32 : PORT_BITS_loop_response_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_response_data-1:0] source_loop_response_data = SOURCE_BITS_loop_response_data'(34);
always @(posedge clk) source_loop_response_data <= {source_loop_response_data[SOURCE_BITS_loop_response_data-2:0],source_loop_response_data[SOURCE_BITS_loop_response_data-1]^source_loop_response_data[SOURCE_BITS_loop_response_data-2]};
wire [163:0] loop_response_data = PORT_BITS_loop_response_data'(source_loop_response_data);
(* KEEP_HIERARCHY = "yes" *) state_processor #(
.PROGRAM_DEPTH(PROGRAM_DEPTH),
.STATE_DEPTH(STATE_DEPTH),
.JOB_DEPTH(JOB_DEPTH),
.PROGRAM_ADDR_BITS(PROGRAM_ADDR_BITS),
.STATE_ADDR_BITS(STATE_ADDR_BITS),
.JOB_COUNT_BITS(JOB_COUNT_BITS)
) core_under_test (
.clk(clk),
.reset(reset),
.run_valid(run_valid),
.run_ready(run_ready),
.run_instruction_count(run_instruction_count),
.run_state_count(run_state_count),
.run_quantum(run_quantum),
.run_chunk_frames(run_chunk_frames),
.run_job_budget(run_job_budget),
.busy(busy),
.done(done),
.error_code(error_code),
.instruction_pc(instruction_pc),
.cfg_valid(cfg_valid),
.cfg_ready(cfg_ready),
.cfg_state(cfg_state),
.cfg_write(cfg_write),
.cfg_address(cfg_address),
.cfg_write_data(cfg_write_data),
.cfg_response_valid(cfg_response_valid),
.cfg_response_error(cfg_response_error),
.cfg_read_data(cfg_read_data),
.job_valid(job_valid),
.job_ready(job_ready),
.job_data(job_data),
.queued_jobs(queued_jobs),
.read_intent_valid(read_intent_valid),
.read_intent_data(read_intent_data),
.loop_read_valid(loop_read_valid),
.loop_read_ready(loop_read_ready),
.loop_read_slot(loop_read_slot),
.loop_response_valid(loop_response_valid),
.loop_response_error(loop_response_error),
.loop_response_data(loop_response_data)
);
endmodule
