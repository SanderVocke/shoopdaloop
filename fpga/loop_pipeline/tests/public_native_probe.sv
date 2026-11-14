module public_native_probe #(
    parameter integer REQUESTS=4, SLOTS=64, QUANTUM=16
) (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready, input wire [7:0] command_opcode,
    input wire [31:0] command_index, input wire [639:0] command_data,
    output wire response_valid, input wire response_ready,
    output wire [3:0] response_error, output wire [831:0] response_data,
    input wire control_valid, output wire control_ready, input wire [7:0] control_opcode,
    input wire control_state, input wire [15:0] control_address, input wire [639:0] control_data,
    output wire control_done, output wire [3:0] control_error, output wire [831:0] control_result,
    input wire execute_valid, input wire execute_finish, output wire execute_ready,
    output wire execute_done, output wire [3:0] execute_error, output wire busy,
    input wire cut_start, input wire [1:0] cut_mode, output wire cut_done,
    output wire fault, output wire [63:0] current_frame,
    input wire observe_valid, output wire observe_ready, input wire [1:0] observe_kind,
    input wire [31:0] observe_index, output wire observe_response_valid,
    output wire observe_error, output wire [63:0] observe_frame,
    output wire [1:0] observe_phase, output wire [831:0] observe_data
);
    wire resident_valid,resident_ready,resident_done,resident_pending;
    wire [7:0] resident_opcode;
    wire [511:0] resident_data,resident_result;
    wire [63:0] resident_sequence,resident_frame;
    wire [47:0] resident_reference;
    wire [15:0] resident_slot;
    wire [31:0] resident_generation;
    wire [1:0] resident_mode;
    wire [3:0] resident_error;
    wire lookup_valid,lookup_ready,lookup_done,update_valid,update_ready,update_done;
    wire [15:0] lookup_slot,update_slot;
    wire [31:0] update_generation;
    wire [3:0] lookup_error,update_error;
    wire [767:0] lookup_data,update_data;
    wire cfg_valid,cfg_state,actions_busy,actions_fault,native_fault;
    wire [7:0] cfg_opcode;
    wire [15:0] cfg_address;
    wire [639:0] cfg_data;
    wire tp_valid,tp_ready,tp_done;
    wire [7:0] tp_opcode;
    wire [31:0] tp_index,quantum;
    wire [639:0] tp_data;
    wire [3:0] tp_error;
    wire [831:0] tp_result;
    assign fault=actions_fault || native_fault;
    assign current_frame=timing.frame;
    wire unused_record_request_valid;
    wire unused_record_request_ack;
    wire [15:0] unused_record_request_slot;
    wire [31:0] unused_record_request_generation;
    wire [31:0] unused_record_request_epoch;
    wire [4:0] unused_record_request_word;
    audio_actions #(.SLOTS(SLOTS),.QUANTUM(QUANTUM),.NATIVE_ACTIONS(1)) actions (
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

        .clk(clk),.reset(reset),.current_frame(current_frame),.cut_start(cut_start),.cut_mode(cut_mode),.cut_done(cut_done),
        .busy(actions_busy),.fault(actions_fault),.request_valid(control_valid),.request_ready(control_ready),
        .request_opcode(control_opcode),.request_state(control_state),.request_address(control_address),.request_data(control_data),
        .response_valid(control_done),.response_error(control_error),.response_data(control_result),
        .cfg_valid(cfg_valid),.cfg_ready(1'b0),.cfg_opcode(cfg_opcode),.cfg_state(cfg_state),.cfg_address(cfg_address),.cfg_data(cfg_data),
        .cfg_response_valid(1'b0),.cfg_response_error(4'b0),.cfg_response_data(129'b0),.*
    );
    wire unused_observation_valid;
    wire [7:0] unused_observation_opcode;
    wire [511:0] unused_observation_data;
    wire unused_configuration_valid;
    wire [7:0] unused_configuration_opcode;
    wire [511:0] unused_configuration_data;
    wire unused_signal_valid;
    wire [511:0] unused_signal_data;
    audio_native #(.REQUESTS(REQUESTS)) native (
        .signal_valid(unused_signal_valid),.signal_ready(1'b0),.signal_data(unused_signal_data),.signal_done(1'b0),.signal_result(512'b0),
        .configuration_valid(unused_configuration_valid),.configuration_ready(1'b0),.configuration_opcode(unused_configuration_opcode),
        .configuration_data(unused_configuration_data),.configuration_done(1'b0),.configuration_result(512'b0),
        .observation_valid(unused_observation_valid),.observation_opcode(unused_observation_opcode),.observation_data(unused_observation_data),
        .observation_ready(1'b0),.observation_done(1'b0),.observation_error(4'b0),.observation_result(512'b0),
        .clk(clk),.reset(reset),.request_valid(resident_valid),.request_ready(resident_ready),.request_opcode(resident_opcode),
        .request_flags(cfg_address),.request_mode(resident_mode),.request_data(resident_data),.request_sequence(resident_sequence),
        .request_frame(resident_frame),.request_reference(resident_reference),.response_valid(resident_done),
        .response_pending(resident_pending),.response_error(resident_error),.response_data(resident_result),.fault(native_fault),.*
    );
    wire unused_loop_ready,unused_loop_valid,unused_loop_error;
    wire [163:0] unused_loop_data;
    wire unused=&{1'b0,resident_slot,resident_generation,cfg_valid,cfg_state,cfg_opcode,cfg_data,actions_busy,quantum,
        unused_loop_ready,unused_loop_valid,unused_loop_error,unused_loop_data};
    transition_processor #(.LOOP_DEPTH(1),.REQUEST_DEPTH(REQUESTS),.EVENT_DEPTH(2+REQUESTS),.RESIDENT_ROSTER(1)) timing (
        .clk(clk),.reset(reset),.command_valid(command_valid),.command_ready(command_ready),.command_opcode(command_opcode),
        .command_index(command_index),.command_data(command_data),.response_valid(response_valid),.response_ready(response_ready),
        .response_error(response_error),.response_data(response_data),.busy(busy),
        .execute_valid(execute_valid),.execute_ready(execute_ready),.execute_finish(execute_finish),.execute_done(execute_done),.execute_error(execute_error),
        .resident_valid(tp_valid),.resident_ready(tp_ready),.resident_opcode(tp_opcode),.resident_index(tp_index),.resident_data(tp_data),
        .resident_done(tp_done),.resident_error(tp_error),.resident_result(tp_result),
        .loop_read_valid(1'b0),.loop_read_ready(unused_loop_ready),.loop_read_slot(32'b0),.loop_response_valid(unused_loop_valid),
        .loop_response_error(unused_loop_error),.loop_response_data(unused_loop_data),.configured_quantum(quantum),.observe_response_error(observe_error),.*
    );
endmodule
