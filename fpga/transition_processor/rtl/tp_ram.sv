module tp_ram #(
    parameter integer WIDTH = 64,
    parameter integer DEPTH = 128,
    parameter integer ADDR_BITS = (DEPTH > 1) ? $clog2(DEPTH) : 1
) (
    input wire clk,
    input wire read_enable,
    input wire [ADDR_BITS-1:0] read_address,
    output reg [WIDTH-1:0] read_data,
    input wire write_enable,
    input wire [ADDR_BITS-1:0] write_address,
    input wire [WIDTH-1:0] write_data
);
    (* ram_style = "block", no_rw_check *) reg [WIDTH-1:0] memory [0:DEPTH-1];
    always @(posedge clk) begin
        if (read_enable) read_data <= memory[read_address];
        if (write_enable) memory[write_address] <= write_data;
    end
`ifdef COCOTB_SIM
    always @(posedge clk) begin
        if (read_enable && write_enable && read_address == write_address)
            $fatal(1, "transition table read/write collision");
    end
`endif
endmodule
