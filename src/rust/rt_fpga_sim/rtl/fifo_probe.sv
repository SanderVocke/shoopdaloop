module probe_top (
    input wire clk,
    input wire reset,
    input wire push_valid,
    output wire push_ready,
    input wire [745:0] push_data,
    output wire pop_valid,
    input wire pop_ready,
    output wire [745:0] pop_data,
    output wire [1:0] count
);
    audio_command_fifo #(.DEPTH(3)) fifo (.*);
endmodule
