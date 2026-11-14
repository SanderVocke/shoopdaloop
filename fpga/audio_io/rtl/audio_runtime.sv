module audio_runtime #(
    parameter integer NODES=128
) (
    input wire clk,reset,loaded,input wire [63:0] graph_epoch,
    input wire configuration_valid,output wire configuration_ready,input wire [639:0] configuration_data,
    output reg configuration_done,output reg [3:0] configuration_error,output reg [128:0] configuration_result,
    input wire request_valid,output wire request_ready,input wire [511:0] request_data,
    output reg response_valid,output reg [511:0] response_data,
    input wire observe_valid,output wire observe_ready,input wire [831:0] observe_data,input wire pre_recording,
    output reg observe_done,
    input wire activation_valid,output wire activation_ready,input wire activation_initial,activation_stereo,
    input wire [63:0] activation_epoch,output reg activation_done,
    output wire parameter_valid,input wire parameter_ready,output wire [511:0] parameter_data,
    input wire parameter_done,input wire [3:0] parameter_error,input wire [128:0] parameter_result,
    output reg [31:0] loop_gain,loop_balance,output wire busy,output reg fault
);
    localparam integer A=NODES > 1 ? $clog2(NODES) : 1;
    localparam [31:0] ONE=32'd16777216;
    localparam [3:0] PLAYBACK=0,CAPTURE=1,STRIP_INPUT=2,STRIP_OUTPUT=3,FADER=4,BUS_INPUT=5,BUS=6,READER=7;
    typedef enum reg [3:0] {IDLE,READ_ROW,READ_WAIT,READ_GET,EVALUATE,
        PARAMETER_SEND,PARAMETER_WAIT,CROSSFADE,MONITOR_TARGET,WRITE_ROW,WRITE_ACK,INITIAL_PARAMETER,INITIAL_NEXT} phase_t;
    phase_t phase,parameter_return;
    reg [1:0] purpose;
    reg [31:0] cursor,node_count;
    reg [511:0] payload,row;
    reg [639:0] configuration;
    reg [7:0] operation;
    reg [NODES-1:0] initialized;
    reg [3:0] field;
    reg recording,preparing_recording,initial_activation,stereo_activation;
    reg [63:0] next_epoch;
    reg [7:0] p_opcode;
    reg [15:0] p_key;
    reg [128:0] p_value;
    reg [31:0] p_target,p_duration;
    assign busy=phase != IDLE;
    assign request_ready=phase == IDLE && !configuration_valid && !observe_valid && !activation_valid && !fault && !reset;
    assign configuration_ready=phase == IDLE && !observe_valid && !activation_valid && !fault && !reset;
    assign observe_ready=phase == IDLE && !activation_valid && !fault && !reset;
    assign activation_ready=phase == IDLE && !fault && !reset;
    wire [511:0] read_data;
    sp_ram #(.WIDTH(512),.DEPTH(NODES),.ADDR_BITS(A)) nodes (
        .clk(clk),.read_enable(phase == READ_ROW),.read_address(A'(cursor)),.read_data(read_data),
        .write_enable(phase == WRITE_ROW),.write_address(A'(cursor)),.write_data(row)
    );
    wire [3:0] kind=row[35:32];
    wire configurable=kind == STRIP_INPUT || kind == STRIP_OUTPUT || kind == FADER || kind == BUS;
    wire gain_node=kind == STRIP_INPUT || kind == STRIP_OUTPUT || kind == BUS_INPUT || kind == BUS;
    wire logical_node=kind == PLAYBACK || kind == CAPTURE;
    wire [15:0] key_base=16'(cursor*7);
    wire monitor_enabled=row[108] || (row[141] && row[173:142] == row[140:109]
        && row[140:109] != 0 && (recording || preparing_recording));
    assign parameter_valid=phase == PARAMETER_SEND;
    assign parameter_data={175'b0,p_duration,p_target,p_value,16'b0,32'b0,64'b0,7'b0,1'b0,p_key,p_opcode};
    wire unused=&{1'b0,observe_data[831:35],observe_data[32:0],configuration_data[631:337],configuration_data[31:0],
        configuration[631:97],configuration[31:0],parameter_result[128:64],parameter_result[31:0],payload[511:504],
        payload[487:402],payload[71:64],payload[31:0],row[511:506]};
    task automatic done;
        begin
            phase<=IDLE;
            case (purpose)
                0:begin response_valid<=1;response_data<=0;response_data[511:504]<=4;end
                1:configuration_done<=1;
                2:observe_done<=1;
                3:activation_done<=1;
            endcase
        end
    endtask
    task automatic refuse(input [7:0] error);
        begin response_valid<=1;response_data<={8'd1,496'b0,error};phase<=IDLE;end
    endtask
    task automatic configure_failure(input [3:0] error);
        begin configuration_done<=1;configuration_error<=error;configuration_result<=0;phase<=IDLE;end
    endtask
    task automatic fail;
        begin fault<=1;done();end
    endtask
    task automatic advance;
        begin if (cursor+1 == node_count) done();else begin cursor<=cursor+1'b1;phase<=READ_ROW;end end
    endtask
    task automatic send_parameter(input [7:0] op,input [15:0] key,input [31:0] target,input [31:0] duration,input [128:0] value,input phase_t destination);
        begin p_opcode<=op;p_key<=key;p_target<=target;p_duration<=duration;p_value<=value;parameter_return<=destination;phase<=PARAMETER_SEND;end
    endtask
    initial if (NODES < 1 || NODES > 9362) $fatal(1,"Invalid runtime node capacity");
    always @(posedge clk) begin : runtime_owner
        reg [31:0] initial_target;
        response_valid<=0;configuration_done<=0;observe_done<=0;activation_done<=0;
        if (reset) begin
            phase<=IDLE;parameter_return<=IDLE;purpose<=0;cursor<=0;node_count<=0;payload<=0;row<=0;configuration<=0;operation<=0;
            initialized<=0;field<=0;recording<=0;preparing_recording<=0;initial_activation<=0;stereo_activation<=0;next_epoch<=0;
            p_opcode<=0;p_key<=0;p_value<=0;p_target<=0;p_duration<=0;
            response_valid<=0;response_data<=0;configuration_done<=0;configuration_error<=0;configuration_result<=0;
            observe_done<=0;activation_done<=0;loop_gain<=32'h3f800000;loop_balance<=0;fault<=0;
        end else if (!fault) case (phase)
            IDLE: if (activation_valid) begin
                purpose<=3;cursor<=0;initial_activation<=activation_initial;stereo_activation<=activation_stereo;
                next_epoch<=activation_epoch;if (node_count == 0) activation_done<=1;else phase<=READ_ROW;
            end else if (observe_valid) begin
                purpose<=2;cursor<=0;recording<=observe_data[34:33] == 2;preparing_recording<=pre_recording;
                if (!observe_data[0] || node_count == 0) begin observe_done<=1;end else phase<=READ_ROW;
            end else if (configuration_valid) begin
                purpose<=1;configuration<=configuration_data;cursor<=configuration_data[31:0];configuration_error<=0;configuration_result<=0;
                if (configuration_data[31:0] >= 32'(NODES) || configuration_data[63:32] == 0) configure_failure(3);
                else if (configuration_data[639:632] == 0) begin
                    if (loaded || initialized[A'(configuration_data[31:0])]) configure_failure(5);
                    else if (configuration_data[67:64] > READER || configuration_data[75:68] > 2) configure_failure(1);
                    else begin
                        row<=0;row[31:0]<=configuration_data[63:32];row[35:32]<=configuration_data[67:64];
                        row[43:36]<=configuration_data[75:68];row[75:44]<=configuration_data[107:76];row[107:76]<=configuration_data[139:108];
                        initialized[A'(configuration_data[31:0])]<=1;phase<=WRITE_ROW;
                        if (configuration_data[31:0] >= node_count) node_count<=configuration_data[31:0]+1'b1;
                    end
                end else if (configuration_data[639:632] == 1 || configuration_data[639:632] == 2) phase<=READ_ROW;
                else configure_failure(10);
            end else if (request_valid) begin
                purpose<=0;payload<=request_data;operation<=request_data[71:64];cursor<=request_data[31:0];
                if (request_data[71:64] == 6 || request_data[71:64] == 7) begin
                    cursor<=0;
                    if (request_data[503:496] != 0) refuse(request_data[503:496]);
                    else begin
                        if (request_data[71:64] == 6) loop_gain<=request_data[103:72];else loop_balance<=request_data[103:72];
                        if (node_count == 0) begin response_valid<=1;response_data<=0;response_data[511:504]<=4;end else phase<=READ_ROW;
                    end
                end else if (request_data[71:64] == 8) begin
                    send_parameter(9,0,request_data[103:72],0,{128'b0,request_data[104]},IDLE);
                end else if (request_data[31:0] >= 32'(NODES) || request_data[63:32] == 0) refuse(1);
                else phase<=READ_ROW;
            end
            READ_ROW: phase<=READ_WAIT;
            READ_WAIT: phase<=READ_GET;
            READ_GET: if (!initialized[A'(cursor)]) begin
                if (purpose >= 2 || (purpose == 0 && (operation == 6 || operation == 7))) advance();
                else if (purpose == 1) configure_failure(3);else refuse(1);
            end else begin row<=read_data;phase<=EVALUATE;end
            EVALUATE: if (purpose == 1) begin
                if (row[31:0] != configuration[63:32]) configure_failure(3);
                else if (configuration[639:632] == 2) begin
                    configuration_result<={17'b0,row[175:174],row[173:142],row[141],row[140:109],row[108],row[75:44],row[43:36],row[35:32]};done();
                end else if (kind != STRIP_INPUT) configure_failure(1);
                else begin row[173:142]<=configuration[95:64];row[141]<=configuration[96];phase<=WRITE_ROW;end
            end else if (purpose == 2) begin
                if (kind != STRIP_INPUT) advance();else send_parameter(7,key_base+5,0,0,0,MONITOR_TARGET);
            end else if (purpose == 3) begin
                if (initial_activation && (row[175] || kind == PLAYBACK)) begin field<=row[175] ? 0 : 6;phase<=INITIAL_PARAMETER;end
                else if (row[174] && row[335:272] == next_epoch) begin row[174]<=0;phase<=WRITE_ROW;end
                else advance();
            end else if (operation == 6 || operation == 7) begin
                if (kind != PLAYBACK) advance();else send_parameter(3,key_base+(operation == 6 ? 16'd0 : 16'd1),payload[135:104],row[75:44],0,READ_GET);
            end else if (row[31:0] != payload[63:32]) refuse(1);
            else case (operation)
                0,1: if (payload[503:496] != 0) refuse(payload[503:496]);
                    else if (logical_node) refuse(12);else if (!gain_node) refuse(2);
                    else send_parameter(3,key_base+16'(operation),payload[135:104],row[75:44],0,IDLE);
                2,3: if (logical_node) refuse(12);
                    else if (!(kind == STRIP_OUTPUT || kind == BUS_INPUT || kind == BUS || (operation == 3 && kind == FADER))) refuse(2);
                    else send_parameter(3,key_base+16'(operation),(operation == 2 ? !payload[72] : payload[72]) ? ONE : 32'b0,row[75:44],0,IDLE);
                4: if (logical_node) refuse(12);else if (!gain_node) refuse(2);else send_parameter(7,key_base+4,0,0,0,CROSSFADE);
                5: if (logical_node) refuse(12);else if (kind != STRIP_INPUT) refuse(2);else begin row[108]<=payload[72];phase<=WRITE_ROW;end
                9: if (payload[135:72] != graph_epoch && &graph_epoch) refuse(5);
                    else if (payload[135:72] != graph_epoch && payload[135:72] != graph_epoch+1'b1) refuse(10);
                    else if (kind != STRIP_INPUT) refuse(2);else done();
                10: if (!configurable) refuse(2);else if (row[174] && row[271:208] != payload[135:72]) refuse(1);
                    else begin row[174]<=0;phase<=WRITE_ROW;end
                11: if (loaded) refuse(4);
                    else if (payload[503:496] != 0) refuse(payload[503:496]);
                    else if (&graph_epoch) refuse(5);
                    else if (payload[231:168] != graph_epoch+1'b1) refuse(10);
                    else if (payload[495:488] != 0) refuse(payload[495:488]);
                    else if ((kind == STRIP_INPUT && payload[239:232] > row[43:36])
                        || ((kind == STRIP_OUTPUT || kind == BUS) && payload[247:240] > row[43:36])) refuse(7);
                    else if (!configurable) refuse(2);
                    else if (row[174] && row[271:208] != payload[167:104]) refuse(4);
                    else begin row[505:176]<=payload[401:72];row[175:174]<=3;phase<=WRITE_ROW;end
                12,13: if (logical_node) refuse(12);else if (kind != READER) refuse(2);else done();
                default: refuse(2);
            endcase
            PARAMETER_SEND: if (parameter_ready) phase<=PARAMETER_WAIT;
            PARAMETER_WAIT: if (parameter_done) begin
                if (parameter_error != 0) fail();
                else if (parameter_return == IDLE) done();
                else if (parameter_return == READ_GET) advance();
                else phase<=parameter_return;
            end
            CROSSFADE: send_parameter(3,key_base+4,ONE-parameter_result[63:32],row[75:44],0,IDLE);
            MONITOR_TARGET: if (parameter_result[63:32] == (monitor_enabled ? ONE : 32'b0)) advance();
                else send_parameter(3,key_base+5,monitor_enabled ? ONE : 32'b0,row[75:44],0,READ_GET);
            WRITE_ROW: phase<=WRITE_ACK;
            WRITE_ACK: if (purpose >= 2) advance();else done();
            INITIAL_PARAMETER: begin
                case (field)
                    0:initial_target=row[473:442];
                    1:initial_target=row[505:474];
                    2:initial_target=row[441] ? 32'b0 : ONE;
                    3:initial_target=ONE;
                    4:initial_target=row[352] ? ONE : 32'b0;
                    default:initial_target=(kind == PLAYBACK ? stereo_activation : kind == STRIP_INPUT ? row[343:336] == 2
                        : (kind == STRIP_OUTPUT || kind == BUS) && row[351:344] == 2) ? ONE : 32'b0;
                endcase
                send_parameter(11,key_base+16'(field),0,0,{1'b1,64'b0,initial_target,initial_target},INITIAL_NEXT);
            end
            INITIAL_NEXT: if (field < 6) begin field<=field == 4 ? 4'd6 : field+1'b1;phase<=INITIAL_PARAMETER;end
                else begin
                    if (row[175]) begin row[108]<=row[440];row[140:109]<=row[207:176];row[175]<=0;end
                    if (row[174] && row[335:272] == next_epoch) row[174]<=0;
                    phase<=WRITE_ROW;
                end
            default: fail();
        endcase
    end
endmodule
