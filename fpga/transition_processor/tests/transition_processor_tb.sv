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
    transition_processor #(.LOOP_DEPTH(LOOP_DEPTH), .REQUEST_DEPTH(REQUEST_DEPTH),
                           .EVENT_DEPTH(EVENT_DEPTH)) core (.*);
endmodule
