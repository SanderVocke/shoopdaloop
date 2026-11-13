module jp_sample_ram #(
    parameter integer DEPTH = 512,
    parameter integer ADDR_BITS = (DEPTH > 1) ? $clog2(DEPTH) : 1
) (
    input wire clk,
    input wire read_enable,
    input wire [ADDR_BITS-1:0] read_address,
    output reg [31:0] read_data,
    input wire write_enable,
    input wire [ADDR_BITS-1:0] write_address,
    input wire [31:0] write_data
);
    (* ram_style = "block" *) reg [31:0] memory [0:DEPTH-1];
    always @(posedge clk) begin
        if (read_enable)
            read_data <= memory[read_address];
        if (write_enable)
            memory[write_address] <= write_data;
    end
endmodule
