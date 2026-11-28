module host_recovery #(
    parameter integer TIMEOUT_CYCLES = 100_000_000,
    parameter integer RESET_CYCLES = 128
) (
    input wire clk, reset,
    input wire [31:0] seed_epoch,
    input wire request,
    input wire quiescent, bus_fault,
    input wire write_start, write_done, read_start, read_done,
    output wire quiesce,
    output wire datapath_reset,
    output reg [31:0] epoch,
    output reg [2:0] state
);
    localparam [2:0] IDLE=0, DRAIN=1, RESET=2, READY=3, FAILED=4;
    reg [31:0] cycles;
    reg [15:0] writes, reads;
    reg unsafe_bus;
    reg quiet;
    assign quiesce = state == DRAIN || state == RESET || state == FAILED;
    assign datapath_reset = state == RESET;
    initial begin
        if (TIMEOUT_CYCLES < 1 || RESET_CYCLES < 2) $fatal(1,"invalid recovery bounds");
    end
    always @(posedge clk) begin
        if (reset) begin
            state <= IDLE;
            epoch <= seed_epoch;
            cycles <= 0;
            writes <= 0;
            reads <= 0;
            unsafe_bus <= 0;
            quiet <= 0;
        end else begin
            if (bus_fault) unsafe_bus <= 1;
            case ({write_start,write_done})
                2'b10: if (writes == 16'hffff) unsafe_bus <= 1; else writes <= writes+1'b1;
                2'b01: if (writes == 0) unsafe_bus <= 1; else writes <= writes-1'b1;
                default: ;
            endcase
            case ({read_start,read_done})
                2'b10: if (reads == 16'hffff) unsafe_bus <= 1; else reads <= reads+1'b1;
                2'b01: if (reads == 0) unsafe_bus <= 1; else reads <= reads-1'b1;
                default: ;
            endcase
            case (state)
                IDLE, READY: if (request) begin
                    state <= DRAIN;
                    cycles <= 0;
                    quiet <= 0;
                end
                DRAIN: begin
                    cycles <= cycles+1;
                    quiet <= quiescent && writes == 0 && reads == 0 && !write_start && !read_start;
                    if (bus_fault || unsafe_bus || epoch == 0 || epoch == 32'hffffffff || cycles >= TIMEOUT_CYCLES-1)
                        state <= FAILED;
                    else if (quiet && quiescent && writes == 0 && reads == 0 && !write_start && !read_start) begin
                        state <= RESET;
                        epoch <= epoch+1'b1;
                        cycles <= 0;
                    end
                end
                RESET: begin
                    if (write_start || read_start || bus_fault || unsafe_bus) state <= FAILED;
                    else if (cycles == RESET_CYCLES-1) state <= READY;
                    cycles <= cycles+1;
                end
                default: ;
            endcase
        end
    end
endmodule
