module audio_activation (
    input wire clk,reset,
    input wire request_valid, output wire request_ready,
    input wire request_initial, input wire [63:0] request_nonce, input wire [191:0] request_spec,
    output reg response_valid, output reg [7:0] response_error, output reg fault,
    output wire parameter_valid, input wire parameter_ready, output wire [7:0] parameter_opcode,
    output wire parameter_bank, output wire [63:0] parameter_nonce,
    input wire parameter_done, input wire [3:0] parameter_error,
    output wire image_valid, input wire image_ready, output wire [7:0] image_opcode,
    output wire image_state, output wire [639:0] image_data,
    input wire image_done, input wire [3:0] image_error,
    output wire tp_valid, input wire tp_ready, output wire [7:0] tp_opcode,
    output wire [639:0] tp_data,
    input wire tp_done, input wire [3:0] tp_error, input wire [831:0] tp_result
);
    typedef enum reg [3:0] {IDLE,IMAGE_CHECK_SEND,IMAGE_CHECK_WAIT,MAP_CHECK_SEND,MAP_CHECK_WAIT,
        TP_CHECK_SEND,TP_CHECK_WAIT,SNAPSHOT_SEND,SNAPSHOT_WAIT,PREPARE_SEND,PREPARE_WAIT,
        ACTIVATE_SEND,ACTIVATE_WAIT,MAP_SEND,MAP_WAIT,TP_INITIAL} phase_t;
    phase_t phase;
    reg initial_action,initial_sent;
    reg [63:0] nonce;
    reg [191:0] specification;
    assign request_ready=phase == IDLE && !reset && !fault;
    assign parameter_valid=phase == MAP_CHECK_SEND || phase == SNAPSHOT_SEND || phase == PREPARE_SEND || phase == MAP_SEND;
    assign parameter_opcode=phase == MAP_CHECK_SEND ? 8'd8 : phase == SNAPSHOT_SEND ? 8'd4 : phase == PREPARE_SEND ? 8'd5 : 8'd6;
    assign parameter_bank=nonce[0];
    assign parameter_nonce=nonce;
    assign image_valid=phase == IMAGE_CHECK_SEND || phase == ACTIVATE_SEND;
    assign image_opcode=phase == IMAGE_CHECK_SEND ? 8'd12 : 8'd5;
    assign image_state=phase == ACTIVATE_SEND;
    assign image_data=phase == IMAGE_CHECK_SEND ? {447'b0,nonce,129'b0} : {576'b0,nonce};
    assign tp_valid=phase == TP_CHECK_SEND || (phase == TP_INITIAL && !initial_sent);
    assign tp_opcode=phase == TP_CHECK_SEND ? 8'd12 : 8'd22;
    assign tp_data=phase == TP_CHECK_SEND ? 640'b0 : {448'b0,specification};
    wire valid_spec=specification[0] && specification[32:1] != 0 && specification[101:99] == 0
        && specification[191:128] == 0 && (!specification[33] || specification[97:34] == {specification[32:1],32'b0});
    wire unused=&{1'b0,tp_result[831:1],specification[127:102]};
    function automatic [7:0] public_error(input [3:0] error);
        case (error) 3:public_error=1;8:public_error=5;default:public_error=6;endcase
    endfunction
    task automatic reply(input [7:0] error);
        begin response_valid<=1;response_error<=error;phase<=IDLE;end
    endtask
    task automatic fail;
        begin fault<=1;reply(6);end
    endtask
    always @(posedge clk) begin
        response_valid<=0;
        if (reset) begin phase<=IDLE;initial_action<=0;initial_sent<=0;nonce<=0;specification<=0;response_valid<=0;response_error<=0;fault<=0;end
        else if (!fault) case (phase)
            IDLE: if (request_valid) begin
                initial_action<=request_initial;initial_sent<=0;nonce<=request_nonce;specification<=request_spec;phase<=IMAGE_CHECK_SEND;
            end
            IMAGE_CHECK_SEND: if (image_ready) phase<=IMAGE_CHECK_WAIT;
            IMAGE_CHECK_WAIT: if (image_done) begin
                if (image_error != 0) reply(public_error(image_error));else phase<=MAP_CHECK_SEND;
            end
            MAP_CHECK_SEND: if (parameter_ready) phase<=MAP_CHECK_WAIT;
            MAP_CHECK_WAIT: if (parameter_done) begin
                if (parameter_error != 0) reply(public_error(parameter_error));
                else if (initial_action) begin if (!valid_spec) reply(2);else phase<=TP_CHECK_SEND;end
                else phase<=SNAPSHOT_SEND;
            end
            TP_CHECK_SEND: if (tp_ready) phase<=TP_CHECK_WAIT;
            TP_CHECK_WAIT: if (tp_done) begin
                if (tp_error != 0) fail();else if (tp_result[0]) reply(4);else phase<=PREPARE_SEND;
            end
            SNAPSHOT_SEND: if (parameter_ready) phase<=SNAPSHOT_WAIT;
            SNAPSHOT_WAIT: if (parameter_done) begin if (parameter_error != 0) fail();else phase<=PREPARE_SEND;end
            PREPARE_SEND: if (parameter_ready) phase<=PREPARE_WAIT;
            PREPARE_WAIT: if (parameter_done) begin if (parameter_error != 0) fail();else phase<=ACTIVATE_SEND;end
            ACTIVATE_SEND: if (image_ready) phase<=ACTIVATE_WAIT;
            ACTIVATE_WAIT: if (image_done) begin if (image_error != 0) fail();else phase<=MAP_SEND;end
            MAP_SEND: if (parameter_ready) phase<=MAP_WAIT;
            MAP_WAIT: if (parameter_done) begin
                if (parameter_error != 0) fail();else if (initial_action) phase<=TP_INITIAL;else reply(0);
            end
            TP_INITIAL: begin
                if (tp_ready && !initial_sent) initial_sent<=1;
                if (initial_sent && tp_done) begin if (tp_error != 0) fail();else reply(0);end
            end
            default: fail();
        endcase
    end
endmodule
