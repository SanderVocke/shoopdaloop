// One outstanding word; delay its response until the minimum service time elapses.
module memory_delay #(
    parameter integer RESPONSE_CYCLES = 1
) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready,
    input wire request_write, input wire [63:0] request_address,
    input wire [31:0] request_write_data,
    output wire response_valid, input wire response_ready,
    output wire response_error, output wire [31:0] response_read_data,
    output wire memory_valid, input wire memory_ready,
    output wire memory_write, output wire [63:0] memory_address,
    output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    localparam integer COUNTER_BITS = (RESPONSE_CYCLES < 2) ? 1 : $clog2(RESPONSE_CYCLES);
    reg pending;
    reg [COUNTER_BITS-1:0] remaining;
    initial begin
        if (RESPONSE_CYCLES < 1) $fatal(1, "Response delay must be positive");
    end
    assign request_ready = memory_ready && !pending && !reset;
    assign memory_valid = request_valid && !pending && !reset;
    assign memory_write = request_write;
    assign memory_address = request_address;
    assign memory_write_data = request_write_data;
    assign response_valid = memory_response_valid && pending && (remaining == 0) && !reset;
    assign memory_response_ready = response_ready && pending && (remaining == 0) && !reset;
    assign response_error = memory_response_error;
    assign response_read_data = memory_read_data;
    always @(posedge clk) begin
        if (reset) begin
            pending <= 0;
            remaining <= 0;
        end else if (request_valid && request_ready) begin
            pending <= 1;
            remaining <= COUNTER_BITS'(RESPONSE_CYCLES - 1);
        end else if (pending) begin
            if (remaining != 0) remaining <= remaining - 1;
            if (response_valid && response_ready) pending <= 0;
        end
    end
endmodule
