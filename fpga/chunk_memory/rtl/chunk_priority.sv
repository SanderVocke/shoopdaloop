module chunk_priority #(
    parameter integer LINES = 4,
    parameter integer INDEX_BITS = LINES < 2 ? 1 : $clog2(LINES)
) (
    input wire [LINES-1:0] mask,
    output wire valid,
    output wire [INDEX_BITS-1:0] index
);
    generate if (LINES == 1) begin : leaf
        assign valid = mask[0];
        assign index = 0;
    end else begin : branch
        localparam integer SPLIT = 2**($clog2(LINES)-1);
        wire low_valid, high_valid;
        wire [INDEX_BITS-1:0] low_index, high_index;
        chunk_priority #(.LINES(SPLIT),.INDEX_BITS(INDEX_BITS)) low(mask[SPLIT-1:0],low_valid,low_index);
        chunk_priority #(.LINES(LINES-SPLIT),.INDEX_BITS(INDEX_BITS)) high(mask[LINES-1:SPLIT],high_valid,high_index);
        assign valid = low_valid || high_valid;
        assign index = low_valid ? low_index : high_index | INDEX_BITS'(SPLIT);
    end endgenerate
endmodule
