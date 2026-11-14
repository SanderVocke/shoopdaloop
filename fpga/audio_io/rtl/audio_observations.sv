// The enclosing execution cut holds observation inputs through the record copy.
module audio_observations #(
    parameter integer SLOTS=256
) (
    input wire clk, input wire reset, input wire [31:0] epoch,
    input wire request_valid, output wire request_ready, input wire [7:0] request_opcode,
    input wire [511:0] request_data, input wire [15:0] request_slot, input wire [31:0] request_generation,
    input wire [1727:0] cut_data, input wire [96:0] scalar_data, input wire [63:0] topology_epoch,
    output reg response_valid, output reg [3:0] response_error, output reg [511:0] response_data, output reg fault,
    output wire ledger_valid, input wire ledger_ready, output wire [2:0] ledger_opcode,
    output wire ledger_latest, output wire [31:0] ledger_slot, output wire [31:0] ledger_generation,
    output wire [63:0] ledger_epoch, input wire ledger_done, input wire [3:0] ledger_error,
    output wire record_valid, input wire record_ready, output wire [15:0] record_slot,
    output wire [31:0] record_generation, output wire [1919:0] record_data,
    input wire record_done, input wire [3:0] record_error
);
    typedef enum reg [2:0] {IDLE,CHECK_QUERY,LEDGER_SEND,LEDGER_WAIT,COMMIT_SEND,COMMIT_WAIT} phase_t;
    phase_t phase;
    reg [7:0] operation;
    reg [511:0] payload;
    reg [15:0] slot;
    reg [31:0] generation,session;
    wire [1727:0] snapshot=operation == 12 ? {576'b0,32'b0,32'hffffffff,cut_data[1087:0]} : cut_data;
    assign request_ready=phase == IDLE && !reset && !fault;
    assign ledger_valid=phase == LEDGER_SEND && !fault;
    assign ledger_opcode=operation == 13 ? 3'd1 : operation == 14 ? 3'd2 : operation == 19 ? 3'd3 : 3'd5;
    assign ledger_latest=payload[64];
    assign ledger_slot=payload[31:0];
    assign ledger_generation=payload[63:32];
    assign ledger_epoch=cut_data[959:896];
    assign record_valid=phase == COMMIT_SEND && session == epoch && !fault;
    assign record_slot=slot;
    assign record_generation=generation;
    assign record_data={31'b0,topology_epoch,scalar_data,snapshot};
    wire unused=&{1'b0,payload[495:66],payload[511:504]};
    task automatic finish(input [7:0] tag, input [511:0] data);
        begin response_valid<=1;response_data<=data;response_data[511:504]<=tag;phase<=IDLE;end
    endtask
    task automatic runtime_failure(input [7:0] code);
        begin finish(1,{504'b0,code});end
    endtask
    task automatic internal_failure;
        begin fault<=1;response_error<=5;finish(0,0);end
    endtask
    task automatic finish_record;
        reg [511:0] reference;
        begin
            reference=0;reference[15:0]=slot;reference[47:16]=generation;reference[79:48]=session;
            reference[87:80]=30;reference[88]=operation == 13;reference[159:96]=cut_data[895:832];
            reference[223:160]=cut_data[959:896];reference[287:224]=topology_epoch;
            finish(5,reference);
        end
    endtask
    initial if (SLOTS < 1 || SLOTS > 65536) $fatal(1,"Invalid public observation capacity");
    always @(posedge clk) begin
        response_valid<=0;
        if (reset) begin
            phase<=IDLE;operation<=0;payload<=0;slot<=0;generation<=0;session<=0;
            response_valid<=0;response_error<=0;response_data<=0;fault<=0;
        end else if (!fault) case (phase)
            IDLE: if (request_valid) begin
                operation<=request_opcode;payload<=request_data;slot<=request_slot;generation<=request_generation;session<=epoch;response_error<=0;
                if (request_generation == 0 || 32'(request_slot) >= 32'(SLOTS)) internal_failure();
                else if (request_opcode == 12 || request_opcode == 13) phase<=CHECK_QUERY;
                else if (request_data[503:496] != 0) runtime_failure(request_data[503:496] <= 21 ? request_data[503:496] : 8'd2);
                else if (request_opcode == 14 || request_opcode == 19 || request_opcode == 20) phase<=LEDGER_SEND;
                else runtime_failure(2);
            end
            CHECK_QUERY: begin
                if (!cut_data[0]) runtime_failure(6);
                else if (payload[31:0] != 0 || payload[63:32] != cut_data[32:1]) finish(2,512'd3);
                else if (payload[503:496] != 0) runtime_failure(payload[503:496] <= 21 ? payload[503:496] : 8'd2);
                else if (operation == 13 && payload[65:64] > 1) runtime_failure(2);
                else phase<=operation == 12 ? COMMIT_SEND : LEDGER_SEND;
            end
            LEDGER_SEND: if (ledger_ready) phase<=LEDGER_WAIT;
            LEDGER_WAIT: if (ledger_done) begin
                if (ledger_error != 0) begin
                    case (ledger_error)
                        2: runtime_failure(3);
                        3: runtime_failure(1);
                        5: runtime_failure(6);
                        8: runtime_failure(5);
                        default: internal_failure();
                    endcase
                end else if (operation == 13) phase<=COMMIT_SEND;
                else if (operation == 19) begin
                    if (cut_data[1631:1600] == 32'hffffffff) runtime_failure(7);
                    else finish(8,{448'b0,cut_data[1663:1600]});
                end else finish(4,0);
            end
            COMMIT_SEND: if (session != epoch) internal_failure(); else if (record_ready) phase<=COMMIT_WAIT;
            COMMIT_WAIT: if (session != epoch) internal_failure(); else if (record_done) begin
                if (record_error != 0) internal_failure();else finish_record();
            end
            default: internal_failure();
        endcase
    end
`ifndef SYNTHESIS
    reg [1919:0] held_record;
    reg copying;
    always @(posedge clk) begin
        if (reset) begin copying<=0;held_record<=0;end
        else begin
            if (record_valid && record_ready) begin held_record<=record_data;copying<=1;end
            if (copying && record_data != held_record) $fatal(1,"Observation changed during retained copy");
            if (record_done) copying<=0;
        end
    end
`endif
endmodule
