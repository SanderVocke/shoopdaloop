// Public delivery uses native TP execution resources only at its actual cut.
module audio_native #(
    parameter integer REQUESTS=16
) (
    input wire clk, input wire reset,
    input wire request_valid, output wire request_ready, input wire [7:0] request_opcode,
    input wire [15:0] request_flags,
    input wire [1:0] request_mode, input wire [511:0] request_data, input wire [63:0] request_sequence,
    input wire [63:0] request_frame, input wire [47:0] request_reference,
    output reg response_valid, output reg response_pending, output reg [3:0] response_error,
    output reg [511:0] response_data, output reg fault,
    output wire tp_valid, input wire tp_ready, output wire [7:0] tp_opcode,
    output wire [31:0] tp_index, output wire [639:0] tp_data,
    input wire tp_done, input wire [3:0] tp_error, input wire [831:0] tp_result,
    output wire lookup_valid, input wire lookup_ready, output wire [15:0] lookup_slot,
    input wire lookup_done, input wire [3:0] lookup_error, input wire [767:0] lookup_data,
    output wire update_valid, input wire update_ready, output wire [15:0] update_slot,
    output wire [31:0] update_generation, output wire [767:0] update_data,
    input wire update_done, input wire [3:0] update_error
);
    localparam [7:0] RUNTIME=1,TIMELINE=2,QUEUED_CANCELLED=3,CANCEL_RESULT=9;
    typedef enum reg [4:0] {IDLE,CHECK_TARGET,PREPARE_INSTALL,INSTALL_RESULT,RESOLVE_RESULT,RELEASE_RESULT,
        LOOKUP_SEND,LOOKUP_WAIT,LOOKUP_CHECK,CANCEL_RESULT_WAIT,UPDATE_SEND,UPDATE_WAIT,
        NATIVE_SEND,NATIVE_WAIT} phase_t;
    phase_t phase, return_phase;
    reg [7:0] opcode,native_opcode;
    reg [1:0] mode;
    reg [15:0] flags;
    reg [511:0] payload,native_result;
    reg [63:0] sequence_number,frame;
    reg [47:0] reference;
    reg [3:0] native_error;
    reg [31:0] native_index;
    reg [639:0] native_data;
    reg [767:0] target_row,mutation;
    reg cancelled_delivered;
    wire native_ready,native_done;
    wire [3:0] native_response_error;
    wire [511:0] native_response_data;
    native_requests #(.REQUESTS(REQUESTS)) credits (
        .clk(clk),.reset(reset),.request_valid(phase == NATIVE_SEND),.request_ready(native_ready),
        .request_opcode(native_opcode),.request_index(native_index),.request_data(native_data),
        .response_valid(native_done),.response_error(native_response_error),.response_data(native_response_data),
        .tp_valid(tp_valid),.tp_ready(tp_ready),.tp_opcode(tp_opcode),.tp_index(tp_index),.tp_data(tp_data),
        .tp_done(tp_done),.tp_error(tp_error),.tp_result(tp_result)
    );
    assign request_ready=phase == IDLE && !fault && !reset;
    assign lookup_valid=phase == LOOKUP_SEND;
    assign lookup_slot=reference[15:0];
    assign update_valid=phase == UPDATE_SEND;
    assign update_slot=reference[15:0];
    assign update_generation=reference[47:16];
    assign update_data=mutation;
    task automatic finish(input [7:0] tag, input [511:0] data, input pending);
        begin response_valid<=1; response_pending<=pending; response_data<=data; response_data[511:504]<=tag; phase<=IDLE; end
    endtask
    task automatic fail(input [7:0] tag, input [7:0] code);
        begin finish(tag,{504'b0,code},0); end
    endtask
    task automatic internal_failure;
        begin fault<=1; response_error<=5; finish(0,0,0); end
    endtask
    task automatic send(input [7:0] operation, input [31:0] index, input [639:0] data, input phase_t destination);
        begin native_opcode<=operation;native_index<=index;native_data<=data;return_phase<=destination;phase<=NATIVE_SEND;end
    endtask
    task automatic mutate;
        begin
            mutation<=target_row; mutation[186]<=1; mutation[254:191]<=frame;
            mutation[766:255]<=0; mutation[766:759]<=QUEUED_CANCELLED;
            phase<=UPDATE_SEND;
        end
    endtask
    wire [63:0] target_id=opcode == 9 ? payload[108:45] : payload[63:0];
    wire unused=&{1'b0,mode,flags};
    always @(posedge clk) begin : delivery
        reg [639:0] install;
        response_valid<=0;
        if (reset) begin
            phase<=IDLE;return_phase<=IDLE;opcode<=0;mode<=0;flags<=0;payload<=0;sequence_number<=0;frame<=0;reference<=0;
            native_opcode<=0;native_index<=0;native_data<=0;native_result<=0;native_error<=0;
            target_row<=0;mutation<=0;cancelled_delivered<=0;fault<=0;response_valid<=0;response_error<=0;response_data<=0;response_pending<=0;
        end else if (!fault) case (phase)
            IDLE: if (request_valid) begin
                opcode<=request_opcode;mode<=request_mode;flags<=request_flags;payload<=request_data;sequence_number<=request_sequence;
                frame<=request_frame;reference<=request_reference;response_error<=0;
                if ((request_opcode == 9 || request_opcode == 10) && request_mode != 0) begin
                    if (request_data[63:32] == 0 || request_data[31:0] >= 32'(REQUESTS)) internal_failure();
                    else send(request_mode == 1 ? 8'd13 : 8'd6,request_data[31:0],{608'b0,request_data[63:32]},
                        request_mode == 1 ? RESOLVE_RESULT : RELEASE_RESULT);
                end else if (request_opcode == 9 || request_opcode == 10) send(12,0,0,CHECK_TARGET);
                else if (request_opcode == 11 && request_mode == 0) begin
                    if (request_data[63:0] == request_sequence) fail(RUNTIME,request_flags[2] ? 8'd1 : 8'd6);
                    else if (request_reference[47:16] == 0) fail(RUNTIME,1); else phase<=LOOKUP_SEND;
                end else if ((request_opcode == 12 || request_opcode == 13) && request_mode == 0) finish(0,request_data,1);
                else fail(RUNTIME,2);
            end
            NATIVE_SEND: if (native_ready) phase<=NATIVE_WAIT;
            NATIVE_WAIT: if (native_done) begin native_result<=native_response_data;native_error<=native_response_error;phase<=return_phase;end
            CHECK_TARGET: begin
                if (native_error != 0) internal_failure();
                else if (target_id[31:0] != 0 || !native_result[0] || native_result[32:1] != target_id[63:32]) fail(TIMELINE,3);
                else if (payload[503:496] != 0) fail(RUNTIME,payload[503:496] <= 21 ? payload[503:496] : 8'd2);
                else if (opcode == 10 && &native_result[511:448]) fail(RUNTIME,5);
                else phase<=PREPARE_INSTALL;
            end
            PREPARE_INSTALL: begin
                install=0;
                if (opcode == 9) install[511:0]=payload;
                else begin
                    install[35:34]=3;install[44]=1;install[108:45]=payload[63:0];
                    install[172:109]=sequence_number;install[236:173]=payload[127:64];install[428:365]=native_result[511:448]+1'b1;
                end
                send(4,0,install,INSTALL_RESULT);
            end
            INSTALL_RESULT: if (native_error != 0) fail(TIMELINE,{4'b0,native_error}); else finish(0,native_result,1);
            RESOLVE_RESULT: begin
                if (native_error != 0 || !native_result[0] || native_result[32:1] != payload[63:32]) internal_failure();
                else finish(0,{51'b0,native_result[396:0],payload[63:0]},native_result[39:37] == 0);
            end
            RELEASE_RESULT: begin
                if (native_error != 0 || native_result[396:0] != payload[460:64]) internal_failure();
                else finish(0,0,0);
            end
            LOOKUP_SEND: if (lookup_ready) phase<=LOOKUP_WAIT;
            LOOKUP_WAIT: if (lookup_done) begin
                if (lookup_error != 0) fail(RUNTIME,1);
                else begin target_row<=lookup_data;phase<=LOOKUP_CHECK;end
            end
            LOOKUP_CHECK: begin
                if (!target_row[0] || !target_row[767] || target_row[32:1] != reference[47:16]
                    || target_row[160:97] != payload[63:0] || target_row[171:170] != 0) fail(RUNTIME,1);
                else if (target_row[186]) fail(RUNTIME,target_row[172] ? 8'd1 : 8'd6);
                else if (target_row[169] && target_row[172]) fail(RUNTIME,1);
                else if (target_row[169]) begin cancelled_delivered<=0;phase<=CANCEL_RESULT_WAIT;end
                else if (target_row[168:161] != 9 && target_row[168:161] != 10) fail(RUNTIME,6);
                else begin
                    cancelled_delivered<=1;
                    send(5,target_row[286:255],{608'b0,target_row[318:287]},CANCEL_RESULT_WAIT);
                end
            end
            CANCEL_RESULT_WAIT: begin
                if (!cancelled_delivered) mutate();
                else if (native_error != 0) internal_failure();
                else finish(CANCEL_RESULT,{447'b0,1'b1,payload[63:0]},0);
            end
            UPDATE_SEND: if (update_ready) phase<=UPDATE_WAIT;
            UPDATE_WAIT: if (update_done) begin
                if (update_error != 0) internal_failure();else finish(CANCEL_RESULT,{447'b0,cancelled_delivered,payload[63:0]},0);
            end
            default: internal_failure();
        endcase
    end
endmodule
