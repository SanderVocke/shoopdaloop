module tp_priority #(
    parameter integer SLOTS = 64,
    parameter integer INDEX_BITS = (SLOTS > 1) ? $clog2(SLOTS) : 1
) (
    input wire [SLOTS-1:0] mask,
    output wire valid,
    output wire [INDEX_BITS-1:0] index
);
    generate
        if (SLOTS == 1) begin : leaf
            assign valid = mask[0];
            assign index = 0;
        end else begin : branch
            localparam integer LEFT = 2**($clog2(SLOTS)-1);
            wire left_valid, right_valid;
            wire [INDEX_BITS-1:0] left_index, right_index;
            tp_priority #(.SLOTS(LEFT), .INDEX_BITS(INDEX_BITS)) lo (
                mask[LEFT-1:0], left_valid, left_index);
            tp_priority #(.SLOTS(SLOTS-LEFT), .INDEX_BITS(INDEX_BITS)) hi (
                mask[SLOTS-1:LEFT], right_valid, right_index);
            assign valid = left_valid | right_valid;
            assign index = left_valid ? left_index : (right_index | INDEX_BITS'(LEFT));
        end
    endgenerate
endmodule
