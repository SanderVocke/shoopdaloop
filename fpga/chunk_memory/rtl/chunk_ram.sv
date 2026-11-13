module chunk_ram #(
    parameter integer DEPTH = 128,
    parameter integer ADDR_BITS = (DEPTH < 2) ? 1 : $clog2(DEPTH)
) (
    input wire clk,
    input wire a_enable, input wire a_write,
    input wire [ADDR_BITS-1:0] a_address, input wire [31:0] a_write_data,
    output reg [31:0] a_read_data,
    input wire b_enable, input wire b_write,
    input wire [ADDR_BITS-1:0] b_address, input wire [31:0] b_write_data,
    output reg [31:0] b_read_data
);
    generate if (DEPTH == 1) begin : single_word
        reg [31:0] samples [0:0];
        always @(posedge clk) if (a_enable) begin
            a_read_data <= samples[a_address];
            if (a_write) samples[a_address] <= a_write_data;
        end
        always @(posedge clk) if (b_enable) begin
            b_read_data <= samples[b_address];
            if (b_write) samples[b_address] <= b_write_data;
        end
    end else begin : block_memory
        (* ram_style = "block" *) reg [31:0] samples [0:DEPTH-1];
        always @(posedge clk) if (a_enable) begin
            a_read_data <= samples[a_address];
            if (a_write) samples[a_address] <= a_write_data;
        end
        always @(posedge clk) if (b_enable) begin
            b_read_data <= samples[b_address];
            if (b_write) samples[b_address] <= b_write_data;
        end
    end endgenerate
endmodule
