`include "state_processor_defs.svh"
module registered_core_context #(

    parameter integer LOOP_DEPTH = 8,
    parameter integer REQUEST_DEPTH = 16,
    parameter integer EVENT_DEPTH = 32,
    parameter integer PROGRAM_DEPTH = 64,
    parameter integer STATE_DEPTH = 32,
    parameter integer JOB_DEPTH = 64,
    parameter integer EXTERNAL_IO = 0,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer REUSE_VIEWS = 0,
    parameter integer CAPTURE_CHANNELS = 0,
    parameter integer VIEWS = 32,
    parameter integer SLOTS = 1024,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer MEMORY_RESPONSE_CYCLES = 1,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer RETIRED_TABLES = 256,
    parameter integer RECLAIM_BLOCKS = 0, parameter integer RECLAIM_CYCLES = 5000,
    parameter integer RECLAIM_GUARD = 1024,
    parameter integer TABLE_PAGES = (SLOTS/BLOCK_CHUNKS+14)/15 + 8*VIEWS + 16,
    parameter integer TABLE_LINES = 8, parameter integer REF_LINES = 4, parameter integer STACK_LINES = 2,
    parameter integer READ_BUFFERS = 4, parameter integer WRITE_BUFFERS = 4, parameter integer BURST_MEMORY = 1,
    parameter integer AXI_DATA_WIDTH = 32,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000

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
localparam integer PORT_BITS_maintenance_enable = 1;
localparam integer SOURCE_BITS_maintenance_enable = PORT_BITS_maintenance_enable < 32 ? 32 : PORT_BITS_maintenance_enable;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_maintenance_enable-1:0] source_maintenance_enable = SOURCE_BITS_maintenance_enable'(9);
always @(posedge clk) source_maintenance_enable <= {source_maintenance_enable[SOURCE_BITS_maintenance_enable-2:0],source_maintenance_enable[SOURCE_BITS_maintenance_enable-1]^source_maintenance_enable[SOURCE_BITS_maintenance_enable-2]};
wire  maintenance_enable = PORT_BITS_maintenance_enable'(source_maintenance_enable);
wire  cut_snapshot_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cut_snapshot_valid;
always @(posedge clk) unused_capture_cut_snapshot_valid <= cut_snapshot_valid;
wire [1727:0] cut_snapshot_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1727:0] unused_capture_cut_snapshot_data;
always @(posedge clk) unused_capture_cut_snapshot_data <= cut_snapshot_data;
wire [31:0] reclaim_pending;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_reclaim_pending;
always @(posedge clk) unused_capture_reclaim_pending <= reclaim_pending;
wire [31:0] reclaim_high_water;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_reclaim_high_water;
always @(posedge clk) unused_capture_reclaim_high_water <= reclaim_high_water;
wire [63:0] reclaim_references;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_reclaim_references;
always @(posedge clk) unused_capture_reclaim_references <= reclaim_references;
wire  reclaim_active;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_reclaim_active;
always @(posedge clk) unused_capture_reclaim_active <= reclaim_active;
wire  reclaim_released;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_reclaim_released;
always @(posedge clk) unused_capture_reclaim_released <= reclaim_released;
wire [31:0] reclaim_block;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_reclaim_block;
always @(posedge clk) unused_capture_reclaim_block <= reclaim_block;
wire [63:0] reclaim_blocks;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_reclaim_blocks;
always @(posedge clk) unused_capture_reclaim_blocks <= reclaim_blocks;
wire [63:0] reclaim_freed;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_reclaim_freed;
always @(posedge clk) unused_capture_reclaim_freed <= reclaim_freed;
wire [63:0] reclaim_cycles;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_reclaim_cycles;
always @(posedge clk) unused_capture_reclaim_cycles <= reclaim_cycles;
wire  reclaim_quiescent;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_reclaim_quiescent;
always @(posedge clk) unused_capture_reclaim_quiescent <= reclaim_quiescent;
wire [31:0] reclaim_pages_used;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_reclaim_pages_used;
always @(posedge clk) unused_capture_reclaim_pages_used <= reclaim_pages_used;
wire [31:0] reclaim_pages_available;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_reclaim_pages_available;
always @(posedge clk) unused_capture_reclaim_pages_available <= reclaim_pages_available;
wire [63:0] reclaim_pages_freed;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_reclaim_pages_freed;
always @(posedge clk) unused_capture_reclaim_pages_freed <= reclaim_pages_freed;
wire [63:0] reclaim_capacity_failures;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] unused_capture_reclaim_capacity_failures;
always @(posedge clk) unused_capture_reclaim_capacity_failures <= reclaim_capacity_failures;
localparam integer PORT_BITS_run_valid = 1;
localparam integer SOURCE_BITS_run_valid = PORT_BITS_run_valid < 32 ? 32 : PORT_BITS_run_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_valid-1:0] source_run_valid = SOURCE_BITS_run_valid'(26);
always @(posedge clk) source_run_valid <= {source_run_valid[SOURCE_BITS_run_valid-2:0],source_run_valid[SOURCE_BITS_run_valid-1]^source_run_valid[SOURCE_BITS_run_valid-2]};
wire  run_valid = PORT_BITS_run_valid'(source_run_valid);
wire  run_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_run_ready;
always @(posedge clk) unused_capture_run_ready <= run_ready;
localparam integer PORT_BITS_run_instruction_count = (31)+1;
localparam integer SOURCE_BITS_run_instruction_count = PORT_BITS_run_instruction_count < 32 ? 32 : PORT_BITS_run_instruction_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_instruction_count-1:0] source_run_instruction_count = SOURCE_BITS_run_instruction_count'(28);
always @(posedge clk) source_run_instruction_count <= {source_run_instruction_count[SOURCE_BITS_run_instruction_count-2:0],source_run_instruction_count[SOURCE_BITS_run_instruction_count-1]^source_run_instruction_count[SOURCE_BITS_run_instruction_count-2]};
wire [31:0] run_instruction_count = PORT_BITS_run_instruction_count'(source_run_instruction_count);
localparam integer PORT_BITS_run_state_count = (31)+1;
localparam integer SOURCE_BITS_run_state_count = PORT_BITS_run_state_count < 32 ? 32 : PORT_BITS_run_state_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_state_count-1:0] source_run_state_count = SOURCE_BITS_run_state_count'(29);
always @(posedge clk) source_run_state_count <= {source_run_state_count[SOURCE_BITS_run_state_count-2:0],source_run_state_count[SOURCE_BITS_run_state_count-1]^source_run_state_count[SOURCE_BITS_run_state_count-2]};
wire [31:0] run_state_count = PORT_BITS_run_state_count'(source_run_state_count);
localparam integer PORT_BITS_run_quantum = (31)+1;
localparam integer SOURCE_BITS_run_quantum = PORT_BITS_run_quantum < 32 ? 32 : PORT_BITS_run_quantum;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_quantum-1:0] source_run_quantum = SOURCE_BITS_run_quantum'(30);
always @(posedge clk) source_run_quantum <= {source_run_quantum[SOURCE_BITS_run_quantum-2:0],source_run_quantum[SOURCE_BITS_run_quantum-1]^source_run_quantum[SOURCE_BITS_run_quantum-2]};
wire [31:0] run_quantum = PORT_BITS_run_quantum'(source_run_quantum);
localparam integer PORT_BITS_run_chunk_frames = (31)+1;
localparam integer SOURCE_BITS_run_chunk_frames = PORT_BITS_run_chunk_frames < 32 ? 32 : PORT_BITS_run_chunk_frames;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_chunk_frames-1:0] source_run_chunk_frames = SOURCE_BITS_run_chunk_frames'(31);
always @(posedge clk) source_run_chunk_frames <= {source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-2:0],source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-1]^source_run_chunk_frames[SOURCE_BITS_run_chunk_frames-2]};
wire [31:0] run_chunk_frames = PORT_BITS_run_chunk_frames'(source_run_chunk_frames);
localparam integer PORT_BITS_run_job_budget = (31)+1;
localparam integer SOURCE_BITS_run_job_budget = PORT_BITS_run_job_budget < 32 ? 32 : PORT_BITS_run_job_budget;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_job_budget-1:0] source_run_job_budget = SOURCE_BITS_run_job_budget'(32);
always @(posedge clk) source_run_job_budget <= {source_run_job_budget[SOURCE_BITS_run_job_budget-2:0],source_run_job_budget[SOURCE_BITS_run_job_budget-1]^source_run_job_budget[SOURCE_BITS_run_job_budget-2]};
wire [31:0] run_job_budget = PORT_BITS_run_job_budget'(source_run_job_budget);
localparam integer PORT_BITS_run_buffers = (31)+1;
localparam integer SOURCE_BITS_run_buffers = PORT_BITS_run_buffers < 32 ? 32 : PORT_BITS_run_buffers;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_buffers-1:0] source_run_buffers = SOURCE_BITS_run_buffers'(33);
always @(posedge clk) source_run_buffers <= {source_run_buffers[SOURCE_BITS_run_buffers-2:0],source_run_buffers[SOURCE_BITS_run_buffers-1]^source_run_buffers[SOURCE_BITS_run_buffers-2]};
wire [31:0] run_buffers = PORT_BITS_run_buffers'(source_run_buffers);
localparam integer PORT_BITS_run_inputs = (31)+1;
localparam integer SOURCE_BITS_run_inputs = PORT_BITS_run_inputs < 32 ? 32 : PORT_BITS_run_inputs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_inputs-1:0] source_run_inputs = SOURCE_BITS_run_inputs'(34);
always @(posedge clk) source_run_inputs <= {source_run_inputs[SOURCE_BITS_run_inputs-2:0],source_run_inputs[SOURCE_BITS_run_inputs-1]^source_run_inputs[SOURCE_BITS_run_inputs-2]};
wire [31:0] run_inputs = PORT_BITS_run_inputs'(source_run_inputs);
localparam integer PORT_BITS_run_outputs = (31)+1;
localparam integer SOURCE_BITS_run_outputs = PORT_BITS_run_outputs < 32 ? 32 : PORT_BITS_run_outputs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_outputs-1:0] source_run_outputs = SOURCE_BITS_run_outputs'(35);
always @(posedge clk) source_run_outputs <= {source_run_outputs[SOURCE_BITS_run_outputs-2:0],source_run_outputs[SOURCE_BITS_run_outputs-1]^source_run_outputs[SOURCE_BITS_run_outputs-2]};
wire [31:0] run_outputs = PORT_BITS_run_outputs'(source_run_outputs);
localparam integer PORT_BITS_job_enable = 1;
localparam integer SOURCE_BITS_job_enable = PORT_BITS_job_enable < 32 ? 32 : PORT_BITS_job_enable;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_job_enable-1:0] source_job_enable = SOURCE_BITS_job_enable'(36);
always @(posedge clk) source_job_enable <= {source_job_enable[SOURCE_BITS_job_enable-2:0],source_job_enable[SOURCE_BITS_job_enable-1]^source_job_enable[SOURCE_BITS_job_enable-2]};
wire  job_enable = PORT_BITS_job_enable'(source_job_enable);
wire  busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_busy;
always @(posedge clk) unused_capture_busy <= busy;
wire  faulted;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_faulted;
always @(posedge clk) unused_capture_faulted <= faulted;
wire  done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_done;
always @(posedge clk) unused_capture_done <= done;
wire [7:0] error_code;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [7:0] unused_capture_error_code;
always @(posedge clk) unused_capture_error_code <= error_code;
wire [1:0] error_stage;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] unused_capture_error_stage;
always @(posedge clk) unused_capture_error_stage <= error_stage;
wire  state_busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_state_busy;
always @(posedge clk) unused_capture_state_busy <= state_busy;
wire  state_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_state_done;
always @(posedge clk) unused_capture_state_done <= state_done;
wire  jobs_busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_jobs_busy;
always @(posedge clk) unused_capture_jobs_busy <= jobs_busy;
wire  jobs_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_jobs_done;
always @(posedge clk) unused_capture_jobs_done <= jobs_done;
wire [BUFFERS-1:0] buffer_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [BUFFERS-1:0] unused_capture_buffer_ready;
always @(posedge clk) unused_capture_buffer_ready <= buffer_ready;
localparam integer PORT_BITS_cfg_valid = 1;
localparam integer SOURCE_BITS_cfg_valid = PORT_BITS_cfg_valid < 32 ? 32 : PORT_BITS_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_valid-1:0] source_cfg_valid = SOURCE_BITS_cfg_valid'(47);
always @(posedge clk) source_cfg_valid <= {source_cfg_valid[SOURCE_BITS_cfg_valid-2:0],source_cfg_valid[SOURCE_BITS_cfg_valid-1]^source_cfg_valid[SOURCE_BITS_cfg_valid-2]};
wire  cfg_valid = PORT_BITS_cfg_valid'(source_cfg_valid);
wire  cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_ready;
always @(posedge clk) unused_capture_cfg_ready <= cfg_ready;
localparam integer PORT_BITS_cfg_state = 1;
localparam integer SOURCE_BITS_cfg_state = PORT_BITS_cfg_state < 32 ? 32 : PORT_BITS_cfg_state;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_state-1:0] source_cfg_state = SOURCE_BITS_cfg_state'(49);
always @(posedge clk) source_cfg_state <= {source_cfg_state[SOURCE_BITS_cfg_state-2:0],source_cfg_state[SOURCE_BITS_cfg_state-1]^source_cfg_state[SOURCE_BITS_cfg_state-2]};
wire  cfg_state = PORT_BITS_cfg_state'(source_cfg_state);
localparam integer PORT_BITS_cfg_write = 1;
localparam integer SOURCE_BITS_cfg_write = PORT_BITS_cfg_write < 32 ? 32 : PORT_BITS_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write-1:0] source_cfg_write = SOURCE_BITS_cfg_write'(50);
always @(posedge clk) source_cfg_write <= {source_cfg_write[SOURCE_BITS_cfg_write-2:0],source_cfg_write[SOURCE_BITS_cfg_write-1]^source_cfg_write[SOURCE_BITS_cfg_write-2]};
wire  cfg_write = PORT_BITS_cfg_write'(source_cfg_write);
localparam integer PORT_BITS_cfg_address = (15)+1;
localparam integer SOURCE_BITS_cfg_address = PORT_BITS_cfg_address < 32 ? 32 : PORT_BITS_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_address-1:0] source_cfg_address = SOURCE_BITS_cfg_address'(51);
always @(posedge clk) source_cfg_address <= {source_cfg_address[SOURCE_BITS_cfg_address-2:0],source_cfg_address[SOURCE_BITS_cfg_address-1]^source_cfg_address[SOURCE_BITS_cfg_address-2]};
wire [15:0] cfg_address = PORT_BITS_cfg_address'(source_cfg_address);
localparam integer PORT_BITS_cfg_write_data = (`SHOOP_SP_STATE_BITS-1)+1;
localparam integer SOURCE_BITS_cfg_write_data = PORT_BITS_cfg_write_data < 32 ? 32 : PORT_BITS_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write_data-1:0] source_cfg_write_data = SOURCE_BITS_cfg_write_data'(52);
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
localparam integer PORT_BITS_audio_cfg_valid = 1;
localparam integer SOURCE_BITS_audio_cfg_valid = PORT_BITS_audio_cfg_valid < 32 ? 32 : PORT_BITS_audio_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_valid-1:0] source_audio_cfg_valid = SOURCE_BITS_audio_cfg_valid'(56);
always @(posedge clk) source_audio_cfg_valid <= {source_audio_cfg_valid[SOURCE_BITS_audio_cfg_valid-2:0],source_audio_cfg_valid[SOURCE_BITS_audio_cfg_valid-1]^source_audio_cfg_valid[SOURCE_BITS_audio_cfg_valid-2]};
wire  audio_cfg_valid = PORT_BITS_audio_cfg_valid'(source_audio_cfg_valid);
wire  audio_cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_audio_cfg_ready;
always @(posedge clk) unused_capture_audio_cfg_ready <= audio_cfg_ready;
localparam integer PORT_BITS_audio_cfg_space = (1)+1;
localparam integer SOURCE_BITS_audio_cfg_space = PORT_BITS_audio_cfg_space < 32 ? 32 : PORT_BITS_audio_cfg_space;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_space-1:0] source_audio_cfg_space = SOURCE_BITS_audio_cfg_space'(58);
always @(posedge clk) source_audio_cfg_space <= {source_audio_cfg_space[SOURCE_BITS_audio_cfg_space-2:0],source_audio_cfg_space[SOURCE_BITS_audio_cfg_space-1]^source_audio_cfg_space[SOURCE_BITS_audio_cfg_space-2]};
wire [1:0] audio_cfg_space = PORT_BITS_audio_cfg_space'(source_audio_cfg_space);
localparam integer PORT_BITS_audio_cfg_write = 1;
localparam integer SOURCE_BITS_audio_cfg_write = PORT_BITS_audio_cfg_write < 32 ? 32 : PORT_BITS_audio_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_write-1:0] source_audio_cfg_write = SOURCE_BITS_audio_cfg_write'(59);
always @(posedge clk) source_audio_cfg_write <= {source_audio_cfg_write[SOURCE_BITS_audio_cfg_write-2:0],source_audio_cfg_write[SOURCE_BITS_audio_cfg_write-1]^source_audio_cfg_write[SOURCE_BITS_audio_cfg_write-2]};
wire  audio_cfg_write = PORT_BITS_audio_cfg_write'(source_audio_cfg_write);
localparam integer PORT_BITS_audio_cfg_address = (31)+1;
localparam integer SOURCE_BITS_audio_cfg_address = PORT_BITS_audio_cfg_address < 32 ? 32 : PORT_BITS_audio_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_address-1:0] source_audio_cfg_address = SOURCE_BITS_audio_cfg_address'(60);
always @(posedge clk) source_audio_cfg_address <= {source_audio_cfg_address[SOURCE_BITS_audio_cfg_address-2:0],source_audio_cfg_address[SOURCE_BITS_audio_cfg_address-1]^source_audio_cfg_address[SOURCE_BITS_audio_cfg_address-2]};
wire [31:0] audio_cfg_address = PORT_BITS_audio_cfg_address'(source_audio_cfg_address);
localparam integer PORT_BITS_audio_cfg_write_data = (31)+1;
localparam integer SOURCE_BITS_audio_cfg_write_data = PORT_BITS_audio_cfg_write_data < 32 ? 32 : PORT_BITS_audio_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_write_data-1:0] source_audio_cfg_write_data = SOURCE_BITS_audio_cfg_write_data'(61);
always @(posedge clk) source_audio_cfg_write_data <= {source_audio_cfg_write_data[SOURCE_BITS_audio_cfg_write_data-2:0],source_audio_cfg_write_data[SOURCE_BITS_audio_cfg_write_data-1]^source_audio_cfg_write_data[SOURCE_BITS_audio_cfg_write_data-2]};
wire [31:0] audio_cfg_write_data = PORT_BITS_audio_cfg_write_data'(source_audio_cfg_write_data);
wire  audio_cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_audio_cfg_response_valid;
always @(posedge clk) unused_capture_audio_cfg_response_valid <= audio_cfg_response_valid;
wire  audio_cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_audio_cfg_response_error;
always @(posedge clk) unused_capture_audio_cfg_response_error <= audio_cfg_response_error;
wire [31:0] audio_cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_audio_cfg_read_data;
always @(posedge clk) unused_capture_audio_cfg_read_data <= audio_cfg_read_data;
wire  monitor_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_monitor_valid;
always @(posedge clk) unused_capture_monitor_valid <= monitor_valid;
wire  monitor_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_monitor_ready;
always @(posedge clk) unused_capture_monitor_ready <= monitor_ready;
wire [`SHOOP_SP_JOB_BITS-1:0] monitor_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [`SHOOP_SP_JOB_BITS-1:0] unused_capture_monitor_data;
always @(posedge clk) unused_capture_monitor_data <= monitor_data;
wire [31:0] queued_jobs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_queued_jobs;
always @(posedge clk) unused_capture_queued_jobs <= queued_jobs;
wire  drain_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_drain_valid;
always @(posedge clk) unused_capture_drain_valid <= drain_valid;
localparam integer PORT_BITS_drain_ready = 1;
localparam integer SOURCE_BITS_drain_ready = PORT_BITS_drain_ready < 32 ? 32 : PORT_BITS_drain_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_drain_ready-1:0] source_drain_ready = SOURCE_BITS_drain_ready'(70);
always @(posedge clk) source_drain_ready <= {source_drain_ready[SOURCE_BITS_drain_ready-2:0],source_drain_ready[SOURCE_BITS_drain_ready-1]^source_drain_ready[SOURCE_BITS_drain_ready-2]};
wire  drain_ready = PORT_BITS_drain_ready'(source_drain_ready);
localparam integer PORT_BITS_command_valid = 1;
localparam integer SOURCE_BITS_command_valid = PORT_BITS_command_valid < 32 ? 32 : PORT_BITS_command_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_valid-1:0] source_command_valid = SOURCE_BITS_command_valid'(71);
always @(posedge clk) source_command_valid <= {source_command_valid[SOURCE_BITS_command_valid-2:0],source_command_valid[SOURCE_BITS_command_valid-1]^source_command_valid[SOURCE_BITS_command_valid-2]};
wire  command_valid = PORT_BITS_command_valid'(source_command_valid);
wire  command_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_command_ready;
always @(posedge clk) unused_capture_command_ready <= command_ready;
localparam integer PORT_BITS_command_opcode = (7)+1;
localparam integer SOURCE_BITS_command_opcode = PORT_BITS_command_opcode < 32 ? 32 : PORT_BITS_command_opcode;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_opcode-1:0] source_command_opcode = SOURCE_BITS_command_opcode'(73);
always @(posedge clk) source_command_opcode <= {source_command_opcode[SOURCE_BITS_command_opcode-2:0],source_command_opcode[SOURCE_BITS_command_opcode-1]^source_command_opcode[SOURCE_BITS_command_opcode-2]};
wire [7:0] command_opcode = PORT_BITS_command_opcode'(source_command_opcode);
localparam integer PORT_BITS_command_index = (31)+1;
localparam integer SOURCE_BITS_command_index = PORT_BITS_command_index < 32 ? 32 : PORT_BITS_command_index;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_index-1:0] source_command_index = SOURCE_BITS_command_index'(74);
always @(posedge clk) source_command_index <= {source_command_index[SOURCE_BITS_command_index-2:0],source_command_index[SOURCE_BITS_command_index-1]^source_command_index[SOURCE_BITS_command_index-2]};
wire [31:0] command_index = PORT_BITS_command_index'(source_command_index);
localparam integer PORT_BITS_command_data = (639)+1;
localparam integer SOURCE_BITS_command_data = PORT_BITS_command_data < 32 ? 32 : PORT_BITS_command_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_data-1:0] source_command_data = SOURCE_BITS_command_data'(75);
always @(posedge clk) source_command_data <= {source_command_data[SOURCE_BITS_command_data-2:0],source_command_data[SOURCE_BITS_command_data-1]^source_command_data[SOURCE_BITS_command_data-2]};
wire [639:0] command_data = PORT_BITS_command_data'(source_command_data);
wire  response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_response_valid;
always @(posedge clk) unused_capture_response_valid <= response_valid;
localparam integer PORT_BITS_response_ready = 1;
localparam integer SOURCE_BITS_response_ready = PORT_BITS_response_ready < 32 ? 32 : PORT_BITS_response_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_response_ready-1:0] source_response_ready = SOURCE_BITS_response_ready'(77);
always @(posedge clk) source_response_ready <= {source_response_ready[SOURCE_BITS_response_ready-2:0],source_response_ready[SOURCE_BITS_response_ready-1]^source_response_ready[SOURCE_BITS_response_ready-2]};
wire  response_ready = PORT_BITS_response_ready'(source_response_ready);
wire [3:0] response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] unused_capture_response_error;
always @(posedge clk) unused_capture_response_error <= response_error;
wire [831:0] response_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [831:0] unused_capture_response_data;
always @(posedge clk) unused_capture_response_data <= response_data;
wire  transition_busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_transition_busy;
always @(posedge clk) unused_capture_transition_busy <= transition_busy;
localparam integer PORT_BITS_arena_base = (63)+1;
localparam integer SOURCE_BITS_arena_base = PORT_BITS_arena_base < 32 ? 32 : PORT_BITS_arena_base;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_arena_base-1:0] source_arena_base = SOURCE_BITS_arena_base'(81);
always @(posedge clk) source_arena_base <= {source_arena_base[SOURCE_BITS_arena_base-2:0],source_arena_base[SOURCE_BITS_arena_base-1]^source_arena_base[SOURCE_BITS_arena_base-2]};
wire [63:0] arena_base = PORT_BITS_arena_base'(source_arena_base);
localparam integer PORT_BITS_store_cmd_valid = 1;
localparam integer SOURCE_BITS_store_cmd_valid = PORT_BITS_store_cmd_valid < 32 ? 32 : PORT_BITS_store_cmd_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_valid-1:0] source_store_cmd_valid = SOURCE_BITS_store_cmd_valid'(82);
always @(posedge clk) source_store_cmd_valid <= {source_store_cmd_valid[SOURCE_BITS_store_cmd_valid-2:0],source_store_cmd_valid[SOURCE_BITS_store_cmd_valid-1]^source_store_cmd_valid[SOURCE_BITS_store_cmd_valid-2]};
wire  store_cmd_valid = PORT_BITS_store_cmd_valid'(source_store_cmd_valid);
wire  store_cmd_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_cmd_ready;
always @(posedge clk) unused_capture_store_cmd_ready <= store_cmd_ready;
localparam integer PORT_BITS_store_cmd_opcode = (3)+1;
localparam integer SOURCE_BITS_store_cmd_opcode = PORT_BITS_store_cmd_opcode < 32 ? 32 : PORT_BITS_store_cmd_opcode;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_opcode-1:0] source_store_cmd_opcode = SOURCE_BITS_store_cmd_opcode'(84);
always @(posedge clk) source_store_cmd_opcode <= {source_store_cmd_opcode[SOURCE_BITS_store_cmd_opcode-2:0],source_store_cmd_opcode[SOURCE_BITS_store_cmd_opcode-1]^source_store_cmd_opcode[SOURCE_BITS_store_cmd_opcode-2]};
wire [3:0] store_cmd_opcode = PORT_BITS_store_cmd_opcode'(source_store_cmd_opcode);
localparam integer PORT_BITS_store_cmd_view = (63)+1;
localparam integer SOURCE_BITS_store_cmd_view = PORT_BITS_store_cmd_view < 32 ? 32 : PORT_BITS_store_cmd_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_view-1:0] source_store_cmd_view = SOURCE_BITS_store_cmd_view'(85);
always @(posedge clk) source_store_cmd_view <= {source_store_cmd_view[SOURCE_BITS_store_cmd_view-2:0],source_store_cmd_view[SOURCE_BITS_store_cmd_view-1]^source_store_cmd_view[SOURCE_BITS_store_cmd_view-2]};
wire [63:0] store_cmd_view = PORT_BITS_store_cmd_view'(source_store_cmd_view);
localparam integer PORT_BITS_store_cmd_start = (63)+1;
localparam integer SOURCE_BITS_store_cmd_start = PORT_BITS_store_cmd_start < 32 ? 32 : PORT_BITS_store_cmd_start;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_start-1:0] source_store_cmd_start = SOURCE_BITS_store_cmd_start'(86);
always @(posedge clk) source_store_cmd_start <= {source_store_cmd_start[SOURCE_BITS_store_cmd_start-2:0],source_store_cmd_start[SOURCE_BITS_store_cmd_start-1]^source_store_cmd_start[SOURCE_BITS_store_cmd_start-2]};
wire [63:0] store_cmd_start = PORT_BITS_store_cmd_start'(source_store_cmd_start);
localparam integer PORT_BITS_store_cmd_length = (63)+1;
localparam integer SOURCE_BITS_store_cmd_length = PORT_BITS_store_cmd_length < 32 ? 32 : PORT_BITS_store_cmd_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_length-1:0] source_store_cmd_length = SOURCE_BITS_store_cmd_length'(87);
always @(posedge clk) source_store_cmd_length <= {source_store_cmd_length[SOURCE_BITS_store_cmd_length-2:0],source_store_cmd_length[SOURCE_BITS_store_cmd_length-1]^source_store_cmd_length[SOURCE_BITS_store_cmd_length-2]};
wire [63:0] store_cmd_length = PORT_BITS_store_cmd_length'(source_store_cmd_length);
localparam integer PORT_BITS_store_cmd_count = (31)+1;
localparam integer SOURCE_BITS_store_cmd_count = PORT_BITS_store_cmd_count < 32 ? 32 : PORT_BITS_store_cmd_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_count-1:0] source_store_cmd_count = SOURCE_BITS_store_cmd_count'(88);
always @(posedge clk) source_store_cmd_count <= {source_store_cmd_count[SOURCE_BITS_store_cmd_count-2:0],source_store_cmd_count[SOURCE_BITS_store_cmd_count-1]^source_store_cmd_count[SOURCE_BITS_store_cmd_count-2]};
wire [31:0] store_cmd_count = PORT_BITS_store_cmd_count'(source_store_cmd_count);
wire  store_busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_busy;
always @(posedge clk) unused_capture_store_busy <= store_busy;
wire  store_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_done;
always @(posedge clk) unused_capture_store_done <= store_done;
wire [7:0] store_error_code;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [7:0] unused_capture_store_error_code;
always @(posedge clk) unused_capture_store_error_code <= store_error_code;
wire [31:0] store_result_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_store_result_view;
always @(posedge clk) unused_capture_store_result_view <= store_result_view;
wire  store_result_empty;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_result_empty;
always @(posedge clk) unused_capture_store_result_empty <= store_result_empty;
localparam integer PORT_BITS_store_write_valid = 1;
localparam integer SOURCE_BITS_store_write_valid = PORT_BITS_store_write_valid < 32 ? 32 : PORT_BITS_store_write_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_write_valid-1:0] source_store_write_valid = SOURCE_BITS_store_write_valid'(94);
always @(posedge clk) source_store_write_valid <= {source_store_write_valid[SOURCE_BITS_store_write_valid-2:0],source_store_write_valid[SOURCE_BITS_store_write_valid-1]^source_store_write_valid[SOURCE_BITS_store_write_valid-2]};
wire  store_write_valid = PORT_BITS_store_write_valid'(source_store_write_valid);
wire  store_write_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_write_ready;
always @(posedge clk) unused_capture_store_write_ready <= store_write_ready;
localparam integer PORT_BITS_store_write_data = (31)+1;
localparam integer SOURCE_BITS_store_write_data = PORT_BITS_store_write_data < 32 ? 32 : PORT_BITS_store_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_write_data-1:0] source_store_write_data = SOURCE_BITS_store_write_data'(96);
always @(posedge clk) source_store_write_data <= {source_store_write_data[SOURCE_BITS_store_write_data-2:0],source_store_write_data[SOURCE_BITS_store_write_data-1]^source_store_write_data[SOURCE_BITS_store_write_data-2]};
wire [31:0] store_write_data = PORT_BITS_store_write_data'(source_store_write_data);
wire  store_read_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_read_valid;
always @(posedge clk) unused_capture_store_read_valid <= store_read_valid;
localparam integer PORT_BITS_store_read_ready = 1;
localparam integer SOURCE_BITS_store_read_ready = PORT_BITS_store_read_ready < 32 ? 32 : PORT_BITS_store_read_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_read_ready-1:0] source_store_read_ready = SOURCE_BITS_store_read_ready'(98);
always @(posedge clk) source_store_read_ready <= {source_store_read_ready[SOURCE_BITS_store_read_ready-2:0],source_store_read_ready[SOURCE_BITS_store_read_ready-1]^source_store_read_ready[SOURCE_BITS_store_read_ready-2]};
wire  store_read_ready = PORT_BITS_store_read_ready'(source_store_read_ready);
wire [31:0] store_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_store_read_data;
always @(posedge clk) unused_capture_store_read_data <= store_read_data;
localparam integer PORT_BITS_store_cfg_valid = 1;
localparam integer SOURCE_BITS_store_cfg_valid = PORT_BITS_store_cfg_valid < 32 ? 32 : PORT_BITS_store_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_valid-1:0] source_store_cfg_valid = SOURCE_BITS_store_cfg_valid'(100);
always @(posedge clk) source_store_cfg_valid <= {source_store_cfg_valid[SOURCE_BITS_store_cfg_valid-2:0],source_store_cfg_valid[SOURCE_BITS_store_cfg_valid-1]^source_store_cfg_valid[SOURCE_BITS_store_cfg_valid-2]};
wire  store_cfg_valid = PORT_BITS_store_cfg_valid'(source_store_cfg_valid);
wire  store_cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_cfg_ready;
always @(posedge clk) unused_capture_store_cfg_ready <= store_cfg_ready;
localparam integer PORT_BITS_store_cfg_write = 1;
localparam integer SOURCE_BITS_store_cfg_write = PORT_BITS_store_cfg_write < 32 ? 32 : PORT_BITS_store_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_write-1:0] source_store_cfg_write = SOURCE_BITS_store_cfg_write'(102);
always @(posedge clk) source_store_cfg_write <= {source_store_cfg_write[SOURCE_BITS_store_cfg_write-2:0],source_store_cfg_write[SOURCE_BITS_store_cfg_write-1]^source_store_cfg_write[SOURCE_BITS_store_cfg_write-2]};
wire  store_cfg_write = PORT_BITS_store_cfg_write'(source_store_cfg_write);
localparam integer PORT_BITS_store_cfg_space = (2)+1;
localparam integer SOURCE_BITS_store_cfg_space = PORT_BITS_store_cfg_space < 32 ? 32 : PORT_BITS_store_cfg_space;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_space-1:0] source_store_cfg_space = SOURCE_BITS_store_cfg_space'(103);
always @(posedge clk) source_store_cfg_space <= {source_store_cfg_space[SOURCE_BITS_store_cfg_space-2:0],source_store_cfg_space[SOURCE_BITS_store_cfg_space-1]^source_store_cfg_space[SOURCE_BITS_store_cfg_space-2]};
wire [2:0] store_cfg_space = PORT_BITS_store_cfg_space'(source_store_cfg_space);
localparam integer PORT_BITS_store_cfg_address = (31)+1;
localparam integer SOURCE_BITS_store_cfg_address = PORT_BITS_store_cfg_address < 32 ? 32 : PORT_BITS_store_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_address-1:0] source_store_cfg_address = SOURCE_BITS_store_cfg_address'(104);
always @(posedge clk) source_store_cfg_address <= {source_store_cfg_address[SOURCE_BITS_store_cfg_address-2:0],source_store_cfg_address[SOURCE_BITS_store_cfg_address-1]^source_store_cfg_address[SOURCE_BITS_store_cfg_address-2]};
wire [31:0] store_cfg_address = PORT_BITS_store_cfg_address'(source_store_cfg_address);
localparam integer PORT_BITS_store_cfg_write_data = (159)+1;
localparam integer SOURCE_BITS_store_cfg_write_data = PORT_BITS_store_cfg_write_data < 32 ? 32 : PORT_BITS_store_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_write_data-1:0] source_store_cfg_write_data = SOURCE_BITS_store_cfg_write_data'(105);
always @(posedge clk) source_store_cfg_write_data <= {source_store_cfg_write_data[SOURCE_BITS_store_cfg_write_data-2:0],source_store_cfg_write_data[SOURCE_BITS_store_cfg_write_data-1]^source_store_cfg_write_data[SOURCE_BITS_store_cfg_write_data-2]};
wire [159:0] store_cfg_write_data = PORT_BITS_store_cfg_write_data'(source_store_cfg_write_data);
wire  store_cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_cfg_response_valid;
always @(posedge clk) unused_capture_store_cfg_response_valid <= store_cfg_response_valid;
wire  store_cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_store_cfg_response_error;
always @(posedge clk) unused_capture_store_cfg_response_error <= store_cfg_response_error;
wire [159:0] store_cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [159:0] unused_capture_store_cfg_read_data;
always @(posedge clk) unused_capture_store_cfg_read_data <= store_cfg_read_data;
wire  memory_fault;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_memory_fault;
always @(posedge clk) unused_capture_memory_fault <= memory_fault;
wire  m_axi_awid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_awid;
always @(posedge clk) unused_capture_m_axi_awid <= m_axi_awid;
wire [31:0] m_axi_awaddr;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_m_axi_awaddr;
always @(posedge clk) unused_capture_m_axi_awaddr <= m_axi_awaddr;
wire [3:0] m_axi_awlen;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] unused_capture_m_axi_awlen;
always @(posedge clk) unused_capture_m_axi_awlen <= m_axi_awlen;
wire [2:0] m_axi_awsize;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [2:0] unused_capture_m_axi_awsize;
always @(posedge clk) unused_capture_m_axi_awsize <= m_axi_awsize;
wire [1:0] m_axi_awburst;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] unused_capture_m_axi_awburst;
always @(posedge clk) unused_capture_m_axi_awburst <= m_axi_awburst;
wire  m_axi_awvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_awvalid;
always @(posedge clk) unused_capture_m_axi_awvalid <= m_axi_awvalid;
localparam integer PORT_BITS_m_axi_awready = 1;
localparam integer SOURCE_BITS_m_axi_awready = PORT_BITS_m_axi_awready < 32 ? 32 : PORT_BITS_m_axi_awready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_awready-1:0] source_m_axi_awready = SOURCE_BITS_m_axi_awready'(116);
always @(posedge clk) source_m_axi_awready <= {source_m_axi_awready[SOURCE_BITS_m_axi_awready-2:0],source_m_axi_awready[SOURCE_BITS_m_axi_awready-1]^source_m_axi_awready[SOURCE_BITS_m_axi_awready-2]};
wire  m_axi_awready = PORT_BITS_m_axi_awready'(source_m_axi_awready);
wire [AXI_DATA_WIDTH-1:0] m_axi_wdata;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [AXI_DATA_WIDTH-1:0] unused_capture_m_axi_wdata;
always @(posedge clk) unused_capture_m_axi_wdata <= m_axi_wdata;
wire [AXI_DATA_WIDTH/8-1:0] m_axi_wstrb;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [AXI_DATA_WIDTH/8-1:0] unused_capture_m_axi_wstrb;
always @(posedge clk) unused_capture_m_axi_wstrb <= m_axi_wstrb;
wire  m_axi_wid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_wid;
always @(posedge clk) unused_capture_m_axi_wid <= m_axi_wid;
wire  m_axi_wlast;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_wlast;
always @(posedge clk) unused_capture_m_axi_wlast <= m_axi_wlast;
wire  m_axi_wvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_wvalid;
always @(posedge clk) unused_capture_m_axi_wvalid <= m_axi_wvalid;
localparam integer PORT_BITS_m_axi_wready = 1;
localparam integer SOURCE_BITS_m_axi_wready = PORT_BITS_m_axi_wready < 32 ? 32 : PORT_BITS_m_axi_wready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_wready-1:0] source_m_axi_wready = SOURCE_BITS_m_axi_wready'(122);
always @(posedge clk) source_m_axi_wready <= {source_m_axi_wready[SOURCE_BITS_m_axi_wready-2:0],source_m_axi_wready[SOURCE_BITS_m_axi_wready-1]^source_m_axi_wready[SOURCE_BITS_m_axi_wready-2]};
wire  m_axi_wready = PORT_BITS_m_axi_wready'(source_m_axi_wready);
localparam integer PORT_BITS_m_axi_bid = 1;
localparam integer SOURCE_BITS_m_axi_bid = PORT_BITS_m_axi_bid < 32 ? 32 : PORT_BITS_m_axi_bid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bid-1:0] source_m_axi_bid = SOURCE_BITS_m_axi_bid'(123);
always @(posedge clk) source_m_axi_bid <= {source_m_axi_bid[SOURCE_BITS_m_axi_bid-2:0],source_m_axi_bid[SOURCE_BITS_m_axi_bid-1]^source_m_axi_bid[SOURCE_BITS_m_axi_bid-2]};
wire  m_axi_bid = PORT_BITS_m_axi_bid'(source_m_axi_bid);
localparam integer PORT_BITS_m_axi_bresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_bresp = PORT_BITS_m_axi_bresp < 32 ? 32 : PORT_BITS_m_axi_bresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bresp-1:0] source_m_axi_bresp = SOURCE_BITS_m_axi_bresp'(124);
always @(posedge clk) source_m_axi_bresp <= {source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2:0],source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-1]^source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2]};
wire [1:0] m_axi_bresp = PORT_BITS_m_axi_bresp'(source_m_axi_bresp);
localparam integer PORT_BITS_m_axi_bvalid = 1;
localparam integer SOURCE_BITS_m_axi_bvalid = PORT_BITS_m_axi_bvalid < 32 ? 32 : PORT_BITS_m_axi_bvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bvalid-1:0] source_m_axi_bvalid = SOURCE_BITS_m_axi_bvalid'(125);
always @(posedge clk) source_m_axi_bvalid <= {source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-2:0],source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-1]^source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-2]};
wire  m_axi_bvalid = PORT_BITS_m_axi_bvalid'(source_m_axi_bvalid);
wire  m_axi_bready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_bready;
always @(posedge clk) unused_capture_m_axi_bready <= m_axi_bready;
wire  m_axi_arid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_arid;
always @(posedge clk) unused_capture_m_axi_arid <= m_axi_arid;
wire [31:0] m_axi_araddr;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_m_axi_araddr;
always @(posedge clk) unused_capture_m_axi_araddr <= m_axi_araddr;
wire [3:0] m_axi_arlen;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] unused_capture_m_axi_arlen;
always @(posedge clk) unused_capture_m_axi_arlen <= m_axi_arlen;
wire [2:0] m_axi_arsize;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [2:0] unused_capture_m_axi_arsize;
always @(posedge clk) unused_capture_m_axi_arsize <= m_axi_arsize;
wire [1:0] m_axi_arburst;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] unused_capture_m_axi_arburst;
always @(posedge clk) unused_capture_m_axi_arburst <= m_axi_arburst;
wire  m_axi_arvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_arvalid;
always @(posedge clk) unused_capture_m_axi_arvalid <= m_axi_arvalid;
localparam integer PORT_BITS_m_axi_arready = 1;
localparam integer SOURCE_BITS_m_axi_arready = PORT_BITS_m_axi_arready < 32 ? 32 : PORT_BITS_m_axi_arready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_arready-1:0] source_m_axi_arready = SOURCE_BITS_m_axi_arready'(133);
always @(posedge clk) source_m_axi_arready <= {source_m_axi_arready[SOURCE_BITS_m_axi_arready-2:0],source_m_axi_arready[SOURCE_BITS_m_axi_arready-1]^source_m_axi_arready[SOURCE_BITS_m_axi_arready-2]};
wire  m_axi_arready = PORT_BITS_m_axi_arready'(source_m_axi_arready);
localparam integer PORT_BITS_m_axi_rid = 1;
localparam integer SOURCE_BITS_m_axi_rid = PORT_BITS_m_axi_rid < 32 ? 32 : PORT_BITS_m_axi_rid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rid-1:0] source_m_axi_rid = SOURCE_BITS_m_axi_rid'(134);
always @(posedge clk) source_m_axi_rid <= {source_m_axi_rid[SOURCE_BITS_m_axi_rid-2:0],source_m_axi_rid[SOURCE_BITS_m_axi_rid-1]^source_m_axi_rid[SOURCE_BITS_m_axi_rid-2]};
wire  m_axi_rid = PORT_BITS_m_axi_rid'(source_m_axi_rid);
localparam integer PORT_BITS_m_axi_rdata = (AXI_DATA_WIDTH-1)+1;
localparam integer SOURCE_BITS_m_axi_rdata = PORT_BITS_m_axi_rdata < 32 ? 32 : PORT_BITS_m_axi_rdata;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rdata-1:0] source_m_axi_rdata = SOURCE_BITS_m_axi_rdata'(135);
always @(posedge clk) source_m_axi_rdata <= {source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2:0],source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-1]^source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2]};
wire [AXI_DATA_WIDTH-1:0] m_axi_rdata = PORT_BITS_m_axi_rdata'(source_m_axi_rdata);
localparam integer PORT_BITS_m_axi_rresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_rresp = PORT_BITS_m_axi_rresp < 32 ? 32 : PORT_BITS_m_axi_rresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rresp-1:0] source_m_axi_rresp = SOURCE_BITS_m_axi_rresp'(136);
always @(posedge clk) source_m_axi_rresp <= {source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2:0],source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-1]^source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2]};
wire [1:0] m_axi_rresp = PORT_BITS_m_axi_rresp'(source_m_axi_rresp);
localparam integer PORT_BITS_m_axi_rlast = 1;
localparam integer SOURCE_BITS_m_axi_rlast = PORT_BITS_m_axi_rlast < 32 ? 32 : PORT_BITS_m_axi_rlast;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rlast-1:0] source_m_axi_rlast = SOURCE_BITS_m_axi_rlast'(137);
always @(posedge clk) source_m_axi_rlast <= {source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2:0],source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-1]^source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2]};
wire  m_axi_rlast = PORT_BITS_m_axi_rlast'(source_m_axi_rlast);
localparam integer PORT_BITS_m_axi_rvalid = 1;
localparam integer SOURCE_BITS_m_axi_rvalid = PORT_BITS_m_axi_rvalid < 32 ? 32 : PORT_BITS_m_axi_rvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rvalid-1:0] source_m_axi_rvalid = SOURCE_BITS_m_axi_rvalid'(138);
always @(posedge clk) source_m_axi_rvalid <= {source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2:0],source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-1]^source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2]};
wire  m_axi_rvalid = PORT_BITS_m_axi_rvalid'(source_m_axi_rvalid);
wire  m_axi_rready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_rready;
always @(posedge clk) unused_capture_m_axi_rready <= m_axi_rready;
(* KEEP_HIERARCHY = "yes" *) loop_pipeline_axi #(
.LOOP_DEPTH(LOOP_DEPTH),
.REQUEST_DEPTH(REQUEST_DEPTH),
.EVENT_DEPTH(EVENT_DEPTH),
.PROGRAM_DEPTH(PROGRAM_DEPTH),
.STATE_DEPTH(STATE_DEPTH),
.JOB_DEPTH(JOB_DEPTH),
.EXTERNAL_IO(EXTERNAL_IO),
.MAX_FRAMES(MAX_FRAMES),
.BUFFERS(BUFFERS),
.INPUT_PORTS(INPUT_PORTS),
.OUTPUT_PORTS(OUTPUT_PORTS),
.REUSE_VIEWS(REUSE_VIEWS),
.CAPTURE_CHANNELS(CAPTURE_CHANNELS),
.VIEWS(VIEWS),
.SLOTS(SLOTS),
.CHUNK_FRAMES(CHUNK_FRAMES),
.MEMORY_RESPONSE_CYCLES(MEMORY_RESPONSE_CYCLES),
.BLOCK_CHUNKS(BLOCK_CHUNKS),
.RETIRED_TABLES(RETIRED_TABLES),
.RECLAIM_BLOCKS(RECLAIM_BLOCKS),
.RECLAIM_CYCLES(RECLAIM_CYCLES),
.RECLAIM_GUARD(RECLAIM_GUARD),
.TABLE_PAGES(TABLE_PAGES),
.TABLE_LINES(TABLE_LINES),
.REF_LINES(REF_LINES),
.STACK_LINES(STACK_LINES),
.READ_BUFFERS(READ_BUFFERS),
.WRITE_BUFFERS(WRITE_BUFFERS),
.BURST_MEMORY(BURST_MEMORY),
.AXI_DATA_WIDTH(AXI_DATA_WIDTH),
.PHYSICAL_BASE(PHYSICAL_BASE)
) core_under_test (
.sample_input_read(sample_input_read),
.sample_input_address(sample_input_address),
.sample_input_data(sample_input_data),
.sample_output_write(sample_output_write),
.sample_output_address(sample_output_address),
.sample_output_data(sample_output_data),
.clk(clk),
.reset(reset),
.maintenance_enable(maintenance_enable),
.cut_snapshot_valid(cut_snapshot_valid),
.cut_snapshot_data(cut_snapshot_data),
.reclaim_pending(reclaim_pending),
.reclaim_high_water(reclaim_high_water),
.reclaim_references(reclaim_references),
.reclaim_active(reclaim_active),
.reclaim_released(reclaim_released),
.reclaim_block(reclaim_block),
.reclaim_blocks(reclaim_blocks),
.reclaim_freed(reclaim_freed),
.reclaim_cycles(reclaim_cycles),
.reclaim_quiescent(reclaim_quiescent),
.reclaim_pages_used(reclaim_pages_used),
.reclaim_pages_available(reclaim_pages_available),
.reclaim_pages_freed(reclaim_pages_freed),
.reclaim_capacity_failures(reclaim_capacity_failures),
.run_valid(run_valid),
.run_ready(run_ready),
.run_instruction_count(run_instruction_count),
.run_state_count(run_state_count),
.run_quantum(run_quantum),
.run_chunk_frames(run_chunk_frames),
.run_job_budget(run_job_budget),
.run_buffers(run_buffers),
.run_inputs(run_inputs),
.run_outputs(run_outputs),
.job_enable(job_enable),
.busy(busy),
.faulted(faulted),
.done(done),
.error_code(error_code),
.error_stage(error_stage),
.state_busy(state_busy),
.state_done(state_done),
.jobs_busy(jobs_busy),
.jobs_done(jobs_done),
.buffer_ready(buffer_ready),
.cfg_valid(cfg_valid),
.cfg_ready(cfg_ready),
.cfg_state(cfg_state),
.cfg_write(cfg_write),
.cfg_address(cfg_address),
.cfg_write_data(cfg_write_data),
.cfg_response_valid(cfg_response_valid),
.cfg_response_error(cfg_response_error),
.cfg_read_data(cfg_read_data),
.audio_cfg_valid(audio_cfg_valid),
.audio_cfg_ready(audio_cfg_ready),
.audio_cfg_space(audio_cfg_space),
.audio_cfg_write(audio_cfg_write),
.audio_cfg_address(audio_cfg_address),
.audio_cfg_write_data(audio_cfg_write_data),
.audio_cfg_response_valid(audio_cfg_response_valid),
.audio_cfg_response_error(audio_cfg_response_error),
.audio_cfg_read_data(audio_cfg_read_data),
.monitor_valid(monitor_valid),
.monitor_ready(monitor_ready),
.monitor_data(monitor_data),
.queued_jobs(queued_jobs),
.drain_valid(drain_valid),
.drain_ready(drain_ready),
.command_valid(command_valid),
.command_ready(command_ready),
.command_opcode(command_opcode),
.command_index(command_index),
.command_data(command_data),
.response_valid(response_valid),
.response_ready(response_ready),
.response_error(response_error),
.response_data(response_data),
.transition_busy(transition_busy),
.arena_base(arena_base),
.store_cmd_valid(store_cmd_valid),
.store_cmd_ready(store_cmd_ready),
.store_cmd_opcode(store_cmd_opcode),
.store_cmd_view(store_cmd_view),
.store_cmd_start(store_cmd_start),
.store_cmd_length(store_cmd_length),
.store_cmd_count(store_cmd_count),
.store_busy(store_busy),
.store_done(store_done),
.store_error_code(store_error_code),
.store_result_view(store_result_view),
.store_result_empty(store_result_empty),
.store_write_valid(store_write_valid),
.store_write_ready(store_write_ready),
.store_write_data(store_write_data),
.store_read_valid(store_read_valid),
.store_read_ready(store_read_ready),
.store_read_data(store_read_data),
.store_cfg_valid(store_cfg_valid),
.store_cfg_ready(store_cfg_ready),
.store_cfg_write(store_cfg_write),
.store_cfg_space(store_cfg_space),
.store_cfg_address(store_cfg_address),
.store_cfg_write_data(store_cfg_write_data),
.store_cfg_response_valid(store_cfg_response_valid),
.store_cfg_response_error(store_cfg_response_error),
.store_cfg_read_data(store_cfg_read_data),
.memory_fault(memory_fault),
.m_axi_awid(m_axi_awid),
.m_axi_awaddr(m_axi_awaddr),
.m_axi_awlen(m_axi_awlen),
.m_axi_awsize(m_axi_awsize),
.m_axi_awburst(m_axi_awburst),
.m_axi_awvalid(m_axi_awvalid),
.m_axi_awready(m_axi_awready),
.m_axi_wdata(m_axi_wdata),
.m_axi_wstrb(m_axi_wstrb),
.m_axi_wid(m_axi_wid),
.m_axi_wlast(m_axi_wlast),
.m_axi_wvalid(m_axi_wvalid),
.m_axi_wready(m_axi_wready),
.m_axi_bid(m_axi_bid),
.m_axi_bresp(m_axi_bresp),
.m_axi_bvalid(m_axi_bvalid),
.m_axi_bready(m_axi_bready),
.m_axi_arid(m_axi_arid),
.m_axi_araddr(m_axi_araddr),
.m_axi_arlen(m_axi_arlen),
.m_axi_arsize(m_axi_arsize),
.m_axi_arburst(m_axi_arburst),
.m_axi_arvalid(m_axi_arvalid),
.m_axi_arready(m_axi_arready),
.m_axi_rid(m_axi_rid),
.m_axi_rdata(m_axi_rdata),
.m_axi_rresp(m_axi_rresp),
.m_axi_rlast(m_axi_rlast),
.m_axi_rvalid(m_axi_rvalid),
.m_axi_rready(m_axi_rready)
);
endmodule
