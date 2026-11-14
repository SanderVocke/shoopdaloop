// Accepted cut records remain owned until a generation- and epoch-checked acknowledgement.
module cut_records #(
    parameter integer SLOTS=256, WORDS=29, A=$clog2(SLOTS*32)
) (
    input wire clk, input wire reset, input wire [31:0] epoch,
    input wire commit_valid, output wire commit_ready, input wire [15:0] commit_slot,
    input wire [31:0] commit_generation, input wire [WORDS*64-1:0] commit_data,
    output reg commit_done, output reg [3:0] commit_error,
    input wire request_valid, output wire request_ready, input wire request_ack,
    input wire [15:0] request_slot, input wire [31:0] request_generation,
    input wire [31:0] request_epoch, input wire [4:0] request_word,
    output reg response_valid, output reg [3:0] response_error, output reg [63:0] response_data
);
    localparam integer S=SLOTS > 1 ? $clog2(SLOTS) : 1;
    localparam [3:0] INVALID_ID=3, INVALID_STATE=5;
    typedef enum reg [3:0] {IDLE,COPY,HEADER,HEADER_WAIT,HEADER_GET,DATA,DATA_WAIT,DATA_GET} phase_t;
    phase_t phase;
    reg [SLOTS-1:0] valid;
    reg [S-1:0] slot;
    reg [31:0] generation, owner_epoch;
    reg [4:0] word_index;
    reg acknowledge;
    wire [63:0] read_data;
    wire commit_accept=commit_valid && commit_ready;
    wire [A-1:0] address=A'(32)*A'(slot)+A'(phase == HEADER ? 31 : word_index);
    assign commit_ready=phase == IDLE && !reset;
    assign request_ready=phase == IDLE && !commit_valid && !reset;
    sp_ram #(.WIDTH(64),.DEPTH(SLOTS*32),.ADDR_BITS(A)) records (
        .clk(clk),.read_enable(phase == HEADER || phase == DATA),.read_address(address),.read_data(read_data),
        .write_enable(phase == COPY),.write_address(address),
        .write_data(word_index == 31 ? {owner_epoch,generation} : commit_data[64*word_index+:64])
    );
    initial if (SLOTS < 1 || SLOTS > 65536 || WORDS < 1 || WORDS > 31)
        $fatal(1,"Invalid retained cut-record geometry");
    task automatic reply(input [3:0] error, input [63:0] data);
        begin response_valid<=1; response_error<=error; response_data<=data; phase<=IDLE; end
    endtask
    always @(posedge clk) begin
        commit_done<=0; response_valid<=0;
        if (reset) begin
            phase<=IDLE; valid<=0; slot<=0; generation<=0; owner_epoch<=0; word_index<=0; acknowledge<=0;
            commit_done<=0; commit_error<=0; response_valid<=0; response_error<=0; response_data<=0;
        end else case (phase)
            IDLE: begin
                if (commit_accept) begin
                    if (32'(commit_slot) >= 32'(SLOTS) || commit_generation == 0) begin commit_done<=1; commit_error<=INVALID_ID; end
                    else if (valid[S'(commit_slot)]) begin commit_done<=1; commit_error<=INVALID_STATE; end
                    else begin
                        slot<=S'(commit_slot); generation<=commit_generation; owner_epoch<=epoch;
                        word_index<=0; commit_error<=0; phase<=COPY;
                    end
                end else if (request_valid && request_ready) begin
                    if (request_epoch != epoch || 32'(request_slot) >= 32'(SLOTS) || request_generation == 0) reply(INVALID_ID,0);
                    else if (!valid[S'(request_slot)]) reply(INVALID_ID,0);
                    else if (!request_ack && request_word >= 5'(WORDS)) reply(INVALID_ID,0);
                    else begin
                        slot<=S'(request_slot); generation<=request_generation; owner_epoch<=request_epoch;
                        word_index<=request_word; acknowledge<=request_ack; phase<=HEADER;
                    end
                end
            end
            COPY: begin
                if (word_index == 31) begin valid[slot]<=1; commit_done<=1; phase<=IDLE; end
                else word_index<=word_index+1'b1 == 5'(WORDS) ? 5'd31 : word_index+1'b1;
            end
            HEADER: phase<=HEADER_WAIT;
            HEADER_WAIT: phase<=HEADER_GET;
            HEADER_GET: begin
                if (read_data != {owner_epoch,generation} || epoch != owner_epoch) reply(INVALID_ID,0);
                else if (acknowledge) begin valid[slot]<=0; reply(0,0); end
                else phase<=DATA;
            end
            DATA: phase<=DATA_WAIT;
            DATA_WAIT: phase<=DATA_GET;
            DATA_GET: reply(0,read_data);
            default: reply(INVALID_STATE,0);
        endcase
    end
endmodule
