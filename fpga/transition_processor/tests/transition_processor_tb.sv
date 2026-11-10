`timescale 1ns/1ps
module transition_processor_tb #(
    parameter integer LOOP_DEPTH=64,
    parameter integer REQUEST_DEPTH=64,
    parameter integer EVENT_DEPTH=192
) (
    output reg clk,
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
    output wire busy
);
    initial clk=0;
    always #5 clk<=~clk;
    wire unused_loop_read_ready, unused_loop_response_valid, unused_loop_response_error;
    wire [162:0] unused_loop_response_data;
    wire [31:0] unused_configured_quantum;
    transition_processor #(.LOOP_DEPTH(LOOP_DEPTH), .REQUEST_DEPTH(REQUEST_DEPTH),
                           .EVENT_DEPTH(EVENT_DEPTH)) core (
        .loop_read_valid(1'b0), .loop_read_slot(32'b0), .loop_read_ready(unused_loop_read_ready),
        .loop_response_valid(unused_loop_response_valid), .loop_response_error(unused_loop_response_error),
        .loop_response_data(unused_loop_response_data), .configured_quantum(unused_configured_quantum), .*);
endmodule
