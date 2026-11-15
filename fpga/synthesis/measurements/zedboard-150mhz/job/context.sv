`include "state_processor_defs.svh"
module registered_core_context #(

    parameter integer EXTERNAL_IO = 0,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer CHUNK_FRAMES = 1,
    parameter integer ENABLE_MEDIA = 0,
    parameter integer BUFFER_BITS = (BUFFERS > 1) ? $clog2(BUFFERS) : 1,
    parameter integer SCRATCH_BITS = (BUFFERS * MAX_FRAMES > 1) ? $clog2(BUFFERS * MAX_FRAMES) : 1,
    parameter integer INPUT_BITS = (INPUT_PORTS * MAX_FRAMES > 1) ? $clog2(INPUT_PORTS * MAX_FRAMES) : 1,
    parameter integer OUTPUT_BITS = (OUTPUT_PORTS * MAX_FRAMES > 1) ? $clog2(OUTPUT_PORTS * MAX_FRAMES) : 1

) (input wire clk);
reg [7:0] startup = 0;
always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;
reg reset = 1;
always @(posedge clk) reset <= !(&startup);
wire  sample_input_read;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_sample_input_read;
always @(posedge clk) unused_capture_sample_input_read <= sample_input_read;
wire [31:0] sample_input_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_sample_input_address;
always @(posedge clk) unused_capture_sample_input_address <= sample_input_address;
localparam integer PORT_BITS_sample_input_data = (31)+1;
localparam integer SOURCE_BITS_sample_input_data = PORT_BITS_sample_input_data < 32 ? 32 : PORT_BITS_sample_input_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_sample_input_data-1:0] source_sample_input_data = SOURCE_BITS_sample_input_data'(3);
always @(posedge clk) source_sample_input_data <= {source_sample_input_data[SOURCE_BITS_sample_input_data-2:0],source_sample_input_data[SOURCE_BITS_sample_input_data-1]^source_sample_input_data[SOURCE_BITS_sample_input_data-2]};
wire [31:0] sample_input_data = PORT_BITS_sample_input_data'(source_sample_input_data);
wire  sample_output_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_sample_output_write;
always @(posedge clk) unused_capture_sample_output_write <= sample_output_write;
wire [31:0] sample_output_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_sample_output_address;
always @(posedge clk) unused_capture_sample_output_address <= sample_output_address;
wire [31:0] sample_output_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_sample_output_data;
always @(posedge clk) unused_capture_sample_output_data <= sample_output_data;
localparam integer PORT_BITS_run_valid = 1;
localparam integer SOURCE_BITS_run_valid = PORT_BITS_run_valid < 32 ? 32 : PORT_BITS_run_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_valid-1:0] source_run_valid = SOURCE_BITS_run_valid'(9);
always @(posedge clk) source_run_valid <= {source_run_valid[SOURCE_BITS_run_valid-2:0],source_run_valid[SOURCE_BITS_run_valid-1]^source_run_valid[SOURCE_BITS_run_valid-2]};
wire  run_valid = PORT_BITS_run_valid'(source_run_valid);
wire  run_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_run_ready;
always @(posedge clk) unused_capture_run_ready <= run_ready;
localparam integer PORT_BITS_run_quantum = (31)+1;
localparam integer SOURCE_BITS_run_quantum = PORT_BITS_run_quantum < 32 ? 32 : PORT_BITS_run_quantum;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_quantum-1:0] source_run_quantum = SOURCE_BITS_run_quantum'(11);
always @(posedge clk) source_run_quantum <= {source_run_quantum[SOURCE_BITS_run_quantum-2:0],source_run_quantum[SOURCE_BITS_run_quantum-1]^source_run_quantum[SOURCE_BITS_run_quantum-2]};
wire [31:0] run_quantum = PORT_BITS_run_quantum'(source_run_quantum);
localparam integer PORT_BITS_run_chunk_frames = (31)+1;
localparam integer SOURCE_BITS_run_chunk_frames = PORT_BITS_run_chunk_frames < 32 ? 32 : PORT_BITS_run_chunk_frames;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_chunk_frames-1:0] source_run_chunk_frames = SOURCE_BITS_run_chunk_frames'(12);
always @(posedge clk) source_run_chunk_frames <= {source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-2:0],source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-1]^source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-2]};
wire [31:0] run_chunk_frames = PORT_BITS_run_chunk_frames'(source_run_chunk_frames);
localparam integer PORT_BITS_run_buffers = (31)+1;
localparam integer SOURCE_BITS_run_buffers = PORT_BITS_run_buffers < 32 ? 32 : PORT_BITS_run_buffers;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_buffers-1:0] source_run_buffers = SOURCE_BITS_run_buffers'(13);
always @(posedge clk) source_run_buffers <= {source_run_buffers[SOURCE_BITS_run_buffers-2:0],source_run_buffers[SOURCE_BITS_run_buffers-1]^source_run_buffers[SOURCE_BITS_run_buffers-2]};
wire [31:0] run_buffers = PORT_BITS_run_buffers'(source_run_buffers);
localparam integer PORT_BITS_run_inputs = (31)+1;
localparam integer SOURCE_BITS_run_inputs = PORT_BITS_run_inputs < 32 ? 32 : PORT_BITS_run_inputs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_inputs-1:0] source_run_inputs = SOURCE_BITS_run_inputs'(14);
always @(posedge clk) source_run_inputs <= {source_run_inputs[SOURCE_BITS_run_inputs-2:0],source_run_inputs[SOURCE_BITS_run_inputs-1]^source_run_inputs[SOURCE_BITS_run_inputs-2]};
wire [31:0] run_inputs = PORT_BITS_run_inputs'(source_run_inputs);
localparam integer PORT_BITS_run_outputs = (31)+1;
localparam integer SOURCE_BITS_run_outputs = PORT_BITS_run_outputs < 32 ? 32 : PORT_BITS_run_outputs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_outputs-1:0] source_run_outputs = SOURCE_BITS_run_outputs'(15);
always @(posedge clk) source_run_outputs <= {source_run_outputs[SOURCE_BITS_run_outputs-2:0],source_run_outputs[SOURCE_BITS_run_outputs-1]^source_run_outputs[SOURCE_BITS_run_outputs-2]};
wire [31:0] run_outputs = PORT_BITS_run_outputs'(source_run_outputs);
localparam integer PORT_BITS_run_jobs = (31)+1;
localparam integer SOURCE_BITS_run_jobs = PORT_BITS_run_jobs < 32 ? 32 : PORT_BITS_run_jobs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_jobs-1:0] source_run_jobs = SOURCE_BITS_run_jobs'(16);
always @(posedge clk) source_run_jobs <= {source_run_jobs[SOURCE_BITS_run_jobs-2:0],source_run_jobs[SOURCE_BITS_run_jobs-1]^source_run_jobs[SOURCE_BITS_run_jobs-2]};
wire [31:0] run_jobs = PORT_BITS_run_jobs'(source_run_jobs);
wire  busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_busy;
always @(posedge clk) unused_capture_busy <= busy;
wire  done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_done;
always @(posedge clk) unused_capture_done <= done;
wire [7:0] error_code;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [7:0] unused_capture_error_code;
always @(posedge clk) unused_capture_error_code <= error_code;
wire [BUFFERS-1:0] buffer_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [BUFFERS-1:0] unused_capture_buffer_ready;
always @(posedge clk) unused_capture_buffer_ready <= buffer_ready;
localparam integer PORT_BITS_job_valid = 1;
localparam integer SOURCE_BITS_job_valid = PORT_BITS_job_valid < 32 ? 32 : PORT_BITS_job_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_job_valid-1:0] source_job_valid = SOURCE_BITS_job_valid'(21);
always @(posedge clk) source_job_valid <= {source_job_valid[SOURCE_BITS_job_valid-2:0],source_job_valid[SOURCE_BITS_job_valid-1]^source_job_valid[SOURCE_BITS_job_valid-2]};
wire  job_valid = PORT_BITS_job_valid'(source_job_valid);
wire  job_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_job_ready;
always @(posedge clk) unused_capture_job_ready <= job_ready;
localparam integer PORT_BITS_job_enable = 1;
localparam integer SOURCE_BITS_job_enable = PORT_BITS_job_enable < 32 ? 32 : PORT_BITS_job_enable;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_job_enable-1:0] source_job_enable = SOURCE_BITS_job_enable'(23);
always @(posedge clk) source_job_enable <= {source_job_enable[SOURCE_BITS_job_enable-2:0],source_job_enable[SOURCE_BITS_job_enable-1]^source_job_enable[SOURCE_BITS_job_enable-2]};
wire  job_enable = PORT_BITS_job_enable'(source_job_enable);
localparam integer PORT_BITS_job_data = (`SHOOP_SP_JOB_BITS-1)+1;
localparam integer SOURCE_BITS_job_data = PORT_BITS_job_data < 32 ? 32 : PORT_BITS_job_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_job_data-1:0] source_job_data = SOURCE_BITS_job_data'(24);
always @(posedge clk) source_job_data <= {source_job_data[SOURCE_BITS_job_data-2:0],source_job_data[SOURCE_BITS_job_data-1]^source_job_data[SOURCE_BITS_job_data-2]};
wire [`SHOOP_SP_JOB_BITS-1:0] job_data = PORT_BITS_job_data'(source_job_data);
localparam integer PORT_BITS_cfg_valid = 1;
localparam integer SOURCE_BITS_cfg_valid = PORT_BITS_cfg_valid < 32 ? 32 : PORT_BITS_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_valid-1:0] source_cfg_valid = SOURCE_BITS_cfg_valid'(25);
always @(posedge clk) source_cfg_valid <= {source_cfg_valid[SOURCE_BITS_cfg_valid-2:0],source_cfg_valid[SOURCE_BITS_cfg_valid-1]^source_cfg_valid[SOURCE_BITS_cfg_valid-2]};
wire  cfg_valid = PORT_BITS_cfg_valid'(source_cfg_valid);
wire  cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_ready;
always @(posedge clk) unused_capture_cfg_ready <= cfg_ready;
localparam integer PORT_BITS_cfg_space = (1)+1;
localparam integer SOURCE_BITS_cfg_space = PORT_BITS_cfg_space < 32 ? 32 : PORT_BITS_cfg_space;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_space-1:0] source_cfg_space = SOURCE_BITS_cfg_space'(27);
always @(posedge clk) source_cfg_space <= {source_cfg_space[SOURCE_BITS_cfg_space-2:0],source_cfg_space[SOURCE_BITS_cfg_space-1]^source_cfg_space[SOURCE_BITS_cfg_space-2]};
wire [1:0] cfg_space = PORT_BITS_cfg_space'(source_cfg_space);
localparam integer PORT_BITS_cfg_write = 1;
localparam integer SOURCE_BITS_cfg_write = PORT_BITS_cfg_write < 32 ? 32 : PORT_BITS_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write-1:0] source_cfg_write = SOURCE_BITS_cfg_write'(28);
always @(posedge clk) source_cfg_write <= {source_cfg_write[SOURCE_BITS_cfg_write-2:0],source_cfg_write[SOURCE_BITS_cfg_write-1]^source_cfg_write[SOURCE_BITS_cfg_write-2]};
wire  cfg_write = PORT_BITS_cfg_write'(source_cfg_write);
localparam integer PORT_BITS_cfg_address = (31)+1;
localparam integer SOURCE_BITS_cfg_address = PORT_BITS_cfg_address < 32 ? 32 : PORT_BITS_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_address-1:0] source_cfg_address = SOURCE_BITS_cfg_address'(29);
always @(posedge clk) source_cfg_address <= {source_cfg_address[SOURCE_BITS_cfg_address-2:0],source_cfg_address[SOURCE_BITS_cfg_address-1]^source_cfg_address[SOURCE_BITS_cfg_address-2]};
wire [31:0] cfg_address = PORT_BITS_cfg_address'(source_cfg_address);
localparam integer PORT_BITS_cfg_write_data = (31)+1;
localparam integer SOURCE_BITS_cfg_write_data = PORT_BITS_cfg_write_data < 32 ? 32 : PORT_BITS_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write_data-1:0] source_cfg_write_data = SOURCE_BITS_cfg_write_data'(30);
always @(posedge clk) source_cfg_write_data <= {source_cfg_write_data[SOURCE_BITS_cfg_write_data-2:0],source_cfg_write_data[SOURCE_BITS_cfg_write_data-1]^source_cfg_write_data[SOURCE_BITS_cfg_write_data-2]};
wire [31:0] cfg_write_data = PORT_BITS_cfg_write_data'(source_cfg_write_data);
wire  cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_response_valid;
always @(posedge clk) unused_capture_cfg_response_valid <= cfg_response_valid;
wire  cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_response_error;
always @(posedge clk) unused_capture_cfg_response_error <= cfg_response_error;
wire [31:0] cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_cfg_read_data;
always @(posedge clk) unused_capture_cfg_read_data <= cfg_read_data;
wire  media_cmd_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_media_cmd_valid;
always @(posedge clk) unused_capture_media_cmd_valid <= media_cmd_valid;
localparam integer PORT_BITS_media_cmd_ready = 1;
localparam integer SOURCE_BITS_media_cmd_ready = PORT_BITS_media_cmd_ready < 32 ? 32 : PORT_BITS_media_cmd_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_cmd_ready-1:0] source_media_cmd_ready = SOURCE_BITS_media_cmd_ready'(35);
always @(posedge clk) source_media_cmd_ready <= {source_media_cmd_ready[SOURCE_BITS_media_cmd_ready-2:0],source_media_cmd_ready[SOURCE_BITS_media_cmd_ready-1]^source_media_cmd_ready[SOURCE_BITS_media_cmd_ready-2]};
wire  media_cmd_ready = PORT_BITS_media_cmd_ready'(source_media_cmd_ready);
wire [3:0] media_cmd_opcode;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] unused_capture_media_cmd_opcode;
always @(posedge clk) unused_capture_media_cmd_opcode <= media_cmd_opcode;
wire [63:0] media_cmd_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_media_cmd_view;
always @(posedge clk) unused_capture_media_cmd_view <= media_cmd_view;
wire [63:0] media_cmd_start;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_media_cmd_start;
always @(posedge clk) unused_capture_media_cmd_start <= media_cmd_start;
wire [63:0] media_cmd_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_media_cmd_length;
always @(posedge clk) unused_capture_media_cmd_length <= media_cmd_length;
wire [31:0] media_cmd_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_media_cmd_count;
always @(posedge clk) unused_capture_media_cmd_count <= media_cmd_count;
localparam integer PORT_BITS_media_done = 1;
localparam integer SOURCE_BITS_media_done = PORT_BITS_media_done < 32 ? 32 : PORT_BITS_media_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_done-1:0] source_media_done = SOURCE_BITS_media_done'(41);
always @(posedge clk) source_media_done <= {source_media_done[SOURCE_BITS_media_done-2:0],source_media_done[SOURCE_BITS_media_done-1]^source_media_done[SOURCE_BITS_media_done-2]};
wire  media_done = PORT_BITS_media_done'(source_media_done);
localparam integer PORT_BITS_media_error = (7)+1;
localparam integer SOURCE_BITS_media_error = PORT_BITS_media_error < 32 ? 32 : PORT_BITS_media_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_error-1:0] source_media_error = SOURCE_BITS_media_error'(42);
always @(posedge clk) source_media_error <= {source_media_error[SOURCE_BITS_media_error-2:0],source_media_error[SOURCE_BITS_media_error-1]^source_media_error[SOURCE_BITS_media_error-2]};
wire [7:0] media_error = PORT_BITS_media_error'(source_media_error);
localparam integer PORT_BITS_media_fault = 1;
localparam integer SOURCE_BITS_media_fault = PORT_BITS_media_fault < 32 ? 32 : PORT_BITS_media_fault;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_fault-1:0] source_media_fault = SOURCE_BITS_media_fault'(43);
always @(posedge clk) source_media_fault <= {source_media_fault[SOURCE_BITS_media_fault-2:0],source_media_fault[SOURCE_BITS_media_fault-1]^source_media_fault[SOURCE_BITS_media_fault-2]};
wire  media_fault = PORT_BITS_media_fault'(source_media_fault);
wire  media_write_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_media_write_valid;
always @(posedge clk) unused_capture_media_write_valid <= media_write_valid;
localparam integer PORT_BITS_media_write_ready = 1;
localparam integer SOURCE_BITS_media_write_ready = PORT_BITS_media_write_ready < 32 ? 32 : PORT_BITS_media_write_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_write_ready-1:0] source_media_write_ready = SOURCE_BITS_media_write_ready'(45);
always @(posedge clk) source_media_write_ready <= {source_media_write_ready[SOURCE_BITS_media_write_ready-2:0],source_media_write_ready[SOURCE_BITS_media_write_ready-1]^source_media_write_ready[SOURCE_BITS_media_write_ready-2]};
wire  media_write_ready = PORT_BITS_media_write_ready'(source_media_write_ready);
wire [31:0] media_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_media_write_data;
always @(posedge clk) unused_capture_media_write_data <= media_write_data;
localparam integer PORT_BITS_media_read_valid = 1;
localparam integer SOURCE_BITS_media_read_valid = PORT_BITS_media_read_valid < 32 ? 32 : PORT_BITS_media_read_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_read_valid-1:0] source_media_read_valid = SOURCE_BITS_media_read_valid'(47);
always @(posedge clk) source_media_read_valid <= {source_media_read_valid[SOURCE_BITS_media_read_valid-2:0],source_media_read_valid[SOURCE_BITS_media_read_valid-1]^source_media_read_valid[SOURCE_BITS_media_read_valid-2]};
wire  media_read_valid = PORT_BITS_media_read_valid'(source_media_read_valid);
wire  media_read_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_media_read_ready;
always @(posedge clk) unused_capture_media_read_ready <= media_read_ready;
localparam integer PORT_BITS_media_read_data = (31)+1;
localparam integer SOURCE_BITS_media_read_data = PORT_BITS_media_read_data < 32 ? 32 : PORT_BITS_media_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_read_data-1:0] source_media_read_data = SOURCE_BITS_media_read_data'(49);
always @(posedge clk) source_media_read_data <= {source_media_read_data[SOURCE_BITS_media_read_data-2:0],source_media_read_data[SOURCE_BITS_media_read_data-1]^source_media_read_data[SOURCE_BITS_media_read_data-2]};
wire [31:0] media_read_data = PORT_BITS_media_read_data'(source_media_read_data);
wire  media_hint_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_media_hint_valid;
always @(posedge clk) unused_capture_media_hint_valid <= media_hint_valid;
localparam integer PORT_BITS_media_hint_ready = 1;
localparam integer SOURCE_BITS_media_hint_ready = PORT_BITS_media_hint_ready < 32 ? 32 : PORT_BITS_media_hint_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_media_hint_ready-1:0] source_media_hint_ready = SOURCE_BITS_media_hint_ready'(51);
always @(posedge clk) source_media_hint_ready <= {source_media_hint_ready[SOURCE_BITS_media_hint_ready-2:0],source_media_hint_ready[SOURCE_BITS_media_hint_ready-1]^source_media_hint_ready[SOURCE_BITS_media_hint_ready-2]};
wire  media_hint_ready = PORT_BITS_media_hint_ready'(source_media_hint_ready);
wire [63:0] media_hint_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_media_hint_view;
always @(posedge clk) unused_capture_media_hint_view <= media_hint_view;
wire [63:0] media_hint_position;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_media_hint_position;
always @(posedge clk) unused_capture_media_hint_position <= media_hint_position;
wire [63:0] media_hint_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_media_hint_length;
always @(posedge clk) unused_capture_media_hint_length <= media_hint_length;
(* KEEP_HIERARCHY = "yes" *) job_processor #(
.EXTERNAL_IO(EXTERNAL_IO),
.MAX_FRAMES(MAX_FRAMES),
.BUFFERS(BUFFERS),
.INPUT_PORTS(INPUT_PORTS),
.OUTPUT_PORTS(OUTPUT_PORTS),
.CHUNK_FRAMES(CHUNK_FRAMES),
.ENABLE_MEDIA(ENABLE_MEDIA),
.BUFFER_BITS(BUFFER_BITS),
.SCRATCH_BITS(SCRATCH_BITS),
.INPUT_BITS(INPUT_BITS),
.OUTPUT_BITS(OUTPUT_BITS)
) core_under_test (
.sample_input_read(sample_input_read),
.sample_input_address(sample_input_address),
.sample_input_data(sample_input_data),
.sample_output_write(sample_output_write),
.sample_output_address(sample_output_address),
.sample_output_data(sample_output_data),
.clk(clk),
.reset(reset),
.run_valid(run_valid),
.run_ready(run_ready),
.run_quantum(run_quantum),
.run_chunk_frames(run_chunk_frames),
.run_buffers(run_buffers),
.run_inputs(run_inputs),
.run_outputs(run_outputs),
.run_jobs(run_jobs),
.busy(busy),
.done(done),
.error_code(error_code),
.buffer_ready(buffer_ready),
.job_valid(job_valid),
.job_ready(job_ready),
.job_enable(job_enable),
.job_data(job_data),
.cfg_valid(cfg_valid),
.cfg_ready(cfg_ready),
.cfg_space(cfg_space),
.cfg_write(cfg_write),
.cfg_address(cfg_address),
.cfg_write_data(cfg_write_data),
.cfg_response_valid(cfg_response_valid),
.cfg_response_error(cfg_response_error),
.cfg_read_data(cfg_read_data),
.media_cmd_valid(media_cmd_valid),
.media_cmd_ready(media_cmd_ready),
.media_cmd_opcode(media_cmd_opcode),
.media_cmd_view(media_cmd_view),
.media_cmd_start(media_cmd_start),
.media_cmd_length(media_cmd_length),
.media_cmd_count(media_cmd_count),
.media_done(media_done),
.media_error(media_error),
.media_fault(media_fault),
.media_write_valid(media_write_valid),
.media_write_ready(media_write_ready),
.media_write_data(media_write_data),
.media_read_valid(media_read_valid),
.media_read_ready(media_read_ready),
.media_read_data(media_read_data),
.media_hint_valid(media_hint_valid),
.media_hint_ready(media_hint_ready),
.media_hint_view(media_hint_view),
.media_hint_position(media_hint_position),
.media_hint_length(media_hint_length)
);
endmodule
