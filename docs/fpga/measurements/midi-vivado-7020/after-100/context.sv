`include "state_processor_defs.svh"
module audio_context #(

    parameter integer EXTERNAL_IO = 0,
    parameter integer LIVE_LOADING = 0,
    parameter integer EXPORT_SYNC_EVENTS = 0,
    parameter integer PROGRAM_REGIONS = LIVE_LOADING != 0 ? 2 : 1,
    parameter integer PROGRAM_DEPTH = 32, parameter integer STATE_DEPTH = 16,
    parameter integer JOB_DEPTH = 32, parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16, parameter integer INPUT_PORTS = 2, parameter integer OUTPUT_PORTS = 2,
    parameter integer REUSE_VIEWS = 0,
    parameter integer MAINTENANCE_AT_RUN = 1,
    parameter integer VIEWS = 32, parameter integer SLOTS = 1024, parameter integer CHUNK_FRAMES = 32,
    parameter integer MEMORY_RESPONSE_CYCLES = 1,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer RETIRED_TABLES = 256,
    parameter integer RECLAIM_BLOCKS = 0, parameter integer RECLAIM_CYCLES = 5000,
    parameter integer RECLAIM_GUARD = 1024,
    parameter integer TABLE_PAGES = (SLOTS/BLOCK_CHUNKS+14)/15 + 8*VIEWS + 16,
    parameter integer TABLE_LINES = 8, parameter integer REF_LINES = 4, parameter integer STACK_LINES = 2,
    parameter integer READ_BUFFERS = 4, parameter integer WRITE_BUFFERS = 4, parameter integer BURST_MEMORY = 1,
    parameter integer AXI_DATA_WIDTH = 32,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000,
    parameter [63:0] PHYSICAL_BYTES_LIMIT = 64'h100000000

) (input wire clk);
reg [7:0] startup = 0;
always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;
reg reset = 1;
always @(posedge clk) reset <= !(&startup);
wire  sample_input_read;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_sample_input_read;
always @(posedge clk) capture_sample_input_read <= sample_input_read;
wire [31:0] sample_input_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_sample_input_address;
always @(posedge clk) capture_sample_input_address <= sample_input_address;
localparam integer PORT_BITS_sample_input_data = (31)+1;
localparam integer SOURCE_BITS_sample_input_data = PORT_BITS_sample_input_data < 32 ? 32 : PORT_BITS_sample_input_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_sample_input_data-1:0] source_sample_input_data = SOURCE_BITS_sample_input_data'(3);
always @(posedge clk) source_sample_input_data <= {source_sample_input_data[SOURCE_BITS_sample_input_data-2:0],source_sample_input_data[SOURCE_BITS_sample_input_data-1]^source_sample_input_data[SOURCE_BITS_sample_input_data-2]};
wire [31:0] sample_input_data = PORT_BITS_sample_input_data'(source_sample_input_data);
wire  sample_output_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_sample_output_write;
always @(posedge clk) capture_sample_output_write <= sample_output_write;
wire [31:0] sample_output_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_sample_output_address;
always @(posedge clk) capture_sample_output_address <= sample_output_address;
wire [31:0] sample_output_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_sample_output_data;
always @(posedge clk) capture_sample_output_data <= sample_output_data;
localparam integer PORT_BITS_maintenance_enable = 1;
localparam integer SOURCE_BITS_maintenance_enable = PORT_BITS_maintenance_enable < 32 ? 32 : PORT_BITS_maintenance_enable;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_maintenance_enable-1:0] source_maintenance_enable = SOURCE_BITS_maintenance_enable'(9);
always @(posedge clk) source_maintenance_enable <= {source_maintenance_enable[SOURCE_BITS_maintenance_enable-2:0],source_maintenance_enable[SOURCE_BITS_maintenance_enable-1]^source_maintenance_enable[SOURCE_BITS_maintenance_enable-2]};
wire  maintenance_enable = PORT_BITS_maintenance_enable'(source_maintenance_enable);
localparam integer PORT_BITS_maintenance_tick = 1;
localparam integer SOURCE_BITS_maintenance_tick = PORT_BITS_maintenance_tick < 32 ? 32 : PORT_BITS_maintenance_tick;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_maintenance_tick-1:0] source_maintenance_tick = SOURCE_BITS_maintenance_tick'(10);
always @(posedge clk) source_maintenance_tick <= {source_maintenance_tick[SOURCE_BITS_maintenance_tick-2:0],source_maintenance_tick[SOURCE_BITS_maintenance_tick-1]^source_maintenance_tick[SOURCE_BITS_maintenance_tick-2]};
wire  maintenance_tick = PORT_BITS_maintenance_tick'(source_maintenance_tick);
wire [31:0] reclaim_pending;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_reclaim_pending;
always @(posedge clk) capture_reclaim_pending <= reclaim_pending;
wire [31:0] reclaim_high_water;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_reclaim_high_water;
always @(posedge clk) capture_reclaim_high_water <= reclaim_high_water;
wire [63:0] reclaim_references;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_reclaim_references;
always @(posedge clk) capture_reclaim_references <= reclaim_references;
wire  reclaim_active;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_reclaim_active;
always @(posedge clk) capture_reclaim_active <= reclaim_active;
wire  reclaim_released;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_reclaim_released;
always @(posedge clk) capture_reclaim_released <= reclaim_released;
wire [31:0] reclaim_block;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_reclaim_block;
always @(posedge clk) capture_reclaim_block <= reclaim_block;
wire [63:0] reclaim_blocks;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_reclaim_blocks;
always @(posedge clk) capture_reclaim_blocks <= reclaim_blocks;
wire [63:0] reclaim_freed;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_reclaim_freed;
always @(posedge clk) capture_reclaim_freed <= reclaim_freed;
wire [63:0] reclaim_cycles;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_reclaim_cycles;
always @(posedge clk) capture_reclaim_cycles <= reclaim_cycles;
wire  reclaim_quiescent;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_reclaim_quiescent;
always @(posedge clk) capture_reclaim_quiescent <= reclaim_quiescent;
wire [31:0] reclaim_pages_used;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_reclaim_pages_used;
always @(posedge clk) capture_reclaim_pages_used <= reclaim_pages_used;
wire [31:0] reclaim_pages_available;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_reclaim_pages_available;
always @(posedge clk) capture_reclaim_pages_available <= reclaim_pages_available;
wire [63:0] reclaim_pages_freed;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_reclaim_pages_freed;
always @(posedge clk) capture_reclaim_pages_freed <= reclaim_pages_freed;
wire [63:0] reclaim_capacity_failures;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_reclaim_capacity_failures;
always @(posedge clk) capture_reclaim_capacity_failures <= reclaim_capacity_failures;
localparam integer PORT_BITS_run_program_region = 1;
localparam integer SOURCE_BITS_run_program_region = PORT_BITS_run_program_region < 32 ? 32 : PORT_BITS_run_program_region;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_program_region-1:0] source_run_program_region = SOURCE_BITS_run_program_region'(25);
always @(posedge clk) source_run_program_region <= {source_run_program_region[SOURCE_BITS_run_program_region-2:0],source_run_program_region[SOURCE_BITS_run_program_region-1]^source_run_program_region[SOURCE_BITS_run_program_region-2]};
wire  run_program_region = PORT_BITS_run_program_region'(source_run_program_region);
localparam integer PORT_BITS_run_sync_count = (31)+1;
localparam integer SOURCE_BITS_run_sync_count = PORT_BITS_run_sync_count < 32 ? 32 : PORT_BITS_run_sync_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_sync_count-1:0] source_run_sync_count = SOURCE_BITS_run_sync_count'(26);
always @(posedge clk) source_run_sync_count <= {source_run_sync_count[SOURCE_BITS_run_sync_count-2:0],source_run_sync_count[SOURCE_BITS_run_sync_count-1]^source_run_sync_count[SOURCE_BITS_run_sync_count-2]};
wire [31:0] run_sync_count = PORT_BITS_run_sync_count'(source_run_sync_count);
localparam integer PORT_BITS_switch_valid = 1;
localparam integer SOURCE_BITS_switch_valid = PORT_BITS_switch_valid < 32 ? 32 : PORT_BITS_switch_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_valid-1:0] source_switch_valid = SOURCE_BITS_switch_valid'(27);
always @(posedge clk) source_switch_valid <= {source_switch_valid[SOURCE_BITS_switch_valid-2:0],source_switch_valid[SOURCE_BITS_switch_valid-1]^source_switch_valid[SOURCE_BITS_switch_valid-2]};
wire  switch_valid = PORT_BITS_switch_valid'(source_switch_valid);
wire  switch_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_switch_ready;
always @(posedge clk) capture_switch_ready <= switch_ready;
localparam integer PORT_BITS_switch_expected = (63)+1;
localparam integer SOURCE_BITS_switch_expected = PORT_BITS_switch_expected < 32 ? 32 : PORT_BITS_switch_expected;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_expected-1:0] source_switch_expected = SOURCE_BITS_switch_expected'(29);
always @(posedge clk) source_switch_expected <= {source_switch_expected[SOURCE_BITS_switch_expected-2:0],source_switch_expected[SOURCE_BITS_switch_expected-1]^source_switch_expected[SOURCE_BITS_switch_expected-2]};
wire [63:0] switch_expected = PORT_BITS_switch_expected'(source_switch_expected);
localparam integer PORT_BITS_switch_revision = (63)+1;
localparam integer SOURCE_BITS_switch_revision = PORT_BITS_switch_revision < 32 ? 32 : PORT_BITS_switch_revision;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_revision-1:0] source_switch_revision = SOURCE_BITS_switch_revision'(30);
always @(posedge clk) source_switch_revision <= {source_switch_revision[SOURCE_BITS_switch_revision-2:0],source_switch_revision[SOURCE_BITS_switch_revision-1]^source_switch_revision[SOURCE_BITS_switch_revision-2]};
wire [63:0] switch_revision = PORT_BITS_switch_revision'(source_switch_revision);
localparam integer PORT_BITS_switch_region = 1;
localparam integer SOURCE_BITS_switch_region = PORT_BITS_switch_region < 32 ? 32 : PORT_BITS_switch_region;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_region-1:0] source_switch_region = SOURCE_BITS_switch_region'(31);
always @(posedge clk) source_switch_region <= {source_switch_region[SOURCE_BITS_switch_region-2:0],source_switch_region[SOURCE_BITS_switch_region-1]^source_switch_region[SOURCE_BITS_switch_region-2]};
wire  switch_region = PORT_BITS_switch_region'(source_switch_region);
localparam integer PORT_BITS_switch_instructions = (15)+1;
localparam integer SOURCE_BITS_switch_instructions = PORT_BITS_switch_instructions < 32 ? 32 : PORT_BITS_switch_instructions;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_instructions-1:0] source_switch_instructions = SOURCE_BITS_switch_instructions'(32);
always @(posedge clk) source_switch_instructions <= {source_switch_instructions[SOURCE_BITS_switch_instructions-2:0],source_switch_instructions[SOURCE_BITS_switch_instructions-1]^source_switch_instructions[SOURCE_BITS_switch_instructions-2]};
wire [15:0] switch_instructions = PORT_BITS_switch_instructions'(source_switch_instructions);
localparam integer PORT_BITS_switch_sync_count = (15)+1;
localparam integer SOURCE_BITS_switch_sync_count = PORT_BITS_switch_sync_count < 32 ? 32 : PORT_BITS_switch_sync_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_sync_count-1:0] source_switch_sync_count = SOURCE_BITS_switch_sync_count'(33);
always @(posedge clk) source_switch_sync_count <= {source_switch_sync_count[SOURCE_BITS_switch_sync_count-2:0],source_switch_sync_count[SOURCE_BITS_switch_sync_count-1]^source_switch_sync_count[SOURCE_BITS_switch_sync_count-2]};
wire [15:0] switch_sync_count = PORT_BITS_switch_sync_count'(source_switch_sync_count);
localparam integer PORT_BITS_switch_states = (15)+1;
localparam integer SOURCE_BITS_switch_states = PORT_BITS_switch_states < 32 ? 32 : PORT_BITS_switch_states;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_states-1:0] source_switch_states = SOURCE_BITS_switch_states'(34);
always @(posedge clk) source_switch_states <= {source_switch_states[SOURCE_BITS_switch_states-2:0],source_switch_states[SOURCE_BITS_switch_states-1]^source_switch_states[SOURCE_BITS_switch_states-2]};
wire [15:0] switch_states = PORT_BITS_switch_states'(source_switch_states);
localparam integer PORT_BITS_switch_jobs = (15)+1;
localparam integer SOURCE_BITS_switch_jobs = PORT_BITS_switch_jobs < 32 ? 32 : PORT_BITS_switch_jobs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_jobs-1:0] source_switch_jobs = SOURCE_BITS_switch_jobs'(35);
always @(posedge clk) source_switch_jobs <= {source_switch_jobs[SOURCE_BITS_switch_jobs-2:0],source_switch_jobs[SOURCE_BITS_switch_jobs-1]^source_switch_jobs[SOURCE_BITS_switch_jobs-2]};
wire [15:0] switch_jobs = PORT_BITS_switch_jobs'(source_switch_jobs);
localparam integer PORT_BITS_switch_buffers = (15)+1;
localparam integer SOURCE_BITS_switch_buffers = PORT_BITS_switch_buffers < 32 ? 32 : PORT_BITS_switch_buffers;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_buffers-1:0] source_switch_buffers = SOURCE_BITS_switch_buffers'(36);
always @(posedge clk) source_switch_buffers <= {source_switch_buffers[SOURCE_BITS_switch_buffers-2:0],source_switch_buffers[SOURCE_BITS_switch_buffers-1]^source_switch_buffers[SOURCE_BITS_switch_buffers-2]};
wire [15:0] switch_buffers = PORT_BITS_switch_buffers'(source_switch_buffers);
wire  switch_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_switch_response_valid;
always @(posedge clk) capture_switch_response_valid <= switch_response_valid;
localparam integer PORT_BITS_switch_response_ready = 1;
localparam integer SOURCE_BITS_switch_response_ready = PORT_BITS_switch_response_ready < 32 ? 32 : PORT_BITS_switch_response_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_switch_response_ready-1:0] source_switch_response_ready = SOURCE_BITS_switch_response_ready'(38);
always @(posedge clk) source_switch_response_ready <= {source_switch_response_ready[SOURCE_BITS_switch_response_ready-2:0],source_switch_response_ready[SOURCE_BITS_switch_response_ready-1]^source_switch_response_ready[SOURCE_BITS_switch_response_ready-2]};
wire  switch_response_ready = PORT_BITS_switch_response_ready'(source_switch_response_ready);
wire  switch_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_switch_response_error;
always @(posedge clk) capture_switch_response_error <= switch_response_error;
wire [63:0] switch_first_frame;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_switch_first_frame;
always @(posedge clk) capture_switch_first_frame <= switch_first_frame;
wire [63:0] active_revision;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_active_revision;
always @(posedge clk) capture_active_revision <= active_revision;
wire [63:0] sync_frame;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_sync_frame;
always @(posedge clk) capture_sync_frame <= sync_frame;
wire  sync_event_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_sync_event_valid;
always @(posedge clk) capture_sync_event_valid <= sync_event_valid;
localparam integer PORT_BITS_sync_event_ready = 1;
localparam integer SOURCE_BITS_sync_event_ready = PORT_BITS_sync_event_ready < 32 ? 32 : PORT_BITS_sync_event_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_sync_event_ready-1:0] source_sync_event_ready = SOURCE_BITS_sync_event_ready'(44);
always @(posedge clk) source_sync_event_ready <= {source_sync_event_ready[SOURCE_BITS_sync_event_ready-2:0],source_sync_event_ready[SOURCE_BITS_sync_event_ready-1]^source_sync_event_ready[SOURCE_BITS_sync_event_ready-2]};
wire  sync_event_ready = PORT_BITS_sync_event_ready'(source_sync_event_ready);
wire [255:0] sync_event_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [255:0] capture_sync_event_data;
always @(posedge clk) capture_sync_event_data <= sync_event_data;
localparam integer PORT_BITS_run_valid = 1;
localparam integer SOURCE_BITS_run_valid = PORT_BITS_run_valid < 32 ? 32 : PORT_BITS_run_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_valid-1:0] source_run_valid = SOURCE_BITS_run_valid'(46);
always @(posedge clk) source_run_valid <= {source_run_valid[SOURCE_BITS_run_valid-2:0],source_run_valid[SOURCE_BITS_run_valid-1]^source_run_valid[SOURCE_BITS_run_valid-2]};
wire  run_valid = PORT_BITS_run_valid'(source_run_valid);
wire  run_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_run_ready;
always @(posedge clk) capture_run_ready <= run_ready;
localparam integer PORT_BITS_run_instruction_count = (31)+1;
localparam integer SOURCE_BITS_run_instruction_count = PORT_BITS_run_instruction_count < 32 ? 32 : PORT_BITS_run_instruction_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_instruction_count-1:0] source_run_instruction_count = SOURCE_BITS_run_instruction_count'(48);
always @(posedge clk) source_run_instruction_count <= {source_run_instruction_count[SOURCE_BITS_run_instruction_count-2:0],source_run_instruction_count[SOURCE_BITS_run_instruction_count-1]^source_run_instruction_count[SOURCE_BITS_run_instruction_count-2]};
wire [31:0] run_instruction_count = PORT_BITS_run_instruction_count'(source_run_instruction_count);
localparam integer PORT_BITS_run_state_count = (31)+1;
localparam integer SOURCE_BITS_run_state_count = PORT_BITS_run_state_count < 32 ? 32 : PORT_BITS_run_state_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_state_count-1:0] source_run_state_count = SOURCE_BITS_run_state_count'(49);
always @(posedge clk) source_run_state_count <= {source_run_state_count[SOURCE_BITS_run_state_count-2:0],source_run_state_count[SOURCE_BITS_run_state_count-1]^source_run_state_count[SOURCE_BITS_run_state_count-2]};
wire [31:0] run_state_count = PORT_BITS_run_state_count'(source_run_state_count);
wire [31:0] run_quantum = 32'd32;
wire [31:0] run_chunk_frames = 32'd32;
localparam integer PORT_BITS_run_job_budget = (31)+1;
localparam integer SOURCE_BITS_run_job_budget = PORT_BITS_run_job_budget < 32 ? 32 : PORT_BITS_run_job_budget;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_job_budget-1:0] source_run_job_budget = SOURCE_BITS_run_job_budget'(52);
always @(posedge clk) source_run_job_budget <= {source_run_job_budget[SOURCE_BITS_run_job_budget-2:0],source_run_job_budget[SOURCE_BITS_run_job_budget-1]^source_run_job_budget[SOURCE_BITS_run_job_budget-2]};
wire [31:0] run_job_budget = PORT_BITS_run_job_budget'(source_run_job_budget);
localparam integer PORT_BITS_run_buffers = (31)+1;
localparam integer SOURCE_BITS_run_buffers = PORT_BITS_run_buffers < 32 ? 32 : PORT_BITS_run_buffers;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_buffers-1:0] source_run_buffers = SOURCE_BITS_run_buffers'(53);
always @(posedge clk) source_run_buffers <= {source_run_buffers[SOURCE_BITS_run_buffers-2:0],source_run_buffers[SOURCE_BITS_run_buffers-1]^source_run_buffers[SOURCE_BITS_run_buffers-2]};
wire [31:0] run_buffers = PORT_BITS_run_buffers'(source_run_buffers);
localparam integer PORT_BITS_run_inputs = (31)+1;
localparam integer SOURCE_BITS_run_inputs = PORT_BITS_run_inputs < 32 ? 32 : PORT_BITS_run_inputs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_inputs-1:0] source_run_inputs = SOURCE_BITS_run_inputs'(54);
always @(posedge clk) source_run_inputs <= {source_run_inputs[SOURCE_BITS_run_inputs-2:0],source_run_inputs[SOURCE_BITS_run_inputs-1]^source_run_inputs[SOURCE_BITS_run_inputs-2]};
wire [31:0] run_inputs = PORT_BITS_run_inputs'(source_run_inputs);
localparam integer PORT_BITS_run_outputs = (31)+1;
localparam integer SOURCE_BITS_run_outputs = PORT_BITS_run_outputs < 32 ? 32 : PORT_BITS_run_outputs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_run_outputs-1:0] source_run_outputs = SOURCE_BITS_run_outputs'(55);
always @(posedge clk) source_run_outputs <= {source_run_outputs[SOURCE_BITS_run_outputs-2:0],source_run_outputs[SOURCE_BITS_run_outputs-1]^source_run_outputs[SOURCE_BITS_run_outputs-2]};
wire [31:0] run_outputs = PORT_BITS_run_outputs'(source_run_outputs);
localparam integer PORT_BITS_job_enable = 1;
localparam integer SOURCE_BITS_job_enable = PORT_BITS_job_enable < 32 ? 32 : PORT_BITS_job_enable;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_job_enable-1:0] source_job_enable = SOURCE_BITS_job_enable'(56);
always @(posedge clk) source_job_enable <= {source_job_enable[SOURCE_BITS_job_enable-2:0],source_job_enable[SOURCE_BITS_job_enable-1]^source_job_enable[SOURCE_BITS_job_enable-2]};
wire  job_enable = PORT_BITS_job_enable'(source_job_enable);
wire  busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_busy;
always @(posedge clk) capture_busy <= busy;
wire  done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_done;
always @(posedge clk) capture_done <= done;
wire [7:0] error_code;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [7:0] capture_error_code;
always @(posedge clk) capture_error_code <= error_code;
wire [1:0] error_stage;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] capture_error_stage;
always @(posedge clk) capture_error_stage <= error_stage;
wire  state_busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_state_busy;
always @(posedge clk) capture_state_busy <= state_busy;
wire  state_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_state_done;
always @(posedge clk) capture_state_done <= state_done;
wire  jobs_busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_jobs_busy;
always @(posedge clk) capture_jobs_busy <= jobs_busy;
wire  jobs_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_jobs_done;
always @(posedge clk) capture_jobs_done <= jobs_done;
wire [BUFFERS-1:0] buffer_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [BUFFERS-1:0] capture_buffer_ready;
always @(posedge clk) capture_buffer_ready <= buffer_ready;
localparam integer PORT_BITS_cfg_valid = 1;
localparam integer SOURCE_BITS_cfg_valid = PORT_BITS_cfg_valid < 32 ? 32 : PORT_BITS_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_valid-1:0] source_cfg_valid = SOURCE_BITS_cfg_valid'(66);
always @(posedge clk) source_cfg_valid <= {source_cfg_valid[SOURCE_BITS_cfg_valid-2:0],source_cfg_valid[SOURCE_BITS_cfg_valid-1]^source_cfg_valid[SOURCE_BITS_cfg_valid-2]};
wire  cfg_valid = PORT_BITS_cfg_valid'(source_cfg_valid);
wire  cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_cfg_ready;
always @(posedge clk) capture_cfg_ready <= cfg_ready;
localparam integer PORT_BITS_cfg_state = 1;
localparam integer SOURCE_BITS_cfg_state = PORT_BITS_cfg_state < 32 ? 32 : PORT_BITS_cfg_state;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_state-1:0] source_cfg_state = SOURCE_BITS_cfg_state'(68);
always @(posedge clk) source_cfg_state <= {source_cfg_state[SOURCE_BITS_cfg_state-2:0],source_cfg_state[SOURCE_BITS_cfg_state-1]^source_cfg_state[SOURCE_BITS_cfg_state-2]};
wire  cfg_state = PORT_BITS_cfg_state'(source_cfg_state);
localparam integer PORT_BITS_cfg_loop = 1;
localparam integer SOURCE_BITS_cfg_loop = PORT_BITS_cfg_loop < 32 ? 32 : PORT_BITS_cfg_loop;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_loop-1:0] source_cfg_loop = SOURCE_BITS_cfg_loop'(69);
always @(posedge clk) source_cfg_loop <= {source_cfg_loop[SOURCE_BITS_cfg_loop-2:0],source_cfg_loop[SOURCE_BITS_cfg_loop-1]^source_cfg_loop[SOURCE_BITS_cfg_loop-2]};
wire  cfg_loop = PORT_BITS_cfg_loop'(source_cfg_loop);
localparam integer PORT_BITS_cfg_write = 1;
localparam integer SOURCE_BITS_cfg_write = PORT_BITS_cfg_write < 32 ? 32 : PORT_BITS_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write-1:0] source_cfg_write = SOURCE_BITS_cfg_write'(70);
always @(posedge clk) source_cfg_write <= {source_cfg_write[SOURCE_BITS_cfg_write-2:0],source_cfg_write[SOURCE_BITS_cfg_write-1]^source_cfg_write[SOURCE_BITS_cfg_write-2]};
wire  cfg_write = PORT_BITS_cfg_write'(source_cfg_write);
localparam integer PORT_BITS_cfg_address = (15)+1;
localparam integer SOURCE_BITS_cfg_address = PORT_BITS_cfg_address < 32 ? 32 : PORT_BITS_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_address-1:0] source_cfg_address = SOURCE_BITS_cfg_address'(71);
always @(posedge clk) source_cfg_address <= {source_cfg_address[SOURCE_BITS_cfg_address-2:0],source_cfg_address[SOURCE_BITS_cfg_address-1]^source_cfg_address[SOURCE_BITS_cfg_address-2]};
wire [15:0] cfg_address = PORT_BITS_cfg_address'(source_cfg_address);
localparam integer PORT_BITS_cfg_write_data = (`SHOOP_SP_STATE_BITS-1)+1;
localparam integer SOURCE_BITS_cfg_write_data = PORT_BITS_cfg_write_data < 32 ? 32 : PORT_BITS_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write_data-1:0] source_cfg_write_data = SOURCE_BITS_cfg_write_data'(72);
always @(posedge clk) source_cfg_write_data <= {source_cfg_write_data[SOURCE_BITS_cfg_write_data-2:0],source_cfg_write_data[SOURCE_BITS_cfg_write_data-1]^source_cfg_write_data[SOURCE_BITS_cfg_write_data-2]};
wire [`SHOOP_SP_STATE_BITS-1:0] cfg_write_data = PORT_BITS_cfg_write_data'(source_cfg_write_data);
wire  cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_cfg_response_valid;
always @(posedge clk) capture_cfg_response_valid <= cfg_response_valid;
wire  cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_cfg_response_error;
always @(posedge clk) capture_cfg_response_error <= cfg_response_error;
wire [`SHOOP_SP_STATE_BITS-1:0] cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [`SHOOP_SP_STATE_BITS-1:0] capture_cfg_read_data;
always @(posedge clk) capture_cfg_read_data <= cfg_read_data;
localparam integer PORT_BITS_audio_cfg_valid = 1;
localparam integer SOURCE_BITS_audio_cfg_valid = PORT_BITS_audio_cfg_valid < 32 ? 32 : PORT_BITS_audio_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_valid-1:0] source_audio_cfg_valid = SOURCE_BITS_audio_cfg_valid'(76);
always @(posedge clk) source_audio_cfg_valid <= {source_audio_cfg_valid[SOURCE_BITS_audio_cfg_valid-2:0],source_audio_cfg_valid[SOURCE_BITS_audio_cfg_valid-1]^source_audio_cfg_valid[SOURCE_BITS_audio_cfg_valid-2]};
wire  audio_cfg_valid = PORT_BITS_audio_cfg_valid'(source_audio_cfg_valid);
wire  audio_cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_audio_cfg_ready;
always @(posedge clk) capture_audio_cfg_ready <= audio_cfg_ready;
localparam integer PORT_BITS_audio_cfg_space = (1)+1;
localparam integer SOURCE_BITS_audio_cfg_space = PORT_BITS_audio_cfg_space < 32 ? 32 : PORT_BITS_audio_cfg_space;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_space-1:0] source_audio_cfg_space = SOURCE_BITS_audio_cfg_space'(78);
always @(posedge clk) source_audio_cfg_space <= {source_audio_cfg_space[SOURCE_BITS_audio_cfg_space-2:0],source_audio_cfg_space[SOURCE_BITS_audio_cfg_space-1]^source_audio_cfg_space[SOURCE_BITS_audio_cfg_space-2]};
wire [1:0] audio_cfg_space = PORT_BITS_audio_cfg_space'(source_audio_cfg_space);
localparam integer PORT_BITS_audio_cfg_write = 1;
localparam integer SOURCE_BITS_audio_cfg_write = PORT_BITS_audio_cfg_write < 32 ? 32 : PORT_BITS_audio_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_write-1:0] source_audio_cfg_write = SOURCE_BITS_audio_cfg_write'(79);
always @(posedge clk) source_audio_cfg_write <= {source_audio_cfg_write[SOURCE_BITS_audio_cfg_write-2:0],source_audio_cfg_write[SOURCE_BITS_audio_cfg_write-1]^source_audio_cfg_write[SOURCE_BITS_audio_cfg_write-2]};
wire  audio_cfg_write = PORT_BITS_audio_cfg_write'(source_audio_cfg_write);
localparam integer PORT_BITS_audio_cfg_address = (31)+1;
localparam integer SOURCE_BITS_audio_cfg_address = PORT_BITS_audio_cfg_address < 32 ? 32 : PORT_BITS_audio_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_address-1:0] source_audio_cfg_address = SOURCE_BITS_audio_cfg_address'(80);
always @(posedge clk) source_audio_cfg_address <= {source_audio_cfg_address[SOURCE_BITS_audio_cfg_address-2:0],source_audio_cfg_address[SOURCE_BITS_audio_cfg_address-1]^source_audio_cfg_address[SOURCE_BITS_audio_cfg_address-2]};
wire [31:0] audio_cfg_address = PORT_BITS_audio_cfg_address'(source_audio_cfg_address);
localparam integer PORT_BITS_audio_cfg_write_data = (31)+1;
localparam integer SOURCE_BITS_audio_cfg_write_data = PORT_BITS_audio_cfg_write_data < 32 ? 32 : PORT_BITS_audio_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_audio_cfg_write_data-1:0] source_audio_cfg_write_data = SOURCE_BITS_audio_cfg_write_data'(81);
always @(posedge clk) source_audio_cfg_write_data <= {source_audio_cfg_write_data[SOURCE_BITS_audio_cfg_write_data-2:0],source_audio_cfg_write_data[SOURCE_BITS_audio_cfg_write_data-1]^source_audio_cfg_write_data[SOURCE_BITS_audio_cfg_write_data-2]};
wire [31:0] audio_cfg_write_data = PORT_BITS_audio_cfg_write_data'(source_audio_cfg_write_data);
wire  audio_cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_audio_cfg_response_valid;
always @(posedge clk) capture_audio_cfg_response_valid <= audio_cfg_response_valid;
wire  audio_cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_audio_cfg_response_error;
always @(posedge clk) capture_audio_cfg_response_error <= audio_cfg_response_error;
wire [31:0] audio_cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_audio_cfg_read_data;
always @(posedge clk) capture_audio_cfg_read_data <= audio_cfg_read_data;
wire  monitor_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_monitor_valid;
always @(posedge clk) capture_monitor_valid <= monitor_valid;
wire  monitor_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_monitor_ready;
always @(posedge clk) capture_monitor_ready <= monitor_ready;
wire [`SHOOP_SP_JOB_BITS-1:0] monitor_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [`SHOOP_SP_JOB_BITS-1:0] capture_monitor_data;
always @(posedge clk) capture_monitor_data <= monitor_data;
wire [31:0] queued_jobs;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_queued_jobs;
always @(posedge clk) capture_queued_jobs <= queued_jobs;
wire  drain_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_drain_valid;
always @(posedge clk) capture_drain_valid <= drain_valid;
localparam integer PORT_BITS_drain_ready = 1;
localparam integer SOURCE_BITS_drain_ready = PORT_BITS_drain_ready < 32 ? 32 : PORT_BITS_drain_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_drain_ready-1:0] source_drain_ready = SOURCE_BITS_drain_ready'(90);
always @(posedge clk) source_drain_ready <= {source_drain_ready[SOURCE_BITS_drain_ready-2:0],source_drain_ready[SOURCE_BITS_drain_ready-1]^source_drain_ready[SOURCE_BITS_drain_ready-2]};
wire  drain_ready = PORT_BITS_drain_ready'(source_drain_ready);
wire  loop_read_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_loop_read_valid;
always @(posedge clk) capture_loop_read_valid <= loop_read_valid;
localparam integer PORT_BITS_loop_read_ready = 1;
localparam integer SOURCE_BITS_loop_read_ready = PORT_BITS_loop_read_ready < 32 ? 32 : PORT_BITS_loop_read_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_read_ready-1:0] source_loop_read_ready = SOURCE_BITS_loop_read_ready'(92);
always @(posedge clk) source_loop_read_ready <= {source_loop_read_ready[SOURCE_BITS_loop_read_ready-2:0],source_loop_read_ready[SOURCE_BITS_loop_read_ready-1]^source_loop_read_ready[SOURCE_BITS_loop_read_ready-2]};
wire  loop_read_ready = PORT_BITS_loop_read_ready'(source_loop_read_ready);
wire [31:0] loop_read_slot;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_loop_read_slot;
always @(posedge clk) capture_loop_read_slot <= loop_read_slot;
localparam integer PORT_BITS_loop_response_valid = 1;
localparam integer SOURCE_BITS_loop_response_valid = PORT_BITS_loop_response_valid < 32 ? 32 : PORT_BITS_loop_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_response_valid-1:0] source_loop_response_valid = SOURCE_BITS_loop_response_valid'(94);
always @(posedge clk) source_loop_response_valid <= {source_loop_response_valid[SOURCE_BITS_loop_response_valid-2:0],source_loop_response_valid[SOURCE_BITS_loop_response_valid-1]^source_loop_response_valid[SOURCE_BITS_loop_response_valid-2]};
wire  loop_response_valid = PORT_BITS_loop_response_valid'(source_loop_response_valid);
localparam integer PORT_BITS_loop_response_error = 1;
localparam integer SOURCE_BITS_loop_response_error = PORT_BITS_loop_response_error < 32 ? 32 : PORT_BITS_loop_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_response_error-1:0] source_loop_response_error = SOURCE_BITS_loop_response_error'(95);
always @(posedge clk) source_loop_response_error <= {source_loop_response_error[SOURCE_BITS_loop_response_error-2:0],source_loop_response_error[SOURCE_BITS_loop_response_error-1]^source_loop_response_error[SOURCE_BITS_loop_response_error-2]};
wire  loop_response_error = PORT_BITS_loop_response_error'(source_loop_response_error);
localparam integer PORT_BITS_loop_response_data = (163)+1;
localparam integer SOURCE_BITS_loop_response_data = PORT_BITS_loop_response_data < 32 ? 32 : PORT_BITS_loop_response_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_response_data-1:0] source_loop_response_data = SOURCE_BITS_loop_response_data'(96);
always @(posedge clk) source_loop_response_data <= {source_loop_response_data[SOURCE_BITS_loop_response_data-2:0],source_loop_response_data[SOURCE_BITS_loop_response_data-1]^source_loop_response_data[SOURCE_BITS_loop_response_data-2]};
wire [163:0] loop_response_data = PORT_BITS_loop_response_data'(source_loop_response_data);
wire [63:0] arena_base = 64'd0;
localparam integer PORT_BITS_store_cmd_valid = 1;
localparam integer SOURCE_BITS_store_cmd_valid = PORT_BITS_store_cmd_valid < 32 ? 32 : PORT_BITS_store_cmd_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_valid-1:0] source_store_cmd_valid = SOURCE_BITS_store_cmd_valid'(98);
always @(posedge clk) source_store_cmd_valid <= {source_store_cmd_valid[SOURCE_BITS_store_cmd_valid-2:0],source_store_cmd_valid[SOURCE_BITS_store_cmd_valid-1]^source_store_cmd_valid[SOURCE_BITS_store_cmd_valid-2]};
wire  store_cmd_valid = PORT_BITS_store_cmd_valid'(source_store_cmd_valid);
wire  store_cmd_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_cmd_ready;
always @(posedge clk) capture_store_cmd_ready <= store_cmd_ready;
localparam integer PORT_BITS_store_cmd_opcode = (3)+1;
localparam integer SOURCE_BITS_store_cmd_opcode = PORT_BITS_store_cmd_opcode < 32 ? 32 : PORT_BITS_store_cmd_opcode;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_opcode-1:0] source_store_cmd_opcode = SOURCE_BITS_store_cmd_opcode'(100);
always @(posedge clk) source_store_cmd_opcode <= {source_store_cmd_opcode[SOURCE_BITS_store_cmd_opcode-2:0],source_store_cmd_opcode[SOURCE_BITS_store_cmd_opcode-1]^source_store_cmd_opcode[SOURCE_BITS_store_cmd_opcode-2]};
wire [3:0] store_cmd_opcode = PORT_BITS_store_cmd_opcode'(source_store_cmd_opcode);
localparam integer PORT_BITS_store_cmd_view = (63)+1;
localparam integer SOURCE_BITS_store_cmd_view = PORT_BITS_store_cmd_view < 32 ? 32 : PORT_BITS_store_cmd_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_view-1:0] source_store_cmd_view = SOURCE_BITS_store_cmd_view'(101);
always @(posedge clk) source_store_cmd_view <= {source_store_cmd_view[SOURCE_BITS_store_cmd_view-2:0],source_store_cmd_view[SOURCE_BITS_store_cmd_view-1]^source_store_cmd_view[SOURCE_BITS_store_cmd_view-2]};
wire [63:0] store_cmd_view = PORT_BITS_store_cmd_view'(source_store_cmd_view);
localparam integer PORT_BITS_store_cmd_start = (63)+1;
localparam integer SOURCE_BITS_store_cmd_start = PORT_BITS_store_cmd_start < 32 ? 32 : PORT_BITS_store_cmd_start;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_start-1:0] source_store_cmd_start = SOURCE_BITS_store_cmd_start'(102);
always @(posedge clk) source_store_cmd_start <= {source_store_cmd_start[SOURCE_BITS_store_cmd_start-2:0],source_store_cmd_start[SOURCE_BITS_store_cmd_start-1]^source_store_cmd_start[SOURCE_BITS_store_cmd_start-2]};
wire [63:0] store_cmd_start = PORT_BITS_store_cmd_start'(source_store_cmd_start);
localparam integer PORT_BITS_store_cmd_length = (63)+1;
localparam integer SOURCE_BITS_store_cmd_length = PORT_BITS_store_cmd_length < 32 ? 32 : PORT_BITS_store_cmd_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_length-1:0] source_store_cmd_length = SOURCE_BITS_store_cmd_length'(103);
always @(posedge clk) source_store_cmd_length <= {source_store_cmd_length[SOURCE_BITS_store_cmd_length-2:0],source_store_cmd_length[SOURCE_BITS_store_cmd_length-1]^source_store_cmd_length[SOURCE_BITS_store_cmd_length-2]};
wire [63:0] store_cmd_length = PORT_BITS_store_cmd_length'(source_store_cmd_length);
localparam integer PORT_BITS_store_cmd_count = (31)+1;
localparam integer SOURCE_BITS_store_cmd_count = PORT_BITS_store_cmd_count < 32 ? 32 : PORT_BITS_store_cmd_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cmd_count-1:0] source_store_cmd_count = SOURCE_BITS_store_cmd_count'(104);
always @(posedge clk) source_store_cmd_count <= {source_store_cmd_count[SOURCE_BITS_store_cmd_count-2:0],source_store_cmd_count[SOURCE_BITS_store_cmd_count-1]^source_store_cmd_count[SOURCE_BITS_store_cmd_count-2]};
wire [31:0] store_cmd_count = PORT_BITS_store_cmd_count'(source_store_cmd_count);
wire  store_busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_busy;
always @(posedge clk) capture_store_busy <= store_busy;
wire  store_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_done;
always @(posedge clk) capture_store_done <= store_done;
wire [7:0] store_error_code;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [7:0] capture_store_error_code;
always @(posedge clk) capture_store_error_code <= store_error_code;
wire [31:0] store_result_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_store_result_view;
always @(posedge clk) capture_store_result_view <= store_result_view;
wire  store_result_empty;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_result_empty;
always @(posedge clk) capture_store_result_empty <= store_result_empty;
localparam integer PORT_BITS_store_write_valid = 1;
localparam integer SOURCE_BITS_store_write_valid = PORT_BITS_store_write_valid < 32 ? 32 : PORT_BITS_store_write_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_write_valid-1:0] source_store_write_valid = SOURCE_BITS_store_write_valid'(110);
always @(posedge clk) source_store_write_valid <= {source_store_write_valid[SOURCE_BITS_store_write_valid-2:0],source_store_write_valid[SOURCE_BITS_store_write_valid-1]^source_store_write_valid[SOURCE_BITS_store_write_valid-2]};
wire  store_write_valid = PORT_BITS_store_write_valid'(source_store_write_valid);
wire  store_write_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_write_ready;
always @(posedge clk) capture_store_write_ready <= store_write_ready;
localparam integer PORT_BITS_store_write_data = (31)+1;
localparam integer SOURCE_BITS_store_write_data = PORT_BITS_store_write_data < 32 ? 32 : PORT_BITS_store_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_write_data-1:0] source_store_write_data = SOURCE_BITS_store_write_data'(112);
always @(posedge clk) source_store_write_data <= {source_store_write_data[SOURCE_BITS_store_write_data-2:0],source_store_write_data[SOURCE_BITS_store_write_data-1]^source_store_write_data[SOURCE_BITS_store_write_data-2]};
wire [31:0] store_write_data = PORT_BITS_store_write_data'(source_store_write_data);
wire  store_read_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_read_valid;
always @(posedge clk) capture_store_read_valid <= store_read_valid;
localparam integer PORT_BITS_store_read_ready = 1;
localparam integer SOURCE_BITS_store_read_ready = PORT_BITS_store_read_ready < 32 ? 32 : PORT_BITS_store_read_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_read_ready-1:0] source_store_read_ready = SOURCE_BITS_store_read_ready'(114);
always @(posedge clk) source_store_read_ready <= {source_store_read_ready[SOURCE_BITS_store_read_ready-2:0],source_store_read_ready[SOURCE_BITS_store_read_ready-1]^source_store_read_ready[SOURCE_BITS_store_read_ready-2]};
wire  store_read_ready = PORT_BITS_store_read_ready'(source_store_read_ready);
wire [31:0] store_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_store_read_data;
always @(posedge clk) capture_store_read_data <= store_read_data;
localparam integer PORT_BITS_store_cfg_valid = 1;
localparam integer SOURCE_BITS_store_cfg_valid = PORT_BITS_store_cfg_valid < 32 ? 32 : PORT_BITS_store_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_valid-1:0] source_store_cfg_valid = SOURCE_BITS_store_cfg_valid'(116);
always @(posedge clk) source_store_cfg_valid <= {source_store_cfg_valid[SOURCE_BITS_store_cfg_valid-2:0],source_store_cfg_valid[SOURCE_BITS_store_cfg_valid-1]^source_store_cfg_valid[SOURCE_BITS_store_cfg_valid-2]};
wire  store_cfg_valid = PORT_BITS_store_cfg_valid'(source_store_cfg_valid);
wire  store_cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_cfg_ready;
always @(posedge clk) capture_store_cfg_ready <= store_cfg_ready;
localparam integer PORT_BITS_store_cfg_write = 1;
localparam integer SOURCE_BITS_store_cfg_write = PORT_BITS_store_cfg_write < 32 ? 32 : PORT_BITS_store_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_write-1:0] source_store_cfg_write = SOURCE_BITS_store_cfg_write'(118);
always @(posedge clk) source_store_cfg_write <= {source_store_cfg_write[SOURCE_BITS_store_cfg_write-2:0],source_store_cfg_write[SOURCE_BITS_store_cfg_write-1]^source_store_cfg_write[SOURCE_BITS_store_cfg_write-2]};
wire  store_cfg_write = PORT_BITS_store_cfg_write'(source_store_cfg_write);
localparam integer PORT_BITS_store_cfg_space = (2)+1;
localparam integer SOURCE_BITS_store_cfg_space = PORT_BITS_store_cfg_space < 32 ? 32 : PORT_BITS_store_cfg_space;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_space-1:0] source_store_cfg_space = SOURCE_BITS_store_cfg_space'(119);
always @(posedge clk) source_store_cfg_space <= {source_store_cfg_space[SOURCE_BITS_store_cfg_space-2:0],source_store_cfg_space[SOURCE_BITS_store_cfg_space-1]^source_store_cfg_space[SOURCE_BITS_store_cfg_space-2]};
wire [2:0] store_cfg_space = PORT_BITS_store_cfg_space'(source_store_cfg_space);
localparam integer PORT_BITS_store_cfg_address = (31)+1;
localparam integer SOURCE_BITS_store_cfg_address = PORT_BITS_store_cfg_address < 32 ? 32 : PORT_BITS_store_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_address-1:0] source_store_cfg_address = SOURCE_BITS_store_cfg_address'(120);
always @(posedge clk) source_store_cfg_address <= {source_store_cfg_address[SOURCE_BITS_store_cfg_address-2:0],source_store_cfg_address[SOURCE_BITS_store_cfg_address-1]^source_store_cfg_address[SOURCE_BITS_store_cfg_address-2]};
wire [31:0] store_cfg_address = PORT_BITS_store_cfg_address'(source_store_cfg_address);
localparam integer PORT_BITS_store_cfg_write_data = (159)+1;
localparam integer SOURCE_BITS_store_cfg_write_data = PORT_BITS_store_cfg_write_data < 32 ? 32 : PORT_BITS_store_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_store_cfg_write_data-1:0] source_store_cfg_write_data = SOURCE_BITS_store_cfg_write_data'(121);
always @(posedge clk) source_store_cfg_write_data <= {source_store_cfg_write_data[SOURCE_BITS_store_cfg_write_data-2:0],source_store_cfg_write_data[SOURCE_BITS_store_cfg_write_data-1]^source_store_cfg_write_data[SOURCE_BITS_store_cfg_write_data-2]};
wire [159:0] store_cfg_write_data = PORT_BITS_store_cfg_write_data'(source_store_cfg_write_data);
wire  store_cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_cfg_response_valid;
always @(posedge clk) capture_store_cfg_response_valid <= store_cfg_response_valid;
wire  store_cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_store_cfg_response_error;
always @(posedge clk) capture_store_cfg_response_error <= store_cfg_response_error;
wire [159:0] store_cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [159:0] capture_store_cfg_read_data;
always @(posedge clk) capture_store_cfg_read_data <= store_cfg_read_data;
wire  memory_fault;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_memory_fault;
always @(posedge clk) capture_memory_fault <= memory_fault;
wire  m_axi_awid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_awid;
always @(posedge clk) capture_m_axi_awid <= m_axi_awid;
wire [31:0] m_axi_awaddr;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_m_axi_awaddr;
always @(posedge clk) capture_m_axi_awaddr <= m_axi_awaddr;
wire [3:0] m_axi_awlen;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] capture_m_axi_awlen;
always @(posedge clk) capture_m_axi_awlen <= m_axi_awlen;
wire [2:0] m_axi_awsize;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [2:0] capture_m_axi_awsize;
always @(posedge clk) capture_m_axi_awsize <= m_axi_awsize;
wire [1:0] m_axi_awburst;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] capture_m_axi_awburst;
always @(posedge clk) capture_m_axi_awburst <= m_axi_awburst;
wire  m_axi_awvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_awvalid;
always @(posedge clk) capture_m_axi_awvalid <= m_axi_awvalid;
localparam integer PORT_BITS_m_axi_awready = 1;
localparam integer SOURCE_BITS_m_axi_awready = PORT_BITS_m_axi_awready < 32 ? 32 : PORT_BITS_m_axi_awready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_awready-1:0] source_m_axi_awready = SOURCE_BITS_m_axi_awready'(132);
always @(posedge clk) source_m_axi_awready <= {source_m_axi_awready[SOURCE_BITS_m_axi_awready-2:0],source_m_axi_awready[SOURCE_BITS_m_axi_awready-1]^source_m_axi_awready[SOURCE_BITS_m_axi_awready-2]};
wire  m_axi_awready = PORT_BITS_m_axi_awready'(source_m_axi_awready);
wire [AXI_DATA_WIDTH-1:0] m_axi_wdata;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [AXI_DATA_WIDTH-1:0] capture_m_axi_wdata;
always @(posedge clk) capture_m_axi_wdata <= m_axi_wdata;
wire [AXI_DATA_WIDTH/8-1:0] m_axi_wstrb;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [AXI_DATA_WIDTH/8-1:0] capture_m_axi_wstrb;
always @(posedge clk) capture_m_axi_wstrb <= m_axi_wstrb;
wire  m_axi_wid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_wid;
always @(posedge clk) capture_m_axi_wid <= m_axi_wid;
wire  m_axi_wlast;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_wlast;
always @(posedge clk) capture_m_axi_wlast <= m_axi_wlast;
wire  m_axi_wvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_wvalid;
always @(posedge clk) capture_m_axi_wvalid <= m_axi_wvalid;
localparam integer PORT_BITS_m_axi_wready = 1;
localparam integer SOURCE_BITS_m_axi_wready = PORT_BITS_m_axi_wready < 32 ? 32 : PORT_BITS_m_axi_wready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_wready-1:0] source_m_axi_wready = SOURCE_BITS_m_axi_wready'(138);
always @(posedge clk) source_m_axi_wready <= {source_m_axi_wready[SOURCE_BITS_m_axi_wready-2:0],source_m_axi_wready[SOURCE_BITS_m_axi_wready-1]^source_m_axi_wready[SOURCE_BITS_m_axi_wready-2]};
wire  m_axi_wready = PORT_BITS_m_axi_wready'(source_m_axi_wready);
localparam integer PORT_BITS_m_axi_bid = 1;
localparam integer SOURCE_BITS_m_axi_bid = PORT_BITS_m_axi_bid < 32 ? 32 : PORT_BITS_m_axi_bid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bid-1:0] source_m_axi_bid = SOURCE_BITS_m_axi_bid'(139);
always @(posedge clk) source_m_axi_bid <= {source_m_axi_bid[SOURCE_BITS_m_axi_bid-2:0],source_m_axi_bid[SOURCE_BITS_m_axi_bid-1]^source_m_axi_bid[SOURCE_BITS_m_axi_bid-2]};
wire  m_axi_bid = PORT_BITS_m_axi_bid'(source_m_axi_bid);
localparam integer PORT_BITS_m_axi_bresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_bresp = PORT_BITS_m_axi_bresp < 32 ? 32 : PORT_BITS_m_axi_bresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bresp-1:0] source_m_axi_bresp = SOURCE_BITS_m_axi_bresp'(140);
always @(posedge clk) source_m_axi_bresp <= {source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2:0],source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-1]^source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2]};
wire [1:0] m_axi_bresp = PORT_BITS_m_axi_bresp'(source_m_axi_bresp);
localparam integer PORT_BITS_m_axi_bvalid = 1;
localparam integer SOURCE_BITS_m_axi_bvalid = PORT_BITS_m_axi_bvalid < 32 ? 32 : PORT_BITS_m_axi_bvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bvalid-1:0] source_m_axi_bvalid = SOURCE_BITS_m_axi_bvalid'(141);
always @(posedge clk) source_m_axi_bvalid <= {source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-2:0],source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-1]^source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-2]};
wire  m_axi_bvalid = PORT_BITS_m_axi_bvalid'(source_m_axi_bvalid);
wire  m_axi_bready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_bready;
always @(posedge clk) capture_m_axi_bready <= m_axi_bready;
wire  m_axi_arid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_arid;
always @(posedge clk) capture_m_axi_arid <= m_axi_arid;
wire [31:0] m_axi_araddr;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_m_axi_araddr;
always @(posedge clk) capture_m_axi_araddr <= m_axi_araddr;
wire [3:0] m_axi_arlen;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] capture_m_axi_arlen;
always @(posedge clk) capture_m_axi_arlen <= m_axi_arlen;
wire [2:0] m_axi_arsize;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [2:0] capture_m_axi_arsize;
always @(posedge clk) capture_m_axi_arsize <= m_axi_arsize;
wire [1:0] m_axi_arburst;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] capture_m_axi_arburst;
always @(posedge clk) capture_m_axi_arburst <= m_axi_arburst;
wire  m_axi_arvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_arvalid;
always @(posedge clk) capture_m_axi_arvalid <= m_axi_arvalid;
localparam integer PORT_BITS_m_axi_arready = 1;
localparam integer SOURCE_BITS_m_axi_arready = PORT_BITS_m_axi_arready < 32 ? 32 : PORT_BITS_m_axi_arready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_arready-1:0] source_m_axi_arready = SOURCE_BITS_m_axi_arready'(149);
always @(posedge clk) source_m_axi_arready <= {source_m_axi_arready[SOURCE_BITS_m_axi_arready-2:0],source_m_axi_arready[SOURCE_BITS_m_axi_arready-1]^source_m_axi_arready[SOURCE_BITS_m_axi_arready-2]};
wire  m_axi_arready = PORT_BITS_m_axi_arready'(source_m_axi_arready);
localparam integer PORT_BITS_m_axi_rid = 1;
localparam integer SOURCE_BITS_m_axi_rid = PORT_BITS_m_axi_rid < 32 ? 32 : PORT_BITS_m_axi_rid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rid-1:0] source_m_axi_rid = SOURCE_BITS_m_axi_rid'(150);
always @(posedge clk) source_m_axi_rid <= {source_m_axi_rid[SOURCE_BITS_m_axi_rid-2:0],source_m_axi_rid[SOURCE_BITS_m_axi_rid-1]^source_m_axi_rid[SOURCE_BITS_m_axi_rid-2]};
wire  m_axi_rid = PORT_BITS_m_axi_rid'(source_m_axi_rid);
localparam integer PORT_BITS_m_axi_rdata = (AXI_DATA_WIDTH-1)+1;
localparam integer SOURCE_BITS_m_axi_rdata = PORT_BITS_m_axi_rdata < 32 ? 32 : PORT_BITS_m_axi_rdata;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rdata-1:0] source_m_axi_rdata = SOURCE_BITS_m_axi_rdata'(151);
always @(posedge clk) source_m_axi_rdata <= {source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2:0],source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-1]^source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2]};
wire [AXI_DATA_WIDTH-1:0] m_axi_rdata = PORT_BITS_m_axi_rdata'(source_m_axi_rdata);
localparam integer PORT_BITS_m_axi_rresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_rresp = PORT_BITS_m_axi_rresp < 32 ? 32 : PORT_BITS_m_axi_rresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rresp-1:0] source_m_axi_rresp = SOURCE_BITS_m_axi_rresp'(152);
always @(posedge clk) source_m_axi_rresp <= {source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2:0],source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-1]^source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2]};
wire [1:0] m_axi_rresp = PORT_BITS_m_axi_rresp'(source_m_axi_rresp);
localparam integer PORT_BITS_m_axi_rlast = 1;
localparam integer SOURCE_BITS_m_axi_rlast = PORT_BITS_m_axi_rlast < 32 ? 32 : PORT_BITS_m_axi_rlast;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rlast-1:0] source_m_axi_rlast = SOURCE_BITS_m_axi_rlast'(153);
always @(posedge clk) source_m_axi_rlast <= {source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2:0],source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-1]^source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2]};
wire  m_axi_rlast = PORT_BITS_m_axi_rlast'(source_m_axi_rlast);
localparam integer PORT_BITS_m_axi_rvalid = 1;
localparam integer SOURCE_BITS_m_axi_rvalid = PORT_BITS_m_axi_rvalid < 32 ? 32 : PORT_BITS_m_axi_rvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rvalid-1:0] source_m_axi_rvalid = SOURCE_BITS_m_axi_rvalid'(154);
always @(posedge clk) source_m_axi_rvalid <= {source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2:0],source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-1]^source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2]};
wire  m_axi_rvalid = PORT_BITS_m_axi_rvalid'(source_m_axi_rvalid);
wire  m_axi_rready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_rready;
always @(posedge clk) capture_m_axi_rready <= m_axi_rready;
(* KEEP_HIERARCHY = "yes" *) media_audio_pipeline_axi #(
.EXTERNAL_IO(EXTERNAL_IO),
.LIVE_LOADING(LIVE_LOADING),
.EXPORT_SYNC_EVENTS(EXPORT_SYNC_EVENTS),
.PROGRAM_REGIONS(PROGRAM_REGIONS),
.PROGRAM_DEPTH(PROGRAM_DEPTH),
.STATE_DEPTH(STATE_DEPTH),
.JOB_DEPTH(JOB_DEPTH),
.MAX_FRAMES(MAX_FRAMES),
.BUFFERS(BUFFERS),
.INPUT_PORTS(INPUT_PORTS),
.OUTPUT_PORTS(OUTPUT_PORTS),
.REUSE_VIEWS(REUSE_VIEWS),
.MAINTENANCE_AT_RUN(MAINTENANCE_AT_RUN),
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
.PHYSICAL_BASE(PHYSICAL_BASE),
.PHYSICAL_BYTES_LIMIT(PHYSICAL_BYTES_LIMIT)
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
.maintenance_tick(maintenance_tick),
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
.run_program_region(run_program_region),
.run_sync_count(run_sync_count),
.switch_valid(switch_valid),
.switch_ready(switch_ready),
.switch_expected(switch_expected),
.switch_revision(switch_revision),
.switch_region(switch_region),
.switch_instructions(switch_instructions),
.switch_sync_count(switch_sync_count),
.switch_states(switch_states),
.switch_jobs(switch_jobs),
.switch_buffers(switch_buffers),
.switch_response_valid(switch_response_valid),
.switch_response_ready(switch_response_ready),
.switch_response_error(switch_response_error),
.switch_first_frame(switch_first_frame),
.active_revision(active_revision),
.sync_frame(sync_frame),
.sync_event_valid(sync_event_valid),
.sync_event_ready(sync_event_ready),
.sync_event_data(sync_event_data),
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
.cfg_loop(cfg_loop),
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
.loop_read_valid(loop_read_valid),
.loop_read_ready(loop_read_ready),
.loop_read_slot(loop_read_slot),
.loop_response_valid(loop_response_valid),
.loop_response_error(loop_response_error),
.loop_response_data(loop_response_data),
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
module midi_context #(

    parameter integer DATA_WIDTH=32,
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8,
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, SNAPSHOTS=768, RETIREMENTS=256,
    parameter integer INPUT_EVENTS=256, MAINTENANCE=64,
    parameter integer TABLE_ENTRIES=(MAX_EVENTS+2*BLOCK_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1

) (input wire clk);
reg [7:0] startup = 0;
always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;
reg reset = 1;
always @(posedge clk) reset <= !(&startup);
wire [63:0] arena_base = 64'd0;
wire [31:0] quantum_frames = 32'd32;
wire [63:0] arena_bytes;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_arena_bytes;
always @(posedge clk) capture_arena_bytes <= arena_bytes;
localparam integer PORT_BITS_command_valid = 1;
localparam integer SOURCE_BITS_command_valid = PORT_BITS_command_valid < 32 ? 32 : PORT_BITS_command_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_valid-1:0] source_command_valid = SOURCE_BITS_command_valid'(6);
always @(posedge clk) source_command_valid <= {source_command_valid[SOURCE_BITS_command_valid-2:0],source_command_valid[SOURCE_BITS_command_valid-1]^source_command_valid[SOURCE_BITS_command_valid-2]};
wire  command_valid = PORT_BITS_command_valid'(source_command_valid);
wire  command_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_command_ready;
always @(posedge clk) capture_command_ready <= command_ready;
localparam integer PORT_BITS_command_op = (4)+1;
localparam integer SOURCE_BITS_command_op = PORT_BITS_command_op < 32 ? 32 : PORT_BITS_command_op;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_op-1:0] source_command_op = SOURCE_BITS_command_op'(8);
always @(posedge clk) source_command_op <= {source_command_op[SOURCE_BITS_command_op-2:0],source_command_op[SOURCE_BITS_command_op-1]^source_command_op[SOURCE_BITS_command_op-2]};
wire [4:0] command_op = PORT_BITS_command_op'(source_command_op);
localparam integer PORT_BITS_command_arg0 = (63)+1;
localparam integer SOURCE_BITS_command_arg0 = PORT_BITS_command_arg0 < 32 ? 32 : PORT_BITS_command_arg0;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_arg0-1:0] source_command_arg0 = SOURCE_BITS_command_arg0'(9);
always @(posedge clk) source_command_arg0 <= {source_command_arg0[SOURCE_BITS_command_arg0-2:0],source_command_arg0[SOURCE_BITS_command_arg0-1]^source_command_arg0[SOURCE_BITS_command_arg0-2]};
wire [63:0] command_arg0 = PORT_BITS_command_arg0'(source_command_arg0);
localparam integer PORT_BITS_command_arg1 = (63)+1;
localparam integer SOURCE_BITS_command_arg1 = PORT_BITS_command_arg1 < 32 ? 32 : PORT_BITS_command_arg1;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_arg1-1:0] source_command_arg1 = SOURCE_BITS_command_arg1'(10);
always @(posedge clk) source_command_arg1 <= {source_command_arg1[SOURCE_BITS_command_arg1-2:0],source_command_arg1[SOURCE_BITS_command_arg1-1]^source_command_arg1[SOURCE_BITS_command_arg1-2]};
wire [63:0] command_arg1 = PORT_BITS_command_arg1'(source_command_arg1);
localparam integer PORT_BITS_command_arg2 = (63)+1;
localparam integer SOURCE_BITS_command_arg2 = PORT_BITS_command_arg2 < 32 ? 32 : PORT_BITS_command_arg2;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_arg2-1:0] source_command_arg2 = SOURCE_BITS_command_arg2'(11);
always @(posedge clk) source_command_arg2 <= {source_command_arg2[SOURCE_BITS_command_arg2-2:0],source_command_arg2[SOURCE_BITS_command_arg2-1]^source_command_arg2[SOURCE_BITS_command_arg2-2]};
wire [63:0] command_arg2 = PORT_BITS_command_arg2'(source_command_arg2);
localparam integer PORT_BITS_command_arg3 = (63)+1;
localparam integer SOURCE_BITS_command_arg3 = PORT_BITS_command_arg3 < 32 ? 32 : PORT_BITS_command_arg3;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_arg3-1:0] source_command_arg3 = SOURCE_BITS_command_arg3'(12);
always @(posedge clk) source_command_arg3 <= {source_command_arg3[SOURCE_BITS_command_arg3-2:0],source_command_arg3[SOURCE_BITS_command_arg3-1]^source_command_arg3[SOURCE_BITS_command_arg3-2]};
wire [63:0] command_arg3 = PORT_BITS_command_arg3'(source_command_arg3);
localparam integer PORT_BITS_command_arg4 = (63)+1;
localparam integer SOURCE_BITS_command_arg4 = PORT_BITS_command_arg4 < 32 ? 32 : PORT_BITS_command_arg4;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_arg4-1:0] source_command_arg4 = SOURCE_BITS_command_arg4'(13);
always @(posedge clk) source_command_arg4 <= {source_command_arg4[SOURCE_BITS_command_arg4-2:0],source_command_arg4[SOURCE_BITS_command_arg4-1]^source_command_arg4[SOURCE_BITS_command_arg4-2]};
wire [63:0] command_arg4 = PORT_BITS_command_arg4'(source_command_arg4);
wire  response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_response_valid;
always @(posedge clk) capture_response_valid <= response_valid;
localparam integer PORT_BITS_response_ready = 1;
localparam integer SOURCE_BITS_response_ready = PORT_BITS_response_ready < 32 ? 32 : PORT_BITS_response_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_response_ready-1:0] source_response_ready = SOURCE_BITS_response_ready'(15);
always @(posedge clk) source_response_ready <= {source_response_ready[SOURCE_BITS_response_ready-2:0],source_response_ready[SOURCE_BITS_response_ready-1]^source_response_ready[SOURCE_BITS_response_ready-2]};
wire  response_ready = PORT_BITS_response_ready'(source_response_ready);
wire [2:0] response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [2:0] capture_response_error;
always @(posedge clk) capture_response_error <= response_error;
wire [63:0] response_handle;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_response_handle;
always @(posedge clk) capture_response_handle <= response_handle;
wire [63:0] response_frame;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_response_frame;
always @(posedge clk) capture_response_frame <= response_frame;
wire [63:0] response_state;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_response_state;
always @(posedge clk) capture_response_state <= response_state;
wire [31:0] response_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_response_count;
always @(posedge clk) capture_response_count <= response_count;
wire [23:0] response_packet;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [23:0] capture_response_packet;
always @(posedge clk) capture_response_packet <= response_packet;
wire [1:0] response_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] capture_response_length;
always @(posedge clk) capture_response_length <= response_length;
wire  packet_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_packet_valid;
always @(posedge clk) capture_packet_valid <= packet_valid;
localparam integer PORT_BITS_packet_ready = 1;
localparam integer SOURCE_BITS_packet_ready = PORT_BITS_packet_ready < 32 ? 32 : PORT_BITS_packet_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_packet_ready-1:0] source_packet_ready = SOURCE_BITS_packet_ready'(24);
always @(posedge clk) source_packet_ready <= {source_packet_ready[SOURCE_BITS_packet_ready-2:0],source_packet_ready[SOURCE_BITS_packet_ready-1]^source_packet_ready[SOURCE_BITS_packet_ready-2]};
wire  packet_ready = PORT_BITS_packet_ready'(source_packet_ready);
localparam integer PORT_BITS_packet_refused = 1;
localparam integer SOURCE_BITS_packet_refused = PORT_BITS_packet_refused < 32 ? 32 : PORT_BITS_packet_refused;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_packet_refused-1:0] source_packet_refused = SOURCE_BITS_packet_refused'(25);
always @(posedge clk) source_packet_refused <= {source_packet_refused[SOURCE_BITS_packet_refused-2:0],source_packet_refused[SOURCE_BITS_packet_refused-1]^source_packet_refused[SOURCE_BITS_packet_refused-2]};
wire  packet_refused = PORT_BITS_packet_refused'(source_packet_refused);
wire [23:0] packet_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [23:0] capture_packet_data;
always @(posedge clk) capture_packet_data <= packet_data;
wire [1:0] packet_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] capture_packet_length;
always @(posedge clk) capture_packet_length <= packet_length;
wire [31:0] packet_destination;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_packet_destination;
always @(posedge clk) capture_packet_destination <= packet_destination;
wire [31:0] packet_offset;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_packet_offset;
always @(posedge clk) capture_packet_offset <= packet_offset;
wire [63:0] memory_arena_bytes = 64'd67108864;
wire [31:0] physical_base = 32'h30000000;
wire  m_axi_awid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_awid;
always @(posedge clk) capture_m_axi_awid <= m_axi_awid;
wire [31:0] m_axi_awaddr;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_m_axi_awaddr;
always @(posedge clk) capture_m_axi_awaddr <= m_axi_awaddr;
wire [3:0] m_axi_awlen;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] capture_m_axi_awlen;
always @(posedge clk) capture_m_axi_awlen <= m_axi_awlen;
wire [2:0] m_axi_awsize;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [2:0] capture_m_axi_awsize;
always @(posedge clk) capture_m_axi_awsize <= m_axi_awsize;
wire [1:0] m_axi_awburst;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] capture_m_axi_awburst;
always @(posedge clk) capture_m_axi_awburst <= m_axi_awburst;
wire  m_axi_awvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_awvalid;
always @(posedge clk) capture_m_axi_awvalid <= m_axi_awvalid;
localparam integer PORT_BITS_m_axi_awready = 1;
localparam integer SOURCE_BITS_m_axi_awready = PORT_BITS_m_axi_awready < 32 ? 32 : PORT_BITS_m_axi_awready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_awready-1:0] source_m_axi_awready = SOURCE_BITS_m_axi_awready'(38);
always @(posedge clk) source_m_axi_awready <= {source_m_axi_awready[SOURCE_BITS_m_axi_awready-2:0],source_m_axi_awready[SOURCE_BITS_m_axi_awready-1]^source_m_axi_awready[SOURCE_BITS_m_axi_awready-2]};
wire  m_axi_awready = PORT_BITS_m_axi_awready'(source_m_axi_awready);
wire [DATA_WIDTH-1:0] m_axi_wdata;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [DATA_WIDTH-1:0] capture_m_axi_wdata;
always @(posedge clk) capture_m_axi_wdata <= m_axi_wdata;
wire [DATA_WIDTH/8-1:0] m_axi_wstrb;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [DATA_WIDTH/8-1:0] capture_m_axi_wstrb;
always @(posedge clk) capture_m_axi_wstrb <= m_axi_wstrb;
wire  m_axi_wid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_wid;
always @(posedge clk) capture_m_axi_wid <= m_axi_wid;
wire  m_axi_wlast;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_wlast;
always @(posedge clk) capture_m_axi_wlast <= m_axi_wlast;
wire  m_axi_wvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_wvalid;
always @(posedge clk) capture_m_axi_wvalid <= m_axi_wvalid;
localparam integer PORT_BITS_m_axi_wready = 1;
localparam integer SOURCE_BITS_m_axi_wready = PORT_BITS_m_axi_wready < 32 ? 32 : PORT_BITS_m_axi_wready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_wready-1:0] source_m_axi_wready = SOURCE_BITS_m_axi_wready'(44);
always @(posedge clk) source_m_axi_wready <= {source_m_axi_wready[SOURCE_BITS_m_axi_wready-2:0],source_m_axi_wready[SOURCE_BITS_m_axi_wready-1]^source_m_axi_wready[SOURCE_BITS_m_axi_wready-2]};
wire  m_axi_wready = PORT_BITS_m_axi_wready'(source_m_axi_wready);
localparam integer PORT_BITS_m_axi_bid = 1;
localparam integer SOURCE_BITS_m_axi_bid = PORT_BITS_m_axi_bid < 32 ? 32 : PORT_BITS_m_axi_bid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bid-1:0] source_m_axi_bid = SOURCE_BITS_m_axi_bid'(45);
always @(posedge clk) source_m_axi_bid <= {source_m_axi_bid[SOURCE_BITS_m_axi_bid-2:0],source_m_axi_bid[SOURCE_BITS_m_axi_bid-1]^source_m_axi_bid[SOURCE_BITS_m_axi_bid-2]};
wire  m_axi_bid = PORT_BITS_m_axi_bid'(source_m_axi_bid);
localparam integer PORT_BITS_m_axi_bresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_bresp = PORT_BITS_m_axi_bresp < 32 ? 32 : PORT_BITS_m_axi_bresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bresp-1:0] source_m_axi_bresp = SOURCE_BITS_m_axi_bresp'(46);
always @(posedge clk) source_m_axi_bresp <= {source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2:0],source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-1]^source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2]};
wire [1:0] m_axi_bresp = PORT_BITS_m_axi_bresp'(source_m_axi_bresp);
localparam integer PORT_BITS_m_axi_bvalid = 1;
localparam integer SOURCE_BITS_m_axi_bvalid = PORT_BITS_m_axi_bvalid < 32 ? 32 : PORT_BITS_m_axi_bvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bvalid-1:0] source_m_axi_bvalid = SOURCE_BITS_m_axi_bvalid'(47);
always @(posedge clk) source_m_axi_bvalid <= {source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-2:0],source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-1]^source_m_axi_bvalid[SOURCE_BITS_m_axi_bvalid-2]};
wire  m_axi_bvalid = PORT_BITS_m_axi_bvalid'(source_m_axi_bvalid);
wire  m_axi_bready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_bready;
always @(posedge clk) capture_m_axi_bready <= m_axi_bready;
wire  m_axi_arid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_arid;
always @(posedge clk) capture_m_axi_arid <= m_axi_arid;
wire [31:0] m_axi_araddr;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_m_axi_araddr;
always @(posedge clk) capture_m_axi_araddr <= m_axi_araddr;
wire [3:0] m_axi_arlen;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] capture_m_axi_arlen;
always @(posedge clk) capture_m_axi_arlen <= m_axi_arlen;
wire [2:0] m_axi_arsize;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [2:0] capture_m_axi_arsize;
always @(posedge clk) capture_m_axi_arsize <= m_axi_arsize;
wire [1:0] m_axi_arburst;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [1:0] capture_m_axi_arburst;
always @(posedge clk) capture_m_axi_arburst <= m_axi_arburst;
wire  m_axi_arvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_arvalid;
always @(posedge clk) capture_m_axi_arvalid <= m_axi_arvalid;
localparam integer PORT_BITS_m_axi_arready = 1;
localparam integer SOURCE_BITS_m_axi_arready = PORT_BITS_m_axi_arready < 32 ? 32 : PORT_BITS_m_axi_arready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_arready-1:0] source_m_axi_arready = SOURCE_BITS_m_axi_arready'(55);
always @(posedge clk) source_m_axi_arready <= {source_m_axi_arready[SOURCE_BITS_m_axi_arready-2:0],source_m_axi_arready[SOURCE_BITS_m_axi_arready-1]^source_m_axi_arready[SOURCE_BITS_m_axi_arready-2]};
wire  m_axi_arready = PORT_BITS_m_axi_arready'(source_m_axi_arready);
localparam integer PORT_BITS_m_axi_rid = 1;
localparam integer SOURCE_BITS_m_axi_rid = PORT_BITS_m_axi_rid < 32 ? 32 : PORT_BITS_m_axi_rid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rid-1:0] source_m_axi_rid = SOURCE_BITS_m_axi_rid'(56);
always @(posedge clk) source_m_axi_rid <= {source_m_axi_rid[SOURCE_BITS_m_axi_rid-2:0],source_m_axi_rid[SOURCE_BITS_m_axi_rid-1]^source_m_axi_rid[SOURCE_BITS_m_axi_rid-2]};
wire  m_axi_rid = PORT_BITS_m_axi_rid'(source_m_axi_rid);
localparam integer PORT_BITS_m_axi_rdata = (DATA_WIDTH-1)+1;
localparam integer SOURCE_BITS_m_axi_rdata = PORT_BITS_m_axi_rdata < 32 ? 32 : PORT_BITS_m_axi_rdata;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rdata-1:0] source_m_axi_rdata = SOURCE_BITS_m_axi_rdata'(57);
always @(posedge clk) source_m_axi_rdata <= {source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2:0],source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-1]^source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2]};
wire [DATA_WIDTH-1:0] m_axi_rdata = PORT_BITS_m_axi_rdata'(source_m_axi_rdata);
localparam integer PORT_BITS_m_axi_rresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_rresp = PORT_BITS_m_axi_rresp < 32 ? 32 : PORT_BITS_m_axi_rresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rresp-1:0] source_m_axi_rresp = SOURCE_BITS_m_axi_rresp'(58);
always @(posedge clk) source_m_axi_rresp <= {source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2:0],source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-1]^source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2]};
wire [1:0] m_axi_rresp = PORT_BITS_m_axi_rresp'(source_m_axi_rresp);
localparam integer PORT_BITS_m_axi_rlast = 1;
localparam integer SOURCE_BITS_m_axi_rlast = PORT_BITS_m_axi_rlast < 32 ? 32 : PORT_BITS_m_axi_rlast;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rlast-1:0] source_m_axi_rlast = SOURCE_BITS_m_axi_rlast'(59);
always @(posedge clk) source_m_axi_rlast <= {source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2:0],source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-1]^source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2]};
wire  m_axi_rlast = PORT_BITS_m_axi_rlast'(source_m_axi_rlast);
localparam integer PORT_BITS_m_axi_rvalid = 1;
localparam integer SOURCE_BITS_m_axi_rvalid = PORT_BITS_m_axi_rvalid < 32 ? 32 : PORT_BITS_m_axi_rvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rvalid-1:0] source_m_axi_rvalid = SOURCE_BITS_m_axi_rvalid'(60);
always @(posedge clk) source_m_axi_rvalid <= {source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2:0],source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-1]^source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2]};
wire  m_axi_rvalid = PORT_BITS_m_axi_rvalid'(source_m_axi_rvalid);
wire  m_axi_rready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_m_axi_rready;
always @(posedge clk) capture_m_axi_rready <= m_axi_rready;
(* KEEP_HIERARCHY = "yes" *) midi_runtime_axi #(
.DATA_WIDTH(DATA_WIDTH),
.LOOPS(LOOPS),
.INPUTS(INPUTS),
.DESTINATIONS(DESTINATIONS),
.BLOCK_EVENTS(BLOCK_EVENTS),
.BLOCKS(BLOCKS),
.MAX_EVENTS(MAX_EVENTS),
.VIEWS(VIEWS),
.SNAPSHOTS(SNAPSHOTS),
.RETIREMENTS(RETIREMENTS),
.INPUT_EVENTS(INPUT_EVENTS),
.MAINTENANCE(MAINTENANCE),
.TABLE_ENTRIES(TABLE_ENTRIES)
) core_under_test (
.clk(clk),
.reset(reset),
.arena_base(arena_base),
.quantum_frames(quantum_frames),
.arena_bytes(arena_bytes),
.command_valid(command_valid),
.command_ready(command_ready),
.command_op(command_op),
.command_arg0(command_arg0),
.command_arg1(command_arg1),
.command_arg2(command_arg2),
.command_arg3(command_arg3),
.command_arg4(command_arg4),
.response_valid(response_valid),
.response_ready(response_ready),
.response_error(response_error),
.response_handle(response_handle),
.response_frame(response_frame),
.response_state(response_state),
.response_count(response_count),
.response_packet(response_packet),
.response_length(response_length),
.packet_valid(packet_valid),
.packet_ready(packet_ready),
.packet_refused(packet_refused),
.packet_data(packet_data),
.packet_length(packet_length),
.packet_destination(packet_destination),
.packet_offset(packet_offset),
.memory_arena_bytes(memory_arena_bytes),
.physical_base(physical_base),
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
module session_context(input wire clk);
(* KEEP_HIERARCHY = "yes" *) audio_context #(
.EXTERNAL_IO(1),
.LIVE_LOADING(1),
.EXPORT_SYNC_EVENTS(1),
.PROGRAM_DEPTH(1024),
.STATE_DEPTH(256),
.JOB_DEPTH(1024),
.MAX_FRAMES(32),
.BUFFERS(512),
.INPUT_PORTS(2),
.OUTPUT_PORTS(2),
.REUSE_VIEWS(1),
.VIEWS(512),
.SLOTS(5763072),
.CHUNK_FRAMES(32),
.BLOCK_CHUNKS(32),
.RETIRED_TABLES(256),
.TABLE_LINES(128),
.REF_LINES(4),
.STACK_LINES(2),
.READ_BUFFERS(4),
.WRITE_BUFFERS(4),
.BURST_MEMORY(1),
.AXI_DATA_WIDTH(64),
.PHYSICAL_BASE(16777216),
.PHYSICAL_BYTES_LIMIT(1073741824)) audio (.clk(clk));
(* KEEP_HIERARCHY = "yes" *) midi_context #(
.DATA_WIDTH(64),
.LOOPS(64),
.INPUTS(1),
.DESTINATIONS(8),
.BLOCK_EVENTS(256),
.BLOCKS(2432),
.MAX_EVENTS(8192),
.VIEWS(256),
.SNAPSHOTS(384),
.RETIREMENTS(128),
.INPUT_EVENTS(256),
.MAINTENANCE(64)) midi (.clk(clk));
endmodule
