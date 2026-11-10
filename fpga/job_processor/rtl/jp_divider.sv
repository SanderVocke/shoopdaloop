module jp_divider (
    input wire clk,
    input wire reset,
    input wire start,
    input wire [63:0] numerator,
    input wire [31:0] denominator,
    output reg done,
    output reg [63:0] quotient,
    output reg [31:0] remainder
);
    reg busy;
    reg [63:0] dividend;
    reg [31:0] divisor;
    reg [6:0] left;
    wire [32:0] trial = {remainder, dividend[63]};
    wire subtract = trial >= {1'b0, divisor};

    always @(posedge clk) begin
        if (reset) begin
            busy <= 0;
            done <= 0;
            quotient <= 0;
            remainder <= 0;
            dividend <= 0;
            divisor <= 0;
            left <= 0;
        end else begin
            done <= 0;
            if (start && !busy) begin
                quotient <= 0;
                remainder <= 0;
                dividend <= numerator;
                divisor <= denominator;
                left <= 64;
                busy <= denominator != 0;
                done <= denominator == 0;
            end else if (busy) begin
                dividend <= dividend << 1;
                quotient <= {quotient[62:0], subtract};
                remainder <= subtract ? 32'(trial - {1'b0, divisor}) : trial[31:0];
                left <= left - 1'b1;
                if (left == 1) begin
                    busy <= 0;
                    done <= 1;
                end
            end
        end
    end
endmodule
