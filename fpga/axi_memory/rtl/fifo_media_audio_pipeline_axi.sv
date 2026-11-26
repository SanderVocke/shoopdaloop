// Generated integration wrapper; do not edit by hand.
`include "state_processor_defs.svh"

module fifo_media_audio_pipeline_axi #(
    parameter integer EXTERNAL_IO = 0,
    parameter integer ENABLE_MIDI = 0,
    parameter integer SPLIT_MIDI_JOBS = 0,
    parameter integer MIDI_JOB_FIFO_DEPTH = 128,
    parameter integer DDR_PROGRAM = 0,
    parameter integer PROGRAM_DEPTH = 1024, parameter integer STATE_DEPTH = 512,
    parameter integer JOB_DEPTH = 1024, parameter integer READ_HINT_DEPTH = JOB_DEPTH, parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 512, parameter integer INPUT_PORTS = 1, parameter integer OUTPUT_PORTS = 2,
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
    parameter integer LOOP_DEPTH = 128,
    parameter integer AXI_DATA_WIDTH = 32,
    parameter [31:0] PHYSICAL_BASE = 32'h01000000,
    parameter [63:0] PHYSICAL_BYTES_LIMIT = 64'h100000000
) (
    output wire midi_job_valid,
    input wire midi_job_ready,
    output wire [255:0] midi_job_data,
    input wire midi_job_done,
    input wire [3:0] midi_job_error,
    output wire [3:0] midi_error_code,
    output wire sample_input_read,
    output wire [31:0] sample_input_address,
    input wire [31:0] sample_input_data,
    output wire sample_output_write,
    output wire [31:0] sample_output_address,
    output wire [31:0] sample_output_data,
    input wire clk,
    input wire reset,
    input wire maintenance_enable,
    input wire maintenance_tick,
    output wire [31:0] reclaim_pending,
    output wire [31:0] reclaim_high_water,
    output wire [63:0] reclaim_references,
    output wire reclaim_active,
    output wire reclaim_released,
    output wire [31:0] reclaim_block,
    output wire [63:0] reclaim_blocks,
    output wire [63:0] reclaim_freed,
    output wire [63:0] reclaim_cycles,
    output wire reclaim_quiescent,
    output wire [31:0] reclaim_pages_used,
    output wire [31:0] reclaim_pages_available,
    output wire [63:0] reclaim_pages_freed,
    output wire [63:0] reclaim_capacity_failures,
    output wire [63:0] active_revision,
    output wire [63:0] sync_frame,
    output wire sync_event_valid,
    output wire [255:0] sync_event_data,
    input wire run_valid,
    output wire run_ready,
    input wire [31:0] run_quantum,
    input wire [31:0] run_chunk_frames,
    input wire [31:0] run_inputs,
    input wire [31:0] run_outputs,
    input wire job_enable,
    output wire busy,
    output wire done,
    output wire [7:0] error_code,
    output wire [1:0] error_stage,
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
    input wire [63:0] arena_base,
    input wire store_cmd_valid,
    output wire store_cmd_ready,
    input wire [3:0] store_cmd_opcode,
    input wire [63:0] store_cmd_view,
    input wire [63:0] store_cmd_start,
    input wire [63:0] store_cmd_length,
    input wire [31:0] store_cmd_count,
    output wire store_busy,
    output wire store_done,
    output wire [7:0] store_error_code,
    output wire [31:0] store_result_view,
    output wire store_result_empty,
    input wire store_write_valid,
    output wire store_write_ready,
    input wire [31:0] store_write_data,
    output wire store_read_valid,
    input wire store_read_ready,
    output wire [31:0] store_read_data,
    input wire store_cfg_valid,
    output wire store_cfg_ready,
    input wire store_cfg_write,
    input wire [2:0] store_cfg_space,
    input wire [31:0] store_cfg_address,
    input wire [159:0] store_cfg_write_data,
    output wire store_cfg_response_valid,
    output wire store_cfg_response_error,
    output wire [159:0] store_cfg_read_data,
    output wire memory_fault,
    output wire capture_valid,
    output wire [63:0] capture_frame,
    input wire observation_read_valid,
    output wire observation_read_ready,
    input wire [15:0] observation_read_slot,
    output wire observation_response_valid,
    output wire observation_response_error,
    output wire [128:0] observation_read_data,
    output wire trace_valid,
    input wire trace_ready,
    output wire [255:0] trace_data,
    output wire [63:0] trace_lost,
    output wire [31:0] trace_epoch,
    output wire [63:0] trace_revision,
    input wire [31:0] epoch,
    input wire pipeline_fault,
    input wire host_valid,
    output wire host_ready,
    input wire [2:0] host_operation,
    input wire [1:0] host_bank,
    input wire [31:0] host_epoch,
    input wire [31:0] host_word,
    input wire [3:0] host_strobe,
    input wire [63:0] host_sequence,
    output wire host_response_valid,
    output wire [3:0] host_receipt,
    input wire [1:0] inspect_bank,
    output wire [2:0] bank_state,
    output wire [8:0] written_words,
    output wire [3:0] free_mask,
    output wire [3:0] result_ready_mask,
    output wire [63:0] next_sequence,
    output wire [63:0] last_admitted,
    output wire sequence_exhausted,
    input wire result_read_valid,
    input wire [1:0] result_read_bank,
    input wire [5:0] result_read_word,
    output wire result_response_valid,
    output wire result_response_error,
    output wire [31:0] result_read_data,
    output wire active_region,
    output wire active_composites,
    output wire [15:0] active_instructions,
    output wire [15:0] active_sync_count,
    output wire [15:0] active_states,
    output wire [15:0] active_buffers,
    output wire handoff_pending,
    output wire [31:0] pending_epoch,
    output wire [63:0] pending_sequence,
    output wire gain_update_valid,
    output wire [15:0] gain_update_slot,
    output wire [31:0] gain_update_bits,
    output wire faulted,
    output wire command_phase,
    output wire [63:0] cutoff_sequence,
    output wire [63:0] groups_attempted,
    output wire [31:0] peak_command_cycles,
    output wire [31:0] current_command_cycles,
    input wire store_response_ready,
    output wire [31:0] store_result_count,
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
    wire memory_valid;
    wire memory_ready;
    wire memory_write;
    wire [63:0] memory_address;
    wire [31:0] memory_write_data;
    wire [63:0] memory_arena_base;
    wire [31:0] memory_count;
    wire memory_write_valid;
    wire memory_write_ready;
    wire memory_read_valid;
    wire memory_read_ready;
    wire memory_response_valid;
    wire memory_response_ready;
    wire memory_response_error;
    wire [31:0] memory_read_data;
    wire memory_meta_valid;
    wire memory_meta_ready;
    wire memory_meta_write;
    wire [63:0] memory_meta_address;
    wire [31:0] memory_meta_write_data;
    wire [31:0] memory_meta_count;
    wire memory_meta_write_valid;
    wire memory_meta_write_ready;
    wire memory_meta_read_valid;
    wire memory_meta_read_ready;
    wire memory_meta_response_valid;
    wire memory_meta_response_ready;
    wire memory_meta_response_error;
    wire [31:0] memory_meta_read_data;
    wire memory_program_valid;
    wire memory_program_ready;
    wire memory_program_write;
    wire [15:0] memory_program_address;
    wire [31:0] memory_program_count;
    wire [31:0] memory_program_write_data;
    wire memory_program_write_valid;
    wire memory_program_write_ready;
    wire memory_program_read_valid;
    wire memory_program_read_ready;
    wire [31:0] memory_program_read_data;
    wire memory_program_response_valid;
    wire memory_program_response_ready;
    wire memory_program_response_error;
    wire [63:0] arena_bytes = 64'(SLOTS)*64'(CHUNK_FRAMES)*4;
    wire [31:0] physical_base = PHYSICAL_BASE;
    localparam integer PROGRAM_REGIONS = 2;
    wire media_memory_meta_valid;
    wire media_memory_meta_ready;
    wire media_memory_meta_write;
    wire [63:0] media_memory_meta_address;
    wire [31:0] media_memory_meta_write_data;
    wire [31:0] media_memory_meta_count;
    wire media_memory_meta_write_valid;
    wire media_memory_meta_write_ready;
    wire media_memory_meta_read_valid;
    wire media_memory_meta_read_ready;
    wire media_memory_meta_response_valid;
    wire media_memory_meta_response_ready;
    wire media_memory_meta_response_error;
    wire [31:0] media_memory_meta_read_data;
    fifo_media_audio_pipeline #(
        .EXTERNAL_IO(EXTERNAL_IO),
        .ENABLE_MIDI(ENABLE_MIDI),
        .SPLIT_MIDI_JOBS(SPLIT_MIDI_JOBS),
        .MIDI_JOB_FIFO_DEPTH(MIDI_JOB_FIFO_DEPTH),
        .DDR_PROGRAM(DDR_PROGRAM),
        .PROGRAM_DEPTH(PROGRAM_DEPTH),
        .STATE_DEPTH(STATE_DEPTH),
        .JOB_DEPTH(JOB_DEPTH),
        .READ_HINT_DEPTH(READ_HINT_DEPTH),
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
        .LOOP_DEPTH(LOOP_DEPTH)) core (.*, .memory_meta_valid(media_memory_meta_valid), .memory_meta_ready(media_memory_meta_ready), .memory_meta_write(media_memory_meta_write), .memory_meta_address(media_memory_meta_address), .memory_meta_write_data(media_memory_meta_write_data), .memory_meta_count(media_memory_meta_count), .memory_meta_write_valid(media_memory_meta_write_valid), .memory_meta_write_ready(media_memory_meta_write_ready), .memory_meta_read_valid(media_memory_meta_read_valid), .memory_meta_read_ready(media_memory_meta_read_ready), .memory_meta_response_valid(media_memory_meta_response_valid), .memory_meta_response_ready(media_memory_meta_response_ready), .memory_meta_response_error(media_memory_meta_response_error), .memory_meta_read_data(media_memory_meta_read_data));
    localparam [63:0] STORAGE_WORDS = ((64'(SLOTS)*64'(CHUNK_FRAMES)+15)/16)*16 + 2*((64'(SLOTS/BLOCK_CHUNKS)+15)/16)*16 + 64'(TABLE_PAGES)*16 + 64'(RETIRED_TABLES)*16;
    localparam [63:0] PROGRAM_OFFSET = ((STORAGE_WORDS*4+63)/64)*64;
    localparam [63:0] TOTAL_BYTES = DDR_PROGRAM != 0
        ? PROGRAM_OFFSET + 64'(PROGRAM_REGIONS)*64'(PROGRAM_DEPTH)*16
        : STORAGE_WORDS*4;
    initial if (TOTAL_BYTES > PHYSICAL_BYTES_LIMIT) $fatal(1,"DDR arena exceeds reserved memory");
    initial if (DDR_PROGRAM != 0 && BURST_MEMORY == 0) $fatal(1,"DDR program requires burst memory");
    generate if (DDR_PROGRAM != 0) begin : ddr_program
        program_axi_mux program_bus (
            .clk(clk), .reset(reset),
            .media_valid(media_memory_meta_valid), .program_valid(memory_program_valid), .memory_valid(memory_meta_valid),
            .media_ready(media_memory_meta_ready), .program_ready(memory_program_ready), .memory_ready(memory_meta_ready),
            .media_write(media_memory_meta_write), .program_write(memory_program_write), .memory_write(memory_meta_write),
            .media_address(media_memory_meta_address), .program_address(memory_arena_base + PROGRAM_OFFSET + {44'b0,memory_program_address,4'b0}), .memory_address(memory_meta_address),
            .media_write_data(media_memory_meta_write_data), .program_write_data(memory_program_write_data), .memory_write_data(memory_meta_write_data),
            .media_count(media_memory_meta_count), .program_count(memory_program_count), .memory_count(memory_meta_count),
            .media_write_valid(media_memory_meta_write_valid), .program_write_valid(memory_program_write_valid), .memory_write_valid(memory_meta_write_valid),
            .media_write_ready(media_memory_meta_write_ready), .program_write_ready(memory_program_write_ready), .memory_write_ready(memory_meta_write_ready),
            .media_read_valid(media_memory_meta_read_valid), .program_read_valid(memory_program_read_valid), .memory_read_valid(memory_meta_read_valid),
            .media_read_ready(media_memory_meta_read_ready), .program_read_ready(memory_program_read_ready), .memory_read_ready(memory_meta_read_ready),
            .media_response_valid(media_memory_meta_response_valid), .program_response_valid(memory_program_response_valid), .memory_response_valid(memory_meta_response_valid),
            .media_response_ready(media_memory_meta_response_ready), .program_response_ready(memory_program_response_ready), .memory_response_ready(memory_meta_response_ready),
            .media_response_error(media_memory_meta_response_error), .program_response_error(memory_program_response_error), .memory_response_error(memory_meta_response_error),
            .media_read_data(media_memory_meta_read_data), .program_read_data(memory_program_read_data), .memory_read_data(memory_meta_read_data)
        );
    end else begin : local_program
        assign memory_meta_valid = media_memory_meta_valid;
        assign media_memory_meta_ready = memory_meta_ready;
        assign memory_meta_write = media_memory_meta_write;
        assign memory_meta_address = media_memory_meta_address;
        assign memory_meta_write_data = media_memory_meta_write_data;
        assign memory_meta_count = media_memory_meta_count;
        assign memory_meta_write_valid = media_memory_meta_write_valid;
        assign media_memory_meta_write_ready = memory_meta_write_ready;
        assign media_memory_meta_read_valid = memory_meta_read_valid;
        assign memory_meta_read_ready = media_memory_meta_read_ready;
        assign media_memory_meta_response_valid = memory_meta_response_valid;
        assign memory_meta_response_ready = media_memory_meta_response_ready;
        assign media_memory_meta_response_error = memory_meta_response_error;
        assign media_memory_meta_read_data = memory_meta_read_data;
        assign memory_program_ready = 0;
        assign memory_program_write_ready = 0;
        assign memory_program_read_valid = 0;
        assign memory_program_read_data = 0;
        assign memory_program_response_valid = 0;
        assign memory_program_response_error = 0;
        wire unused_program = &{1'b0,memory_program_valid,memory_program_write,memory_program_address,memory_program_count,memory_program_write_data,memory_program_write_valid,memory_program_read_ready,memory_program_response_ready};
    end endgenerate
    initial if ({32'b0,PHYSICAL_BASE}+TOTAL_BYTES > 64'h100000000) $fatal(1,"DDR arena exceeds AXI address range");
    shared_axi #(.DATA_WIDTH(AXI_DATA_WIDTH),.BURST_MEMORY(BURST_MEMORY),.MAX_SAMPLE_WORDS(32'(CHUNK_FRAMES)),.MAX_METADATA_WORDS(32'd16)) memory_endpoint (.arena_base(memory_arena_base),.arena_bytes(TOTAL_BYTES),.sample_bytes(arena_bytes),.*);
endmodule
