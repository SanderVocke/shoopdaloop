module audio_events #(
    parameter integer WORDS = 68,
    parameter FILE_NAME = "fpga/audio_io/examples/default/events.hex",
    parameter integer ADDRESS_BITS = (WORDS > 1) ? $clog2(WORDS) : 1
) (
    input wire clk, input wire reset, input wire enable,
    output wire valid, input wire ready, output wire [745:0] data
);
    reg [745:0] rom [0:WORDS-1];
    initial $readmemh(FILE_NAME,rom);
    reg [ADDRESS_BITS-1:0] address;
    reg finished;
    reg [63:0] next_sequence;
    assign valid = enable && !reset;
    assign data = finished ? {next_sequence,2'd2,680'b0} : rom[address];
    always @(posedge clk) begin
        if (reset) begin address<=0; finished<=0; next_sequence<=0; end
        else if (valid && ready) begin
            if (data[681:680] == 2) next_sequence<=data[745:682]+1;
            if (!finished) begin
                if (32'(address)+1 == WORDS) finished<=1;
                else address<=address+1'b1;
            end
        end
    end
endmodule
