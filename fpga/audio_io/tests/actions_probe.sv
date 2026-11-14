module actions_probe #(
    parameter integer SLOTS=4, MAX_WORK=256
) (
    input wire clk, input wire reset, input wire [63:0] current_frame,
    input wire cut_start, output wire cut_done, output wire action_busy, output wire action_fault,
    input wire request_valid, output wire request_ready, input wire [7:0] request_opcode,
    input wire request_state, input wire [15:0] request_address, input wire [639:0] request_data,
    output wire response_valid, output wire [3:0] response_error, output wire [831:0] response_data,
    input wire run_valid, output wire run_ready, output wire busy, output wire done, output wire [7:0] error_code,
    input wire job_ready, output wire job_valid, output wire [255:0] job_data
);
    wire cfg_valid, cfg_ready, cfg_state, cfg_response_valid, image_busy, image_fault;
    wire [7:0] cfg_opcode;
    wire [15:0] cfg_address;
    wire [639:0] cfg_data;
    wire [3:0] cfg_response_error;
    wire [128:0] cfg_response_data;
    wire unused=&{1'b0,image_busy};
    wire resident_fault;
    assign action_fault=resident_fault || image_fault;
    wire unused_action_resident_valid;
    wire [7:0] unused_action_resident_opcode;
    wire [511:0] unused_action_resident_data;
    wire [63:0] unused_action_resident_sequence;
    wire [63:0] unused_action_resident_frame;
    wire [47:0] unused_action_resident_reference;
    wire [15:0] unused_action_resident_slot;
    wire [31:0] unused_action_resident_generation;
    wire [1:0] unused_action_resident_mode;
    wire unused_action_lookup_ready;
    wire unused_action_lookup_done;
    wire [3:0] unused_action_lookup_error;
    wire [767:0] unused_action_lookup_data;
    wire unused_action_update_ready;
    wire unused_action_update_done;
    wire [3:0] unused_action_update_error;
    wire unused_action_private=&{1'b0,unused_action_resident_valid,unused_action_resident_opcode,unused_action_resident_data,unused_action_resident_sequence,unused_action_resident_frame,unused_action_resident_reference,unused_action_resident_slot,unused_action_resident_generation,unused_action_resident_mode,unused_action_lookup_ready,unused_action_lookup_done,unused_action_lookup_error,unused_action_lookup_data,unused_action_update_ready,unused_action_update_done,unused_action_update_error};
    wire unused_record_request_valid;
    wire unused_record_request_ack;
    wire [15:0] unused_record_request_slot;
    wire [31:0] unused_record_request_generation;
    wire [31:0] unused_record_request_epoch;
    wire [4:0] unused_record_request_word;
    audio_actions #(.SLOTS(SLOTS),.MAX_WORK(MAX_WORK),.ACTIVATION_WORK(176),.WATCHDOG(16384)) actions (
        .record_request_valid(unused_record_request_valid),
        .record_request_ack(unused_record_request_ack),
        .record_request_slot(unused_record_request_slot),
        .record_request_generation(unused_record_request_generation),
        .record_request_epoch(unused_record_request_epoch),
        .record_request_word(unused_record_request_word),
        .record_request_ready(1'b0),
        .record_response_valid(1'b0),
        .record_response_error(4'b0),
        .record_response_data(64'b0),

        .resident_valid(unused_action_resident_valid),
        .resident_opcode(unused_action_resident_opcode),
        .resident_data(unused_action_resident_data),
        .resident_sequence(unused_action_resident_sequence),
        .resident_frame(unused_action_resident_frame),
        .resident_reference(unused_action_resident_reference),
        .resident_slot(unused_action_resident_slot),
        .resident_generation(unused_action_resident_generation),
        .resident_mode(unused_action_resident_mode),
        .lookup_ready(unused_action_lookup_ready),
        .lookup_done(unused_action_lookup_done),
        .lookup_error(unused_action_lookup_error),
        .lookup_data(unused_action_lookup_data),
        .update_ready(unused_action_update_ready),
        .update_done(unused_action_update_done),
        .update_error(unused_action_update_error),
        .cut_mode(2'b0),
        .resident_ready(1'b0),
        .resident_done(1'b0),
        .resident_pending(1'b0),
        .resident_error(4'b0),
        .resident_result(512'b0),
        .lookup_valid(1'b0),
        .lookup_slot(16'b0),
        .update_valid(1'b0),
        .update_slot(16'b0),
        .update_generation(32'b0),
        .update_data(768'b0),

        .clk(clk),.reset(reset),.current_frame(current_frame),.cut_start(cut_start),.cut_done(cut_done),.busy(action_busy),.fault(resident_fault),
        .request_valid(request_valid),.request_ready(request_ready),.request_opcode(request_opcode),
        .request_state(request_state),.request_address(request_address),.request_data(request_data),
        .response_valid(response_valid),.response_error(response_error),.response_data(response_data),
        .cfg_valid(cfg_valid),.cfg_ready(cfg_ready),.cfg_opcode(cfg_opcode),.cfg_state(cfg_state),.cfg_address(cfg_address),.cfg_data(cfg_data),
        .cfg_response_valid(cfg_response_valid),.cfg_response_error(cfg_response_error),.cfg_response_data(cfg_response_data)
    );
    image_probe image_leaf (
        .clk(clk),.reset(reset),.request_valid(cfg_valid),.request_ready(cfg_ready),.request_opcode(cfg_opcode),
        .request_state(cfg_state),.request_address(cfg_address),.request_data(cfg_data),
        .response_valid(cfg_response_valid),.response_error(cfg_response_error),.response_data(cfg_response_data),
        .fault(image_fault),.image_busy(image_busy),.run_valid(run_valid),.run_ready(run_ready),.busy(busy),.done(done),.error_code(error_code),
        .job_ready(job_ready),.job_valid(job_valid),.job_data(job_data)
    );
endmodule
