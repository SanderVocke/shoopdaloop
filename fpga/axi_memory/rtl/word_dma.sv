module word_dma #(
    parameter integer DATA_WIDTH = 32,
    parameter integer REGISTER_REQUEST = 0,
    parameter integer RECORD_BURSTS = 0
) (
    input wire clk, input wire reset,
    input wire [63:0] arena_base, input wire [63:0] arena_bytes,
    input wire [31:0] physical_base,
    input wire memory_valid, output wire memory_ready, input wire memory_write,
    input wire [63:0] memory_address, input wire [31:0] memory_write_data,
    input wire memory_record, input wire [127:0] memory_write_record,
    output wire memory_response_valid, input wire memory_response_ready,
    output reg memory_response_error, output reg [31:0] memory_read_data,
    output reg [127:0] memory_read_record,
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
    localparam [2:0] IDLE = 0, COMMAND = 1, WRITE = 2, READ = 3, ACK = 4, RESPONSE = 5, CHECK = 6, DRAIN = 7;
    reg [2:0] phase;

    reg [31:0] payload;
    reg [127:0] record_payload;
    reg record_transfer, burst, wide;
    reg [1:0] record_word;
    reg [LANE_BITS-1:0] lane;
    reg [63:0] request_address,request_base,request_bytes;
    reg [31:0] request_physical;
    wire [63:0] checked_address=REGISTER_REQUEST!=0 ? request_address : memory_address;
    wire [63:0] checked_base=REGISTER_REQUEST!=0 ? request_base : arena_base;
    wire [63:0] checked_bytes=REGISTER_REQUEST!=0 ? request_bytes : arena_bytes;
    wire [31:0] checked_physical=REGISTER_REQUEST!=0 ? request_physical : physical_base;
    wire [63:0] offset = checked_address-checked_base;
    wire [64:0] mapped = {33'b0, checked_physical}+{1'b0, offset};
    wire request_record = RECORD_BURSTS != 0 && memory_record;
    wire checked_record = REGISTER_REQUEST != 0 ? record_transfer : request_record;
    wire [63:0] transfer_bytes = checked_record ? 64'd16 : 64'd4;
    wire bad_address = checked_address < checked_base || checked_bytes < transfer_bytes || offset > checked_bytes-transfer_bytes
        || mapped[64:32] != 0 || mapped[31:0] > (checked_record ? 32'hfffffff0 : 32'hfffffffc) || checked_address[1:0] != 0
        || mapped[1:0] != 0 || (checked_record && checked_address > 64'hfffffffffffffff0);
    wire can_burst = checked_record && mapped[11:0] <= 12'd4080 && (DATA_WIDTH != 64 || !mapped[2]);
    wire last_record_beat = record_word == (wide ? 2'd2 : 2'd3);
    wire [31:0] out_word = record_transfer ? record_payload[32*record_word+:32] : payload;
    assign command_valid = phase == COMMAND;

    assign command_length = burst ? (wide ? 8'd1 : 8'd3) : 0;
    assign command_size = wide ? 3'd3 : 3'd2;
    assign write_valid = phase == WRITE;

    assign write_data = wide ? DATA_WIDTH'(record_payload[64*record_word[1]+:64]) : DATA_WIDTH'(out_word) << (lane*8);
    assign write_strobe = wide ? {DATA_WIDTH/8{1'b1}} : (DATA_WIDTH/8)'(4'b1111) << lane;

    assign read_ready = phase == READ || phase == DRAIN;

    assign completion_ready = phase == ACK;
    wire unused_last = &{1'b0, read_last};
    assign memory_ready = phase == IDLE && !reset;
    assign memory_response_valid = phase == RESPONSE && !reset;

    always @(posedge clk) begin
        if (reset) begin
            request_address<=0;request_base<=0;request_bytes<=0;request_physical<=0;
            phase <= IDLE; command_write <= 0; command_address <= 0; payload <= 0; lane <= 0;
            memory_response_error <= 0; memory_read_data <= 0;
            record_payload <= 0; memory_read_record <= 0; record_transfer <= 0;
            record_word <= 0; burst <= 0; wide <= 0;
        end else begin
            if (memory_valid && memory_ready) begin
                command_write <= memory_write;payload <= memory_write_data;memory_read_data <= 0;
                record_transfer <= request_record; record_word <= 0;
                if (request_record) record_payload <= memory_write_record;
                if(REGISTER_REQUEST!=0) begin
                    request_address<=memory_address;request_base<=arena_base;request_bytes<=arena_bytes;request_physical<=physical_base;
                    phase<=CHECK;
                end else begin
                    command_address<=mapped[31:0];lane<=mapped[LANE_BITS-1:0];
                    burst<=can_burst;wide<=can_burst && DATA_WIDTH==64;
                    memory_response_error<=bad_address;phase<=bad_address ? RESPONSE : COMMAND;
                end
            end
            if(phase==CHECK) begin
                command_address<=mapped[31:0];lane<=mapped[LANE_BITS-1:0];
                burst<=can_burst;wide<=can_burst && DATA_WIDTH==64;
                memory_response_error<=bad_address;phase<=bad_address ? RESPONSE : COMMAND;
            end
            if (command_valid && command_ready) phase <= command_write ? WRITE : READ;
            if (write_valid && write_ready) begin
                if (burst && !last_record_beat) record_word <= record_word + (wide ? 2'd2 : 2'd1);
                else phase <= ACK;
            end
            if (phase==READ && read_valid) begin
                if (read_error) begin
                    memory_response_error <= 1;
                    phase <= read_last ? RESPONSE : DRAIN;
                end else if (record_transfer) begin
                    if (wide) memory_read_record[64*record_word[1]+:64] <= 64'(read_data);
                    else memory_read_record[32*record_word+:32] <= 32'(read_data >> (lane*8));
                    if (last_record_beat) phase <= RESPONSE;
                    else begin
                        record_word <= record_word + (wide ? 2'd2 : 2'd1);
                        if (!burst) begin
                            command_address <= command_address + 32'd4;
                            lane <= lane + LANE_BITS'(4);
                            phase <= COMMAND;
                        end
                    end
                end else begin
                    memory_read_data <= 32'(read_data >> (lane*8));
                    phase <= RESPONSE;
                end
            end
            if (phase==DRAIN && read_valid && read_last) phase<=RESPONSE;
            if (completion_valid && completion_ready) begin
                if (record_transfer && !burst && !last_record_beat && !completion_error) begin
                    record_word <= record_word + 1'b1;
                    command_address <= command_address + 32'd4;
                    lane <= lane + LANE_BITS'(4);
                    phase <= COMMAND;
                end else begin
                    memory_response_error <= completion_error;
                    phase <= RESPONSE;
                end
            end
            if (memory_response_valid && memory_response_ready) phase <= IDLE;
        end
    end
endmodule
