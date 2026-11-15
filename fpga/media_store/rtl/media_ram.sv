module media_ram #(
    parameter integer WIDTH = 32, parameter integer DEPTH = 32,
    parameter integer ADDR_BITS = (DEPTH < 2) ? 1 : $clog2(DEPTH)
) (
    input wire clk, input wire read_enable, input wire [ADDR_BITS-1:0] read_address,
    output reg [WIDTH-1:0] read_data,
    input wire write_enable, input wire [ADDR_BITS-1:0] write_address,
    input wire [WIDTH-1:0] write_data
);
    (* ram_style = "block" *) reg [WIDTH-1:0] memory [0:DEPTH-1];
    always @(posedge clk) begin
        if (read_enable) read_data <= memory[read_address];
        if (write_enable) memory[write_address] <= write_data;
    end
endmodule
