module audio_monitor #(
    parameter integer REQUESTS=16
) (
    input wire clk,reset,
    input wire start,output wire ready,input wire [831:0] cut_data,output reg done,output reg fault,
    output wire tp_valid,input wire tp_ready,output wire [7:0] tp_opcode,output wire [31:0] tp_index,output wire [639:0] tp_data,
    input wire tp_done,input wire [3:0] tp_error,input wire [831:0] tp_result,
    output wire observe_valid,input wire observe_ready,output wire [831:0] observe_data,output wire pre_recording,
    input wire observe_done
);
    typedef enum reg [2:0] {IDLE,READ_SEND,READ_WAIT,OBSERVE_SEND,OBSERVE_WAIT} phase_t;
    phase_t phase;
    reg [31:0] cursor;
    reg pre;
    assign ready=phase == IDLE && !reset && !fault;
    assign tp_valid=phase == READ_SEND;
    assign tp_opcode=13;
    assign tp_index=cursor;
    assign tp_data=0;
    assign observe_valid=phase == OBSERVE_SEND;
    assign observe_data=cut_data;
    assign pre_recording=pre;
    wire unused=&{1'b0,tp_result[831:333],tp_result[268:45],tp_result[43:40],tp_result[36],tp_result[32:1]};
    initial if (REQUESTS < 1 || REQUESTS > 65536) $fatal(1,"Invalid monitor request capacity");
    always @(posedge clk) begin
        done<=0;
        if (reset) begin phase<=IDLE;cursor<=0;pre<=0;done<=0;fault<=0;end
        else if (!fault) case (phase)
            IDLE: if (start) begin cursor<=0;pre<=0;phase<=cut_data[0] ? READ_SEND : OBSERVE_SEND;end
            READ_SEND: if (tp_ready) phase<=READ_WAIT;
            READ_WAIT: if (tp_done) begin
                if (tp_error != 0) begin fault<=1;done<=1;phase<=IDLE;end
                else begin
                    if (tp_result[0] && tp_result[33] && tp_result[35:34] == 2 && tp_result[39:37] == 0
                        && !tp_result[44] && tp_result[332:269] == 1) pre<=1;
                    if (cursor+1 == 32'(REQUESTS)) phase<=OBSERVE_SEND;
                    else begin cursor<=cursor+1'b1;phase<=READ_SEND;end
                end
            end
            OBSERVE_SEND: if (observe_ready) phase<=OBSERVE_WAIT;
            OBSERVE_WAIT: if (observe_done) begin done<=1;phase<=IDLE;end
            default: begin fault<=1;done<=1;phase<=IDLE;end
        endcase
    end
endmodule
