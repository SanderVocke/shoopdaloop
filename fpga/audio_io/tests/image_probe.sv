`include "state_processor_defs.svh"
module image_probe #(
    parameter integer PROGRAM_DEPTH=16, STATE_DEPTH=8, JOB_DEPTH=16, INITIAL_STATES=2, BANKS=1, CANONICAL=0
) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready,
    input wire [7:0] request_opcode, input wire request_state,
    input wire [15:0] request_address, input wire [639:0] request_data,
    output wire response_valid, output wire [3:0] response_error, output wire [128:0] response_data,
    output wire fault, output wire image_busy,
    input wire run_valid, output wire run_ready, output wire busy, output wire done, output wire [7:0] error_code,
    input wire job_ready, output wire job_valid, output wire [255:0] job_data
);
    wire cfg_valid, cfg_ready, cfg_state, cfg_write, cfg_response_valid, cfg_response_error;
    wire [15:0] cfg_address;
    wire [128:0] cfg_write_data, cfg_read_data;
    wire override_counts;
    wire [31:0] instructions, states, jobs, buffers;
    audio_image #(.PROGRAM_DEPTH(PROGRAM_DEPTH),.STATE_DEPTH(STATE_DEPTH),.JOB_DEPTH(JOB_DEPTH),.BUFFERS(8),.BANKS(BANKS),.CANONICAL(CANONICAL)) image (
        .clk(clk),.reset(reset),.execution_idle(!busy && !run_valid),.current_state_count(32'(INITIAL_STATES)),
        .request_valid(request_valid),.request_ready(request_ready),.request_opcode(request_opcode),
        .request_state(request_state),.request_address(request_address),.request_data(request_data),
        .busy(image_busy),.response_valid(response_valid),.response_error(response_error),.response_data(response_data),.fault(fault),
        .active_override(override_counts),.active_instructions(instructions),.active_states(states),.active_jobs(jobs),.active_buffers(buffers),
        .cfg_valid(cfg_valid),.cfg_ready(cfg_ready),.cfg_state(cfg_state),.cfg_write(cfg_write),.cfg_address(cfg_address),
        .cfg_write_data(cfg_write_data),.cfg_response_valid(cfg_response_valid),.cfg_response_error(cfg_response_error),.cfg_read_data(cfg_read_data)
    );
    wire processor_ready, read_intent_valid, loop_read_valid;
    wire [31:0] pc, queued, loop_slot;
    wire [255:0] read_intent_data;
    wire unused = &{1'b0,buffers,pc,queued,read_intent_valid,read_intent_data,loop_read_valid,loop_slot};
    assign run_ready = processor_ready && !image_busy;
    state_processor #(.PROGRAM_DEPTH(PROGRAM_DEPTH),.STATE_DEPTH(STATE_DEPTH),.JOB_DEPTH(JOB_DEPTH)) processor (
        .clk(clk),.reset(reset),.run_valid(run_valid && !image_busy),.run_ready(processor_ready),
        .run_instruction_count(override_counts ? instructions : 32'd1),.run_state_count(override_counts ? states : 32'(INITIAL_STATES)),
        .run_quantum(32'd32),.run_chunk_frames(32'd1),.run_job_budget(override_counts ? jobs : 32'd1),
        .busy(busy),.done(done),.error_code(error_code),.instruction_pc(pc),
        .cfg_valid(cfg_valid),.cfg_ready(cfg_ready),.cfg_state(cfg_state),.cfg_write(cfg_write),.cfg_address(cfg_address),
        .cfg_write_data(cfg_write_data),.cfg_response_valid(cfg_response_valid),.cfg_response_error(cfg_response_error),.cfg_read_data(cfg_read_data),
        .job_valid(job_valid),.job_ready(job_ready),.job_data(job_data),.queued_jobs(queued),
        .read_intent_valid(read_intent_valid),.read_intent_data(read_intent_data),
        .loop_read_valid(loop_read_valid),.loop_read_ready(1'b1),.loop_read_slot(loop_slot),
        .loop_response_valid(1'b0),.loop_response_error(1'b0),.loop_response_data(164'b0)
    );
endmodule
