`include "state_processor_defs.svh"
module registered_core_context #(

    parameter integer REUSE_VIEWS = 0,
    parameter integer VIEWS = 32,
    parameter integer SLOTS = 1024,
    parameter integer CHUNK_FRAMES = 32,
    parameter integer BLOCK_CHUNKS = 32,
    parameter integer RETIRED_TABLES = 256,
    parameter integer RECLAIM_BLOCKS = 0, parameter integer RECLAIM_CYCLES = 5000,
    parameter integer RECLAIM_GUARD = 1024,
    parameter integer TABLE_PAGES = (SLOTS/BLOCK_CHUNKS+14)/15 + 8*VIEWS + 16,
    parameter integer TABLE_LINES = 8, parameter integer REF_LINES = 4, parameter integer STACK_LINES = 2,
    parameter integer READ_BUFFERS = 4,
    parameter integer WRITE_BUFFERS = 4, parameter integer BURST_MEMORY = 1,
    parameter integer VIEW_BITS = (VIEWS < 2) ? 1 : $clog2(VIEWS),
    parameter integer SLOT_BITS = (SLOTS/BLOCK_CHUNKS < 2) ? 1 : $clog2(SLOTS/BLOCK_CHUNKS),
    parameter integer TABLE_BITS = VIEW_BITS + SLOT_BITS,
    parameter integer AXI_DATA_WIDTH = 32,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000

) (input wire clk);
reg [7:0] startup = 0;
always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;
reg reset = 1;
always @(posedge clk) reset <= !(&startup);
localparam integer PORT_BITS_maintenance_enable = 1;
localparam integer SOURCE_BITS_maintenance_enable = PORT_BITS_maintenance_enable < 32 ? 32 : PORT_BITS_maintenance_enable;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_maintenance_enable-1:0] source_maintenance_enable = SOURCE_BITS_maintenance_enable'(3);
always @(posedge clk) source_maintenance_enable <= {source_maintenance_enable[SOURCE_BITS_maintenance_enable-2:0],source_maintenance_enable[SOURCE_BITS_maintenance_enable-1]^source_maintenance_enable[SOURCE_BITS_maintenance_enable-2]};
wire  maintenance_enable = PORT_BITS_maintenance_enable'(source_maintenance_enable);
localparam integer PORT_BITS_maintenance_tick = 1;
localparam integer SOURCE_BITS_maintenance_tick = PORT_BITS_maintenance_tick < 32 ? 32 : PORT_BITS_maintenance_tick;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_maintenance_tick-1:0] source_maintenance_tick = SOURCE_BITS_maintenance_tick'(4);
always @(posedge clk) source_maintenance_tick <= {source_maintenance_tick[SOURCE_BITS_maintenance_tick-2:0],source_maintenance_tick[SOURCE_BITS_maintenance_tick-1]^source_maintenance_tick[SOURCE_BITS_maintenance_tick-2]};
wire  maintenance_tick = PORT_BITS_maintenance_tick'(source_maintenance_tick);
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
localparam integer PORT_BITS_arena_base = (63)+1;
localparam integer SOURCE_BITS_arena_base = PORT_BITS_arena_base < 32 ? 32 : PORT_BITS_arena_base;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_arena_base-1:0] source_arena_base = SOURCE_BITS_arena_base'(19);
always @(posedge clk) source_arena_base <= {source_arena_base[SOURCE_BITS_arena_base-2:0],source_arena_base[SOURCE_BITS_arena_base-1]^source_arena_base[SOURCE_BITS_arena_base-2]};
wire [63:0] arena_base = PORT_BITS_arena_base'(source_arena_base);
localparam integer PORT_BITS_hint_valid = 1;
localparam integer SOURCE_BITS_hint_valid = PORT_BITS_hint_valid < 32 ? 32 : PORT_BITS_hint_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_hint_valid-1:0] source_hint_valid = SOURCE_BITS_hint_valid'(20);
always @(posedge clk) source_hint_valid <= {source_hint_valid[SOURCE_BITS_hint_valid-2:0],source_hint_valid[SOURCE_BITS_hint_valid-1]^source_hint_valid[SOURCE_BITS_hint_valid-2]};
wire  hint_valid = PORT_BITS_hint_valid'(source_hint_valid);
wire  hint_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_hint_ready;
always @(posedge clk) unused_capture_hint_ready <= hint_ready;
localparam integer PORT_BITS_hint_view = (63)+1;
localparam integer SOURCE_BITS_hint_view = PORT_BITS_hint_view < 32 ? 32 : PORT_BITS_hint_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_hint_view-1:0] source_hint_view = SOURCE_BITS_hint_view'(22);
always @(posedge clk) source_hint_view <= {source_hint_view[SOURCE_BITS_hint_view-2:0],source_hint_view[SOURCE_BITS_hint_view-1]^source_hint_view[SOURCE_BITS_hint_view-2]};
wire [63:0] hint_view = PORT_BITS_hint_view'(source_hint_view);
localparam integer PORT_BITS_hint_position = (63)+1;
localparam integer SOURCE_BITS_hint_position = PORT_BITS_hint_position < 32 ? 32 : PORT_BITS_hint_position;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_hint_position-1:0] source_hint_position = SOURCE_BITS_hint_position'(23);
always @(posedge clk) source_hint_position <= {source_hint_position[SOURCE_BITS_hint_position-2:0],source_hint_position[SOURCE_BITS_hint_position-1]^source_hint_position[SOURCE_BITS_hint_position-2]};
wire [63:0] hint_position = PORT_BITS_hint_position'(source_hint_position);
localparam integer PORT_BITS_hint_length = (63)+1;
localparam integer SOURCE_BITS_hint_length = PORT_BITS_hint_length < 32 ? 32 : PORT_BITS_hint_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_hint_length-1:0] source_hint_length = SOURCE_BITS_hint_length'(24);
always @(posedge clk) source_hint_length <= {source_hint_length[SOURCE_BITS_hint_length-2:0],source_hint_length[SOURCE_BITS_hint_length-1]^source_hint_length[SOURCE_BITS_hint_length-2]};
wire [63:0] hint_length = PORT_BITS_hint_length'(source_hint_length);
localparam integer PORT_BITS_hint_count = (31)+1;
localparam integer SOURCE_BITS_hint_count = PORT_BITS_hint_count < 32 ? 32 : PORT_BITS_hint_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_hint_count-1:0] source_hint_count = SOURCE_BITS_hint_count'(25);
always @(posedge clk) source_hint_count <= {source_hint_count[SOURCE_BITS_hint_count-2:0],source_hint_count[SOURCE_BITS_hint_count-1]^source_hint_count[SOURCE_BITS_hint_count-2]};
wire [31:0] hint_count = PORT_BITS_hint_count'(source_hint_count);
localparam integer PORT_BITS_cmd_valid = 1;
localparam integer SOURCE_BITS_cmd_valid = PORT_BITS_cmd_valid < 32 ? 32 : PORT_BITS_cmd_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cmd_valid-1:0] source_cmd_valid = SOURCE_BITS_cmd_valid'(26);
always @(posedge clk) source_cmd_valid <= {source_cmd_valid[SOURCE_BITS_cmd_valid-2:0],source_cmd_valid[SOURCE_BITS_cmd_valid-1]^source_cmd_valid[SOURCE_BITS_cmd_valid-2]};
wire  cmd_valid = PORT_BITS_cmd_valid'(source_cmd_valid);
wire  cmd_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cmd_ready;
always @(posedge clk) unused_capture_cmd_ready <= cmd_ready;
localparam integer PORT_BITS_cmd_opcode = (3)+1;
localparam integer SOURCE_BITS_cmd_opcode = PORT_BITS_cmd_opcode < 32 ? 32 : PORT_BITS_cmd_opcode;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cmd_opcode-1:0] source_cmd_opcode = SOURCE_BITS_cmd_opcode'(28);
always @(posedge clk) source_cmd_opcode <= {source_cmd_opcode[SOURCE_BITS_cmd_opcode-2:0],source_cmd_opcode[SOURCE_BITS_cmd_opcode-1]^source_cmd_opcode[SOURCE_BITS_cmd_opcode-2]};
wire [3:0] cmd_opcode = PORT_BITS_cmd_opcode'(source_cmd_opcode);
localparam integer PORT_BITS_cmd_view = (63)+1;
localparam integer SOURCE_BITS_cmd_view = PORT_BITS_cmd_view < 32 ? 32 : PORT_BITS_cmd_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cmd_view-1:0] source_cmd_view = SOURCE_BITS_cmd_view'(29);
always @(posedge clk) source_cmd_view <= {source_cmd_view[SOURCE_BITS_cmd_view-2:0],source_cmd_view[SOURCE_BITS_cmd_view-1]^source_cmd_view[SOURCE_BITS_cmd_view-2]};
wire [63:0] cmd_view = PORT_BITS_cmd_view'(source_cmd_view);
localparam integer PORT_BITS_cmd_start = (63)+1;
localparam integer SOURCE_BITS_cmd_start = PORT_BITS_cmd_start < 32 ? 32 : PORT_BITS_cmd_start;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cmd_start-1:0] source_cmd_start = SOURCE_BITS_cmd_start'(30);
always @(posedge clk) source_cmd_start <= {source_cmd_start[SOURCE_BITS_cmd_start-2:0],source_cmd_start[SOURCE_BITS_cmd_start-1]^source_cmd_start[SOURCE_BITS_cmd_start-2]};
wire [63:0] cmd_start = PORT_BITS_cmd_start'(source_cmd_start);
localparam integer PORT_BITS_cmd_length = (63)+1;
localparam integer SOURCE_BITS_cmd_length = PORT_BITS_cmd_length < 32 ? 32 : PORT_BITS_cmd_length;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cmd_length-1:0] source_cmd_length = SOURCE_BITS_cmd_length'(31);
always @(posedge clk) source_cmd_length <= {source_cmd_length[SOURCE_BITS_cmd_length-2:0],source_cmd_length[SOURCE_BITS_cmd_length-1]^source_cmd_length[SOURCE_BITS_cmd_length-2]};
wire [63:0] cmd_length = PORT_BITS_cmd_length'(source_cmd_length);
localparam integer PORT_BITS_cmd_count = (31)+1;
localparam integer SOURCE_BITS_cmd_count = PORT_BITS_cmd_count < 32 ? 32 : PORT_BITS_cmd_count;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cmd_count-1:0] source_cmd_count = SOURCE_BITS_cmd_count'(32);
always @(posedge clk) source_cmd_count <= {source_cmd_count[SOURCE_BITS_cmd_count-2:0],source_cmd_count[SOURCE_BITS_cmd_count-1]^source_cmd_count[SOURCE_BITS_cmd_count-2]};
wire [31:0] cmd_count = PORT_BITS_cmd_count'(source_cmd_count);
wire  busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_busy;
always @(posedge clk) unused_capture_busy <= busy;
wire  done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_done;
always @(posedge clk) unused_capture_done <= done;
wire [7:0] error_code;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [7:0] unused_capture_error_code;
always @(posedge clk) unused_capture_error_code <= error_code;
wire  memory_fault;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_memory_fault;
always @(posedge clk) unused_capture_memory_fault <= memory_fault;
wire [31:0] result_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_result_view;
always @(posedge clk) unused_capture_result_view <= result_view;
wire  result_empty;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_result_empty;
always @(posedge clk) unused_capture_result_empty <= result_empty;
localparam integer PORT_BITS_write_valid = 1;
localparam integer SOURCE_BITS_write_valid = PORT_BITS_write_valid < 32 ? 32 : PORT_BITS_write_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_write_valid-1:0] source_write_valid = SOURCE_BITS_write_valid'(39);
always @(posedge clk) source_write_valid <= {source_write_valid[SOURCE_BITS_write_valid-2:0],source_write_valid[SOURCE_BITS_write_valid-1]^source_write_valid[SOURCE_BITS_write_valid-2]};
wire  write_valid = PORT_BITS_write_valid'(source_write_valid);
wire  write_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_write_ready;
always @(posedge clk) unused_capture_write_ready <= write_ready;
localparam integer PORT_BITS_write_data = (31)+1;
localparam integer SOURCE_BITS_write_data = PORT_BITS_write_data < 32 ? 32 : PORT_BITS_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_write_data-1:0] source_write_data = SOURCE_BITS_write_data'(41);
always @(posedge clk) source_write_data <= {source_write_data[SOURCE_BITS_write_data-2:0],source_write_data[SOURCE_BITS_write_data-1]^source_write_data[SOURCE_BITS_write_data-2]};
wire [31:0] write_data = PORT_BITS_write_data'(source_write_data);
wire  read_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_read_valid;
always @(posedge clk) unused_capture_read_valid <= read_valid;
localparam integer PORT_BITS_read_ready = 1;
localparam integer SOURCE_BITS_read_ready = PORT_BITS_read_ready < 32 ? 32 : PORT_BITS_read_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_read_ready-1:0] source_read_ready = SOURCE_BITS_read_ready'(43);
always @(posedge clk) source_read_ready <= {source_read_ready[SOURCE_BITS_read_ready-2:0],source_read_ready[SOURCE_BITS_read_ready-1]^source_read_ready[SOURCE_BITS_read_ready-2]};
wire  read_ready = PORT_BITS_read_ready'(source_read_ready);
wire [31:0] read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_read_data;
always @(posedge clk) unused_capture_read_data <= read_data;
localparam integer PORT_BITS_cfg_valid = 1;
localparam integer SOURCE_BITS_cfg_valid = PORT_BITS_cfg_valid < 32 ? 32 : PORT_BITS_cfg_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_valid-1:0] source_cfg_valid = SOURCE_BITS_cfg_valid'(45);
always @(posedge clk) source_cfg_valid <= {source_cfg_valid[SOURCE_BITS_cfg_valid-2:0],source_cfg_valid[SOURCE_BITS_cfg_valid-1]^source_cfg_valid[SOURCE_BITS_cfg_valid-2]};
wire  cfg_valid = PORT_BITS_cfg_valid'(source_cfg_valid);
wire  cfg_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_ready;
always @(posedge clk) unused_capture_cfg_ready <= cfg_ready;
localparam integer PORT_BITS_cfg_write = 1;
localparam integer SOURCE_BITS_cfg_write = PORT_BITS_cfg_write < 32 ? 32 : PORT_BITS_cfg_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write-1:0] source_cfg_write = SOURCE_BITS_cfg_write'(47);
always @(posedge clk) source_cfg_write <= {source_cfg_write[SOURCE_BITS_cfg_write-2:0],source_cfg_write[SOURCE_BITS_cfg_write-1]^source_cfg_write[SOURCE_BITS_cfg_write-2]};
wire  cfg_write = PORT_BITS_cfg_write'(source_cfg_write);
localparam integer PORT_BITS_cfg_space = (2)+1;
localparam integer SOURCE_BITS_cfg_space = PORT_BITS_cfg_space < 32 ? 32 : PORT_BITS_cfg_space;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_space-1:0] source_cfg_space = SOURCE_BITS_cfg_space'(48);
always @(posedge clk) source_cfg_space <= {source_cfg_space[SOURCE_BITS_cfg_space-2:0],source_cfg_space[SOURCE_BITS_cfg_space-1]^source_cfg_space[SOURCE_BITS_cfg_space-2]};
wire [2:0] cfg_space = PORT_BITS_cfg_space'(source_cfg_space);
localparam integer PORT_BITS_cfg_address = (31)+1;
localparam integer SOURCE_BITS_cfg_address = PORT_BITS_cfg_address < 32 ? 32 : PORT_BITS_cfg_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_address-1:0] source_cfg_address = SOURCE_BITS_cfg_address'(49);
always @(posedge clk) source_cfg_address <= {source_cfg_address[SOURCE_BITS_cfg_address-2:0],source_cfg_address[SOURCE_BITS_cfg_address-1]^source_cfg_address[SOURCE_BITS_cfg_address-2]};
wire [31:0] cfg_address = PORT_BITS_cfg_address'(source_cfg_address);
localparam integer PORT_BITS_cfg_write_data = (159)+1;
localparam integer SOURCE_BITS_cfg_write_data = PORT_BITS_cfg_write_data < 32 ? 32 : PORT_BITS_cfg_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_cfg_write_data-1:0] source_cfg_write_data = SOURCE_BITS_cfg_write_data'(50);
always @(posedge clk) source_cfg_write_data <= {source_cfg_write_data[SOURCE_BITS_cfg_write_data-2:0],source_cfg_write_data[SOURCE_BITS_cfg_write_data-1]^source_cfg_write_data[SOURCE_BITS_cfg_write_data-2]};
wire [159:0] cfg_write_data = PORT_BITS_cfg_write_data'(source_cfg_write_data);
wire  cfg_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_response_valid;
always @(posedge clk) unused_capture_cfg_response_valid <= cfg_response_valid;
wire  cfg_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_cfg_response_error;
always @(posedge clk) unused_capture_cfg_response_error <= cfg_response_error;
wire [159:0] cfg_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [159:0] unused_capture_cfg_read_data;
always @(posedge clk) unused_capture_cfg_read_data <= cfg_read_data;
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
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_awready-1:0] source_m_axi_awready = SOURCE_BITS_m_axi_awready'(60);
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
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_wready-1:0] source_m_axi_wready = SOURCE_BITS_m_axi_wready'(66);
always @(posedge clk) source_m_axi_wready <= {source_m_axi_wready[SOURCE_BITS_m_axi_wready-2:0],source_m_axi_wready[SOURCE_BITS_m_axi_wready-1]^source_m_axi_wready[SOURCE_BITS_m_axi_wready-2]};
wire  m_axi_wready = PORT_BITS_m_axi_wready'(source_m_axi_wready);
localparam integer PORT_BITS_m_axi_bid = 1;
localparam integer SOURCE_BITS_m_axi_bid = PORT_BITS_m_axi_bid < 32 ? 32 : PORT_BITS_m_axi_bid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bid-1:0] source_m_axi_bid = SOURCE_BITS_m_axi_bid'(67);
always @(posedge clk) source_m_axi_bid <= {source_m_axi_bid[SOURCE_BITS_m_axi_bid-2:0],source_m_axi_bid[SOURCE_BITS_m_axi_bid-1]^source_m_axi_bid[SOURCE_BITS_m_axi_bid-2]};
wire  m_axi_bid = PORT_BITS_m_axi_bid'(source_m_axi_bid);
localparam integer PORT_BITS_m_axi_bresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_bresp = PORT_BITS_m_axi_bresp < 32 ? 32 : PORT_BITS_m_axi_bresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bresp-1:0] source_m_axi_bresp = SOURCE_BITS_m_axi_bresp'(68);
always @(posedge clk) source_m_axi_bresp <= {source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2:0],source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-1]^source_m_axi_bresp[SOURCE_BITS_m_axi_bresp-2]};
wire [1:0] m_axi_bresp = PORT_BITS_m_axi_bresp'(source_m_axi_bresp);
localparam integer PORT_BITS_m_axi_bvalid = 1;
localparam integer SOURCE_BITS_m_axi_bvalid = PORT_BITS_m_axi_bvalid < 32 ? 32 : PORT_BITS_m_axi_bvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_bvalid-1:0] source_m_axi_bvalid = SOURCE_BITS_m_axi_bvalid'(69);
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
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_arready-1:0] source_m_axi_arready = SOURCE_BITS_m_axi_arready'(77);
always @(posedge clk) source_m_axi_arready <= {source_m_axi_arready[SOURCE_BITS_m_axi_arready-2:0],source_m_axi_arready[SOURCE_BITS_m_axi_arready-1]^source_m_axi_arready[SOURCE_BITS_m_axi_arready-2]};
wire  m_axi_arready = PORT_BITS_m_axi_arready'(source_m_axi_arready);
localparam integer PORT_BITS_m_axi_rid = 1;
localparam integer SOURCE_BITS_m_axi_rid = PORT_BITS_m_axi_rid < 32 ? 32 : PORT_BITS_m_axi_rid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rid-1:0] source_m_axi_rid = SOURCE_BITS_m_axi_rid'(78);
always @(posedge clk) source_m_axi_rid <= {source_m_axi_rid[SOURCE_BITS_m_axi_rid-2:0],source_m_axi_rid[SOURCE_BITS_m_axi_rid-1]^source_m_axi_rid[SOURCE_BITS_m_axi_rid-2]};
wire  m_axi_rid = PORT_BITS_m_axi_rid'(source_m_axi_rid);
localparam integer PORT_BITS_m_axi_rdata = (AXI_DATA_WIDTH-1)+1;
localparam integer SOURCE_BITS_m_axi_rdata = PORT_BITS_m_axi_rdata < 32 ? 32 : PORT_BITS_m_axi_rdata;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rdata-1:0] source_m_axi_rdata = SOURCE_BITS_m_axi_rdata'(79);
always @(posedge clk) source_m_axi_rdata <= {source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2:0],source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-1]^source_m_axi_rdata[SOURCE_BITS_m_axi_rdata-2]};
wire [AXI_DATA_WIDTH-1:0] m_axi_rdata = PORT_BITS_m_axi_rdata'(source_m_axi_rdata);
localparam integer PORT_BITS_m_axi_rresp = (1)+1;
localparam integer SOURCE_BITS_m_axi_rresp = PORT_BITS_m_axi_rresp < 32 ? 32 : PORT_BITS_m_axi_rresp;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rresp-1:0] source_m_axi_rresp = SOURCE_BITS_m_axi_rresp'(80);
always @(posedge clk) source_m_axi_rresp <= {source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2:0],source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-1]^source_m_axi_rresp[SOURCE_BITS_m_axi_rresp-2]};
wire [1:0] m_axi_rresp = PORT_BITS_m_axi_rresp'(source_m_axi_rresp);
localparam integer PORT_BITS_m_axi_rlast = 1;
localparam integer SOURCE_BITS_m_axi_rlast = PORT_BITS_m_axi_rlast < 32 ? 32 : PORT_BITS_m_axi_rlast;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rlast-1:0] source_m_axi_rlast = SOURCE_BITS_m_axi_rlast'(81);
always @(posedge clk) source_m_axi_rlast <= {source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2:0],source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-1]^source_m_axi_rlast[SOURCE_BITS_m_axi_rlast-2]};
wire  m_axi_rlast = PORT_BITS_m_axi_rlast'(source_m_axi_rlast);
localparam integer PORT_BITS_m_axi_rvalid = 1;
localparam integer SOURCE_BITS_m_axi_rvalid = PORT_BITS_m_axi_rvalid < 32 ? 32 : PORT_BITS_m_axi_rvalid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_m_axi_rvalid-1:0] source_m_axi_rvalid = SOURCE_BITS_m_axi_rvalid'(82);
always @(posedge clk) source_m_axi_rvalid <= {source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2:0],source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-1]^source_m_axi_rvalid[SOURCE_BITS_m_axi_rvalid-2]};
wire  m_axi_rvalid = PORT_BITS_m_axi_rvalid'(source_m_axi_rvalid);
wire  m_axi_rready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_m_axi_rready;
always @(posedge clk) unused_capture_m_axi_rready <= m_axi_rready;
(* KEEP_HIERARCHY = "yes" *) media_store_axi #(
.REUSE_VIEWS(REUSE_VIEWS),
.VIEWS(VIEWS),
.SLOTS(SLOTS),
.CHUNK_FRAMES(CHUNK_FRAMES),
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
.VIEW_BITS(VIEW_BITS),
.SLOT_BITS(SLOT_BITS),
.TABLE_BITS(TABLE_BITS),
.AXI_DATA_WIDTH(AXI_DATA_WIDTH),
.PHYSICAL_BASE(PHYSICAL_BASE)
) core_under_test (
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
.arena_base(arena_base),
.hint_valid(hint_valid),
.hint_ready(hint_ready),
.hint_view(hint_view),
.hint_position(hint_position),
.hint_length(hint_length),
.hint_count(hint_count),
.cmd_valid(cmd_valid),
.cmd_ready(cmd_ready),
.cmd_opcode(cmd_opcode),
.cmd_view(cmd_view),
.cmd_start(cmd_start),
.cmd_length(cmd_length),
.cmd_count(cmd_count),
.busy(busy),
.done(done),
.error_code(error_code),
.memory_fault(memory_fault),
.result_view(result_view),
.result_empty(result_empty),
.write_valid(write_valid),
.write_ready(write_ready),
.write_data(write_data),
.read_valid(read_valid),
.read_ready(read_ready),
.read_data(read_data),
.cfg_valid(cfg_valid),
.cfg_ready(cfg_ready),
.cfg_write(cfg_write),
.cfg_space(cfg_space),
.cfg_address(cfg_address),
.cfg_write_data(cfg_write_data),
.cfg_response_valid(cfg_response_valid),
.cfg_response_error(cfg_response_error),
.cfg_read_data(cfg_read_data),
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
