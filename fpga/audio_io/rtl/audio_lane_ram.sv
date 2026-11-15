module audio_lane_ram #(
    parameter integer DEPTH = 96,
    parameter integer ADDRESS_BITS = (DEPTH > 1) ? $clog2(DEPTH) : 1
) (
    input wire write_clk,
    input wire write_enable,
    input wire [ADDRESS_BITS-1:0] write_address,
    input wire [31:0] write_data,
    input wire read_clk,
    input wire read_enable,
    input wire [ADDRESS_BITS-1:0] read_address,
    output reg [31:0] read_data
);
    (* ram_style = "block" *) reg [31:0] memory [0:DEPTH-1];
    always @(posedge write_clk)
        if (write_enable) memory[write_address] <= write_data;
    always @(posedge read_clk)
        if (read_enable) read_data <= memory[read_address];
endmodule
