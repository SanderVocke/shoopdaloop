module native_probe #(parameter integer REQUESTS=4) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready, input wire [7:0] request_opcode,
    input wire [31:0] request_index, input wire [639:0] request_data,
    output wire request_done, output wire [3:0] request_error, output wire [511:0] request_result,
    input wire command_valid, output wire command_ready, input wire [7:0] command_opcode,
    input wire [31:0] command_index, input wire [639:0] command_data,
    output wire response_valid, input wire response_ready,
    output wire [3:0] response_error, output wire [831:0] response_data,
    input wire execute_valid, input wire execute_finish, output wire execute_ready, output wire execute_done, output wire [3:0] execute_error,
    output wire busy
);
    wire tp_valid,tp_ready,tp_done;
    wire [7:0] tp_opcode;
    wire [31:0] tp_index,quantum;
    wire [639:0] tp_data;
    wire [3:0] tp_error;
    wire [831:0] tp_result;
    native_requests #(.REQUESTS(REQUESTS)) requests (.response_valid(request_done),.response_error(request_error),.response_data(request_result),.*);
    wire unused_loop_ready,unused_loop_valid,unused_loop_error,unused_observe_ready,unused_observe_valid,unused_observe_error;
    wire [163:0] unused_loop_data;
    wire [63:0] unused_observe_frame;
    wire [1:0] unused_observe_phase;
    wire [831:0] unused_observe_data;
    wire unused=&{1'b0,quantum,unused_loop_ready,unused_loop_valid,unused_loop_error,unused_loop_data,
        unused_observe_ready,unused_observe_valid,unused_observe_error,unused_observe_frame,unused_observe_phase,unused_observe_data};
    transition_processor #(.LOOP_DEPTH(1),.REQUEST_DEPTH(REQUESTS),.EVENT_DEPTH(2+REQUESTS),.RESIDENT_ROSTER(1)) timing (
        .clk(clk),.reset(reset),.command_valid(command_valid),.command_ready(command_ready),.command_opcode(command_opcode),
        .command_index(command_index),.command_data(command_data),.response_valid(response_valid),.response_ready(response_ready),
        .response_error(response_error),.response_data(response_data),.busy(busy),
        .execute_valid(execute_valid),.execute_ready(execute_ready),.execute_finish(execute_finish),.execute_done(execute_done),.execute_error(execute_error),
        .resident_valid(tp_valid),.resident_ready(tp_ready),.resident_opcode(tp_opcode),.resident_index(tp_index),.resident_data(tp_data),
        .resident_done(tp_done),.resident_error(tp_error),.resident_result(tp_result),
        .loop_read_valid(1'b0),.loop_read_ready(unused_loop_ready),.loop_read_slot(32'b0),.loop_response_valid(unused_loop_valid),.loop_response_error(unused_loop_error),.loop_response_data(unused_loop_data),
        .observe_valid(1'b0),.observe_ready(unused_observe_ready),.observe_kind(2'b0),.observe_index(32'b0),
        .observe_response_valid(unused_observe_valid),.observe_response_error(unused_observe_error),.observe_frame(unused_observe_frame),.observe_phase(unused_observe_phase),.observe_data(unused_observe_data),.configured_quantum(quantum)
    );
endmodule
