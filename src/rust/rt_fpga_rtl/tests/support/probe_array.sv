module probe_array (
    input wire clk, input wire reset, input wire write_enable,
    input wire [1:0] write_index, input wire [63:0] write_data,
    output reg [31:0] ticks
);
    reg [63:0] words [0:3];
    integer i;
    always @(posedge clk) begin
        if (reset) begin
            ticks <= 0;
            for (i=0; i<4; i=i+1) words[i] <= 0;
        end else begin
            ticks <= ticks + 1;
            if (write_enable) words[write_index] <= write_data;
        end
    end
endmodule
