module transition_measure_adapter #(
    parameter integer LOOP_DEPTH = 64,
    parameter integer REQUEST_DEPTH = 64,
    parameter integer EVENT_DEPTH = 192,
    parameter integer INTEGRATED = 0,
    parameter integer HAS_EXEC_INTERFACE = 1
) (
    input wire clk,
    input wire reset,
    input wire command_valid,
    output wire command_ready,
    input wire [7:0] command_opcode,
    input wire [31:0] command_index,
    input wire [639:0] command_data,
    output wire response_valid,
    input wire response_ready,
    output wire [3:0] response_error,
    output wire [831:0] response_data,
    output wire busy,
    input wire loop_read_valid,
    output wire loop_read_ready,
    input wire [31:0] loop_read_slot,
    output wire loop_response_valid,
    output wire loop_response_error,
    output wire [163:0] loop_response_data,
    output wire [31:0] configured_quantum,
    input wire execute_valid,
    output wire execute_ready,
    input wire execute_finish,
    output wire execute_done,
    output wire [3:0] execute_error
);
    if (HAS_EXEC_INTERFACE != 0) begin : independent_execution
    (* KEEP_HIERARCHY = "yes" *) transition_processor #(
        .LOOP_DEPTH(LOOP_DEPTH), .REQUEST_DEPTH(REQUEST_DEPTH),
        .EVENT_DEPTH(EVENT_DEPTH), .INTEGRATED(INTEGRATED)
    ) processor (
        .clk(clk),
        .reset(reset),
        .command_valid(command_valid),
        .command_ready(command_ready),
        .command_opcode(command_opcode),
        .command_index(command_index),
        .command_data(command_data),
        .response_valid(response_valid),
        .response_ready(response_ready),
        .response_error(response_error),
        .response_data(response_data),
        .busy(busy),
        .loop_read_valid(loop_read_valid),
        .loop_read_ready(loop_read_ready),
        .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid),
        .loop_response_error(loop_response_error),
        .loop_response_data(loop_response_data),
        .observe_valid(1'b0),.observe_ready(),.observe_kind(2'b0),.observe_index(32'b0),
        .observe_response_valid(),.observe_response_error(),.observe_frame(),.observe_phase(),.observe_data(),
        .resident_valid(1'b0),.resident_ready(),.resident_opcode(8'b0),.resident_index(32'b0),.resident_data(640'b0),
        .resident_done(),.resident_error(),.resident_result(),
        .configured_quantum(configured_quantum),
        .execute_valid(execute_valid),
        .execute_ready(execute_ready),
        .execute_finish(execute_finish),
        .execute_done(execute_done),
        .execute_error(execute_error)
    );
    end
    else begin : shared_host_execution
    (* KEEP_HIERARCHY = "yes" *) transition_processor #(
        .LOOP_DEPTH(LOOP_DEPTH), .REQUEST_DEPTH(REQUEST_DEPTH),
        .EVENT_DEPTH(EVENT_DEPTH), .INTEGRATED(INTEGRATED)
    ) processor (
        .clk(clk),
        .reset(reset),
        .command_valid(command_valid),
        .command_ready(command_ready),
        .command_opcode(command_opcode),
        .command_index(command_index),
        .command_data(command_data),
        .response_valid(response_valid),
        .response_ready(response_ready),
        .response_error(response_error),
        .response_data(response_data),
        .busy(busy),
        .loop_read_valid(loop_read_valid),
        .loop_read_ready(loop_read_ready),
        .loop_read_slot(loop_read_slot),
        .loop_response_valid(loop_response_valid),
        .loop_response_error(loop_response_error),
        .loop_response_data(loop_response_data),
        .observe_valid(1'b0),.observe_ready(),.observe_kind(2'b0),.observe_index(32'b0),
        .observe_response_valid(),.observe_response_error(),.observe_frame(),.observe_phase(),.observe_data(),
        .resident_valid(1'b0),.resident_ready(),.resident_opcode(8'b0),.resident_index(32'b0),.resident_data(640'b0),
        .resident_done(),.resident_error(),.resident_result(),
        .configured_quantum(configured_quantum)
    );
    assign execute_ready = 0;
    assign execute_done = 0;
    assign execute_error = 0;
    end
endmodule
