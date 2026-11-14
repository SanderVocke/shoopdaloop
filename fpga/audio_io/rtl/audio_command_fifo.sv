module audio_command_fifo #(
    parameter integer WIDTH = 746,
    parameter integer DEPTH = 256,
    parameter integer ENABLE_PEEK = 0,
    parameter integer ADDR_BITS = (DEPTH > 1) ? $clog2(DEPTH) : 1,
    parameter integer COUNT_BITS = $clog2(DEPTH + 1)
) (
    input  wire                           clk,
    input  wire                           reset,
    input  wire                           push_valid,
    output wire                           push_ready,
    input  wire [WIDTH-1:0]    push_data,
    output reg                            pop_valid,
    input  wire                           pop_ready,
    output reg  [WIDTH-1:0]    pop_data,
    output reg  [COUNT_BITS-1:0]           count,
    input wire peek_valid, output wire peek_ready, input wire [31:0] peek_offset,
    output wire peek_response_valid, output wire [WIDTH-1:0] peek_data
);
    reg [ADDR_BITS-1:0] write_pointer;
    reg [ADDR_BITS-1:0] read_pointer;
    reg read_pending;
    reg peek_pending;
    wire push = push_valid && push_ready;
    wire pop = pop_valid && pop_ready;
    wire prefetch = (count != 0) && !pop_valid && !read_pending && !peek_pending && !reset;
    assign peek_ready = ENABLE_PEEK != 0 && pop_valid && !read_pending && !peek_pending
        && peek_offset < 32'(count) && !reset;
    wire peek = peek_valid && peek_ready;
    wire read_enable = prefetch || peek;
    wire [ADDR_BITS:0] peek_sum = {1'b0,read_pointer}+{1'b0,ADDR_BITS'(peek_offset)};
    wire [ADDR_BITS-1:0] peek_address = ADDR_BITS'(peek_sum >= (ADDR_BITS+1)'(DEPTH)
        ? peek_sum-(ADDR_BITS+1)'(DEPTH) : peek_sum);
    wire [WIDTH-1:0] read_data;
    assign peek_response_valid = peek_pending && !reset;
    assign peek_data = read_data;

    assign push_ready = (count < COUNT_BITS'(DEPTH)) && !reset;

    sp_ram #(
        .WIDTH(WIDTH), .DEPTH(DEPTH), .ADDR_BITS(ADDR_BITS)
    ) storage (
        .clk(clk),
        .read_enable(read_enable), .read_address(peek ? peek_address : read_pointer), .read_data(read_data),
        .write_enable(push), .write_address(write_pointer), .write_data(push_data)
    );

    always @(posedge clk) begin
        if (reset) begin
            write_pointer <= 0;
            read_pointer <= 0;
            read_pending <= 0;
            peek_pending <= 0;
            pop_valid <= 0;
            pop_data <= 0;
            count <= 0;
        end else begin
            read_pending <= prefetch;
            peek_pending <= peek;
            if (read_pending) begin
                pop_data <= read_data;
                pop_valid <= 1;
            end
            if (push) begin
                write_pointer <= (write_pointer == ADDR_BITS'(DEPTH - 1))
                    ? {ADDR_BITS{1'b0}} : write_pointer + 1'b1;
            end
            if (pop) begin
                pop_valid <= 0;
                read_pointer <= (read_pointer == ADDR_BITS'(DEPTH - 1))
                    ? {ADDR_BITS{1'b0}} : read_pointer + 1'b1;
            end
            case ({push, pop})
                2'b10: count <= count + 1'b1;
                2'b01: count <= count - 1'b1;
                default: begin end
            endcase
        end
    end
endmodule
