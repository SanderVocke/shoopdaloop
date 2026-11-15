`include "state_processor_defs.svh"

module sp_job_fifo #(
    parameter integer DEPTH = 256,
    parameter integer ADDR_BITS = (DEPTH > 1) ? $clog2(DEPTH) : 1,
    parameter integer COUNT_BITS = $clog2(DEPTH + 1)
) (
    input  wire                           clk,
    input  wire                           reset,
    input  wire                           push_valid,
    output wire                           push_ready,
    input  wire [`SHOOP_SP_JOB_BITS-1:0]    push_data,
    output reg                            pop_valid,
    input  wire                           pop_ready,
    output reg  [`SHOOP_SP_JOB_BITS-1:0]    pop_data,
    output reg  [COUNT_BITS-1:0]           count
);
    reg [ADDR_BITS-1:0] write_pointer;
    reg [ADDR_BITS-1:0] read_pointer;
    reg read_pending;
    wire push = push_valid && push_ready;
    wire pop = pop_valid && pop_ready;
    wire read_enable = (count != 0) && !pop_valid && !read_pending && !reset;
    wire [`SHOOP_SP_JOB_BITS-1:0] read_data;

    assign push_ready = (count < COUNT_BITS'(DEPTH)) && !reset;

    sp_ram #(
        .WIDTH(`SHOOP_SP_JOB_BITS), .DEPTH(DEPTH), .ADDR_BITS(ADDR_BITS)
    ) storage (
        .clk(clk),
        .read_enable(read_enable), .read_address(read_pointer), .read_data(read_data),
        .write_enable(push), .write_address(write_pointer), .write_data(push_data)
    );

    always @(posedge clk) begin
        if (reset) begin
            write_pointer <= 0;
            read_pointer <= 0;
            read_pending <= 0;
            pop_valid <= 0;
            pop_data <= 0;
            count <= 0;
        end else begin
            read_pending <= read_enable;
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
