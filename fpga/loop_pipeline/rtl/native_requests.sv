// Bounded native execution credits for the fixed one-loop resident roster.
module native_requests #(
    parameter integer REQUESTS=16
) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready,
    input wire [7:0] request_opcode, input wire [31:0] request_index, input wire [639:0] request_data,
    output reg response_valid, output reg [3:0] response_error, output reg [511:0] response_data,
    output wire tp_valid, input wire tp_ready, output wire [7:0] tp_opcode,
    output wire [31:0] tp_index, output wire [639:0] tp_data,
    input wire tp_done, input wire [3:0] tp_error, input wire [831:0] tp_result
);
    typedef enum reg [3:0] {IDLE,SCAN_SEND,SCAN_WAIT,INSTALL_SEND,INSTALL_WAIT,
        READ_SEND,READ_WAIT,DIRECT_SEND,DIRECT_WAIT} phase_t;
    phase_t phase;
    reg [7:0] opcode;
    reg [31:0] index, cursor;
    reg [639:0] payload;
    reg [63:0] native_id;
    reg pool_full;
    assign request_ready=phase == IDLE && !reset;
    assign tp_valid=phase == SCAN_SEND || phase == INSTALL_SEND || phase == READ_SEND || phase == DIRECT_SEND;
    assign tp_opcode=phase == SCAN_SEND || phase == READ_SEND ? 8'd13 : phase == INSTALL_SEND ? 8'd4 : opcode;
    assign tp_index=phase == SCAN_SEND || phase == INSTALL_SEND || phase == READ_SEND ? cursor : index;
    assign tp_data=phase == SCAN_SEND || phase == READ_SEND ? 640'b0 : payload;
    task automatic finish(input [3:0] error, input [511:0] data);
        begin response_valid<=1; response_error<=error; response_data<=error == 0 ? data : 512'b0; phase<=IDLE; end
    endtask
    initial if (REQUESTS < 1 || REQUESTS > 65536) $fatal(1,"Invalid resident request profile");
    always @(posedge clk) begin
        response_valid<=0;
        if (reset) begin
            phase<=IDLE; opcode<=0; index<=0; cursor<=0; payload<=0; native_id<=0; pool_full<=0;
            response_valid<=0; response_error<=0; response_data<=0;
        end else case (phase)
            IDLE: if (request_valid) begin
                opcode<=request_opcode; index<=request_index; payload<=request_data; cursor<=0;
                if (request_opcode == 4 && pool_full) finish(2,0);
                else phase<=request_opcode == 4 ? SCAN_SEND : DIRECT_SEND;
            end
            SCAN_SEND: if (tp_ready) phase<=SCAN_WAIT;
            SCAN_WAIT: if (tp_done) begin
                if (tp_error != 0) finish(tp_error,0);
                else if (!tp_result[0] && tp_result[32:1] != 0) begin
                    payload[32:1]<=tp_result[32:1]; payload[492:429]<=64'd1; phase<=INSTALL_SEND;
                end else if (cursor+1 == 32'(REQUESTS)) begin pool_full<=1; finish(2,0); end
                else begin cursor<=cursor+1'b1; phase<=SCAN_SEND; end
            end
            INSTALL_SEND: if (tp_ready) phase<=INSTALL_WAIT;
            INSTALL_WAIT: if (tp_done) begin
                if (tp_error != 0) finish(tp_error,0);
                else begin native_id<=tp_result[63:0]; phase<=READ_SEND; end
            end
            READ_SEND: if (tp_ready) phase<=READ_WAIT;
            READ_WAIT: if (tp_done) begin
                if (tp_error != 0) finish(tp_error,0);
                else if (!tp_result[0] || tp_result[32:1] != native_id[63:32]) finish(5,0);
                else finish(0,{51'b0,tp_result[396:0],native_id});
            end
            DIRECT_SEND: if (tp_ready) phase<=DIRECT_WAIT;
            DIRECT_WAIT: if (tp_done) begin
                if (opcode == 6 && tp_error == 0) pool_full<=0;
                finish(tp_error,tp_result[511:0]);
            end
            default: finish(5,0);
        endcase
    end
    wire unused=&{1'b0,tp_result[831:512]};
endmodule
