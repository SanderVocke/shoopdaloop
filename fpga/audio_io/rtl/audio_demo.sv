// Generated prepared audio demonstration; do not edit by hand.
`include "state_processor_defs.svh"
module audio_demo #(
    parameter integer Q = 32, P = Q, CHANNELS = 16, RX_BANKS = 3, TX_BANKS = 3,
    parameter integer AXI_DATA_WIDTH = 32, CHUNK_FRAMES = Q,
    parameter integer BOOT_WORDS = 166, EVENT_WORDS = 74,
    parameter integer INSTRUCTIONS = 64, STATES = 32, BUFFERS = 32, JOB_BUDGET = 80,
    parameter integer RESET_CYCLES = 64, WATCHDOG_CYCLES = 1000000,
    parameter integer VIEWS = 32, SLOTS = 1024, BLOCK_CHUNKS = 32,
    parameter integer RETIRED_TABLES = 256, RECLAIM_BLOCKS = 0, RECLAIM_CYCLES = 5000, RECLAIM_GUARD = 1024,
    parameter integer TABLE_LINES = 8, REF_LINES = 4, STACK_LINES = 2,
    parameter integer TABLE_PAGES = (SLOTS/BLOCK_CHUNKS+14)/15+8*VIEWS+16,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000,
    parameter BOOT_FILE = "fpga/audio_io/examples/default/boot.hex",
    parameter EVENT_FILE = "fpga/audio_io/examples/default/events.hex"
) (
    input wire clk, input wire audio_clk, input wire reset,
    input wire frame_tick, input wire frame_prepare, input wire job_enable,
    input wire restart, output wire boot_done, output wire boot_fault,
    output reg [31:0] epoch,
    output reg [31:0] recoveries,
    output reg [3:0] last_recovery_reason,
    output reg [63:0] last_failure_sequence, output reg [63:0] last_failure_logical_frame,
    output reg [7:0] last_error_code, output reg [1:0] last_error_stage,
    output wire faulted,
    output wire done,
    output wire [7:0] error_code,
    output wire [1:0] error_stage,
    output wire monitor_valid,
    output wire monitor_ready,
    output wire [`SHOOP_SP_JOB_BITS-1:0] monitor_data,
    output wire memory_quiescent,
    output wire [CHANNELS*32-1:0] tx_samples,
    output wire tx_valid,
    output wire [63:0] audio_frame,
    output wire [63:0] session_frame,
    output wire recovery_requested,
    output wire [3:0] recovery_reason,
    output wire [63:0] failure_sequence,
    output wire [63:0] process_sequence,
    output wire [31:0] processed_quanta,
    output wire [31:0] played_quanta,
    output wire [31:0] missed_deadlines,
    output wire [31:0] discarded_quanta,
    output wire [31:0] input_losses,
    output wire [31:0] rx_high_water,
    output wire [31:0] tx_high_water,
    output wire m_axi_awid,
    output wire [31:0] m_axi_awaddr,
    output wire [3:0] m_axi_awlen,
    output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst,
    output wire m_axi_awvalid,
    input wire m_axi_awready,
    output wire [AXI_DATA_WIDTH-1:0] m_axi_wdata,
    output wire [AXI_DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wid,
    output wire m_axi_wlast,
    output wire m_axi_wvalid,
    input wire m_axi_wready,
    input wire m_axi_bid,
    input wire [1:0] m_axi_bresp,
    input wire m_axi_bvalid,
    output wire m_axi_bready,
    output wire m_axi_arid,
    output wire [31:0] m_axi_araddr,
    output wire [3:0] m_axi_arlen,
    output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst,
    output wire m_axi_arvalid,
    input wire m_axi_arready,
    input wire m_axi_rid,
    input wire [AXI_DATA_WIDTH-1:0] m_axi_rdata,
    input wire [1:0] m_axi_rresp,
    input wire m_axi_rlast,
    input wire m_axi_rvalid,
    output wire m_axi_rready
);
    wire [31:0] reclaim_pending;
    wire [31:0] reclaim_high_water;
    wire [63:0] reclaim_references;
    wire reclaim_active;
    wire reclaim_released;
    wire [31:0] reclaim_block;
    wire [63:0] reclaim_blocks;
    wire [63:0] reclaim_freed;
    wire [63:0] reclaim_cycles;
    wire reclaim_quiescent;
    wire [31:0] reclaim_pages_used;
    wire [31:0] reclaim_pages_available;
    wire [63:0] reclaim_pages_freed;
    wire [63:0] reclaim_capacity_failures;
    wire run_ready;
    wire [31:0] run_instruction_count;
    wire [31:0] run_state_count;
    wire [31:0] run_quantum;
    wire [31:0] run_chunk_frames;
    wire [31:0] run_job_budget;
    wire [31:0] run_buffers;
    wire [31:0] run_inputs;
    wire [31:0] run_outputs;
    wire busy;
    wire state_busy;
    wire state_done;
    wire jobs_busy;
    wire jobs_done;
    wire [BUFFERS-1:0] buffer_ready;
    wire cfg_valid;
    wire cfg_ready;
    wire cfg_state;
    wire cfg_write;
    wire [15:0] cfg_address;
    wire [`SHOOP_SP_STATE_BITS-1:0] cfg_write_data;
    wire cfg_response_valid;
    wire cfg_response_error;
    wire [`SHOOP_SP_STATE_BITS-1:0] cfg_read_data;
    wire audio_cfg_valid;
    wire audio_cfg_ready;
    wire [1:0] audio_cfg_space;
    wire audio_cfg_write;
    wire [31:0] audio_cfg_address;
    wire [31:0] audio_cfg_write_data;
    wire audio_cfg_response_valid;
    wire audio_cfg_response_error;
    wire [31:0] audio_cfg_read_data;
    wire [31:0] queued_jobs;
    wire drain_valid;
    wire drain_ready;
    wire command_valid;
    wire command_ready;
    wire [7:0] command_opcode;
    wire [31:0] command_index;
    wire [639:0] command_data;
    wire response_valid;
    wire response_ready;
    wire [3:0] response_error;
    wire [831:0] response_data;
    wire transition_busy;
    wire [63:0] arena_base;
    wire store_cmd_valid;
    wire store_cmd_ready;
    wire [3:0] store_cmd_opcode;
    wire [63:0] store_cmd_view;
    wire [63:0] store_cmd_start;
    wire [63:0] store_cmd_length;
    wire [31:0] store_cmd_count;
    wire store_busy;
    wire store_done;
    wire [7:0] store_error_code;
    wire [31:0] store_result_view;
    wire store_result_empty;
    wire store_write_valid;
    wire store_write_ready;
    wire [31:0] store_write_data;
    wire store_read_valid;
    wire store_read_ready;
    wire [31:0] store_read_data;
    wire store_cfg_valid;
    wire store_cfg_ready;
    wire store_cfg_write;
    wire [2:0] store_cfg_space;
    wire [31:0] store_cfg_address;
    wire [159:0] store_cfg_write_data;
    wire store_cfg_response_valid;
    wire store_cfg_response_error;
    wire [159:0] store_cfg_read_data;
    wire memory_fault;
    wire session_reset;
    wire enable;
    wire quiesce;
    wire session_reset_ready;
    wire [CHANNELS*32-1:0] rx_samples;
    wire control_valid;
    wire control_ready;
    wire [745:0] control_data;
    audio_loop_pipeline #(.MAX_FRAMES(Q),.CHUNK_FRAMES(CHUNK_FRAMES),.P(P),
        .BUFFERS(BUFFERS),.INPUT_PORTS(CHANNELS),.OUTPUT_PORTS(CHANNELS),.RX_BANKS(RX_BANKS),.TX_BANKS(TX_BANKS),
        .VIEWS(VIEWS),.SLOTS(SLOTS),.BLOCK_CHUNKS(BLOCK_CHUNKS),.TABLE_PAGES(TABLE_PAGES),.RETIRED_TABLES(RETIRED_TABLES),
        .RECLAIM_BLOCKS(RECLAIM_BLOCKS),.RECLAIM_CYCLES(RECLAIM_CYCLES),.RECLAIM_GUARD(RECLAIM_GUARD),
        .TABLE_LINES(TABLE_LINES),.REF_LINES(REF_LINES),.STACK_LINES(STACK_LINES),.PHYSICAL_BASE(PHYSICAL_BASE),
        .REQUEST_DEPTH(128),.EVENT_DEPTH(256),.AXI_DATA_WIDTH(AXI_DATA_WIDTH),.WATCHDOG_CYCLES(WATCHDOG_CYCLES)) engine (.*);
    localparam [2:0] LOADING=0, RUNNING=1, DRAINING=2, RESETTING=3, FAILED=4;
    reg [2:0] phase;
    reg [31:0] reset_count;
    assign enable = phase == RUNNING;
    assign quiesce = phase == DRAINING || phase == RESETTING;
    assign session_reset = phase == RESETTING;
    assign run_instruction_count = INSTRUCTIONS;
    assign run_state_count = STATES;
    assign run_quantum = Q;
    assign run_chunk_frames = CHUNK_FRAMES;
    assign run_job_budget = JOB_BUDGET;
    assign run_buffers = BUFFERS;
    assign run_inputs = CHANNELS;
    assign run_outputs = CHANNELS;
    assign drain_ready = 0;
    assign arena_base = 0;
    assign audio_cfg_valid = 0;
    assign audio_cfg_space = 0;
    assign audio_cfg_write = 0;
    assign audio_cfg_address = 0;
    assign audio_cfg_write_data = 0;
    assign store_cmd_valid = 0;
    assign store_cmd_opcode = 0;
    assign store_cmd_view = 0;
    assign store_cmd_start = 0;
    assign store_cmd_length = 0;
    assign store_cmd_count = 0;
    assign store_write_valid = 0;
    assign store_write_data = 0;
    assign store_read_ready = 0;
    wire reload_reset = reset || (session_reset && session_reset_ready);
    audio_boot #(.WORDS(BOOT_WORDS),.FILE_NAME(BOOT_FILE)) loader (
        .clk(clk),.reset(reload_reset),.done(boot_done),.fault(boot_fault),
        .command_valid(command_valid),.command_ready(command_ready),.command_opcode(command_opcode),
        .command_index(command_index),.command_data(command_data),.response_valid(response_valid),
        .response_ready(response_ready),.response_error(response_error),
        .cfg_valid(cfg_valid),.cfg_ready(cfg_ready),.cfg_state(cfg_state),.cfg_write(cfg_write),
        .cfg_address(cfg_address),.cfg_write_data(cfg_write_data),.cfg_response_valid(cfg_response_valid),.cfg_response_error(cfg_response_error),
        .store_cfg_valid(store_cfg_valid),.store_cfg_ready(store_cfg_ready),.store_cfg_write(store_cfg_write),
        .store_cfg_space(store_cfg_space),.store_cfg_address(store_cfg_address),.store_cfg_write_data(store_cfg_write_data),
        .store_cfg_response_valid(store_cfg_response_valid),.store_cfg_response_error(store_cfg_response_error)
    );
    audio_events #(.WORDS(EVENT_WORDS),.FILE_NAME(EVENT_FILE)) events (
        .clk(clk),.reset(reload_reset),.enable(boot_done && !boot_fault),
        .valid(control_valid),.ready(control_ready),.data(control_data)
    );
    for (genvar channel=0;channel<CHANNELS;channel=channel+1) begin : generator
        assign rx_samples[channel*32 +: 32] = 32'((channel+1)*100000)+32'(session_frame)*37+1;
    end
    always @(posedge clk) begin
        if (reset) begin
            phase<=LOADING; epoch<=1; reset_count<=0; recoveries<=0;
            last_recovery_reason<=0; last_failure_sequence<=0; last_failure_logical_frame<=0;
            last_error_code<=0; last_error_stage<=0;
        end
        else case (phase)
            LOADING: if (boot_fault) phase<=FAILED; else if (boot_done) phase<=RUNNING;
            RUNNING: if (recovery_requested || restart) begin
                phase<=DRAINING;
                recoveries<=recoveries == 32'hffffffff ? recoveries : recoveries+1;
                last_recovery_reason<=recovery_requested ? recovery_reason : 0;
                last_failure_sequence<=failure_sequence; last_failure_logical_frame<=failure_sequence*Q;
                last_error_code<=error_code; last_error_stage<=error_stage;
            end
            DRAINING: if (memory_quiescent) begin phase<=RESETTING; reset_count<=0; end
            // Count reset cycles only while the quiescent memory boundary accepts reset.
            RESETTING: if (!session_reset_ready) reset_count<=0;
                else if (reset_count == RESET_CYCLES-1) begin phase<=LOADING; epoch<=epoch+1; end
                else reset_count<=reset_count+1;
            default: begin end
        endcase
    end
    wire unused_status = &{1'b0,reclaim_pending,reclaim_high_water,reclaim_references,reclaim_active,reclaim_released,reclaim_block,reclaim_blocks,reclaim_freed,reclaim_cycles,reclaim_quiescent,reclaim_pages_used,reclaim_pages_available,reclaim_pages_freed,reclaim_capacity_failures,run_ready,busy,state_busy,state_done,jobs_busy,jobs_done,buffer_ready,cfg_ready,cfg_response_valid,cfg_response_error,cfg_read_data,audio_cfg_ready,audio_cfg_response_valid,audio_cfg_response_error,audio_cfg_read_data,queued_jobs,drain_valid,command_ready,response_valid,response_error,response_data,transition_busy,store_cmd_ready,store_busy,store_done,store_error_code,store_result_view,store_result_empty,store_write_ready,store_read_valid,store_read_data,store_cfg_ready,store_cfg_response_valid,store_cfg_response_error,store_cfg_read_data,memory_fault,session_reset_ready,control_ready};
endmodule
