module word_dma #(
    parameter integer DATA_WIDTH = 32
) (
    input wire clk, input wire reset,
    input wire [63:0] arena_base, input wire [63:0] arena_bytes,
    input wire [31:0] physical_base,
    input wire memory_valid, output wire memory_ready, input wire memory_write,
    input wire [63:0] memory_address, input wire [31:0] memory_write_data,
    output wire memory_response_valid, input wire memory_response_ready,
    output reg memory_response_error, output reg [31:0] memory_read_data,
    output wire command_valid,
    input wire command_ready,
    output reg command_write,
    output reg [31:0] command_address,
    output wire [7:0] command_length,
    output wire [2:0] command_size,
    output wire write_valid,
    input wire write_ready,
    output wire [DATA_WIDTH-1:0] write_data,
    output wire [DATA_WIDTH/8-1:0] write_strobe,
    input wire read_valid,
    output wire read_ready,
    input wire [DATA_WIDTH-1:0] read_data,
    input wire read_error,
    input wire read_last,
    input wire completion_valid,
    output wire completion_ready,
    input wire completion_error
);
    localparam integer LANE_BITS = $clog2(DATA_WIDTH/8);
    localparam [2:0] IDLE = 0, COMMAND = 1, WRITE = 2, READ = 3, ACK = 4, RESPONSE = 5;
    reg [2:0] phase;

    reg [31:0] payload;
    reg [LANE_BITS-1:0] lane;
    wire [63:0] offset = memory_address-arena_base;
    wire [64:0] mapped = {33'b0, physical_base}+{1'b0, offset};
    wire bad_address = memory_address < arena_base || arena_bytes < 4 || offset > arena_bytes-4
        || mapped[64:32] != 0 || mapped[31:0] > 32'hfffffffc || memory_address[1:0] != 0
        || mapped[1:0] != 0;
    assign command_valid = phase == COMMAND;

    assign command_length = 0;
    assign command_size = 2;
    assign write_valid = phase == WRITE;

    assign write_data = DATA_WIDTH'(payload) << (lane*8);
    assign write_strobe = (DATA_WIDTH/8)'(4'b1111) << lane;

    assign read_ready = phase == READ;

    assign completion_ready = phase == ACK;
    wire unused_last = &{1'b0, read_last};
    assign memory_ready = phase == IDLE && !reset;
    assign memory_response_valid = phase == RESPONSE && !reset;

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; command_write <= 0; command_address <= 0; payload <= 0; lane <= 0;
            memory_response_error <= 0; memory_read_data <= 0;
        end else begin
            if (memory_valid && memory_ready) begin
                command_write <= memory_write; command_address <= mapped[31:0];
                payload <= memory_write_data; lane <= mapped[LANE_BITS-1:0];
                memory_response_error <= bad_address; memory_read_data <= 0;
                phase <= bad_address ? RESPONSE : COMMAND;
            end
            if (command_valid && command_ready) phase <= command_write ? WRITE : READ;
            if (write_valid && write_ready) phase <= ACK;
            if (read_valid && read_ready) begin
                memory_read_data <= 32'(read_data >> (lane*8)); memory_response_error <= read_error;
                phase <= RESPONSE;
            end
            if (completion_valid && completion_ready) begin
                memory_response_error <= completion_error; phase <= RESPONSE;
            end
            if (memory_response_valid && memory_response_ready) phase <= IDLE;
        end
    end
endmodule
