module fifo_config_service (
    input wire clk, live, halt, input wire [31:0] epoch,
    input wire valid, input wire [1:0] operation, input wire [3:0] index,
    input wire [31:0] word,
    output wire [3:0] receipt,
    output wire request_space, response_pending,
    output wire [31:0] response_tag, output wire [1:0] response_code,
    input wire [3:0] read_index, output reg [31:0] read_data,
    output wire config_valid, input wire config_ready,
    output reg [1:0] config_operation, output reg [2:0] config_space,
    output reg config_write, output reg [31:0] config_address,
    output wire [159:0] config_data,
    input wire config_response_valid, config_response_error,
    input wire [159:0] config_response_data
);
    localparam [2:0] FILL=0, HEADER=1, RESERVED=2, DATA=3, ISSUE=4, WAIT=5, REPLY=6;
    localparam [3:0] ACCEPTED=0, FULL=1, EPOCH=2, FAULT=3, INVALID=4, UNAVAILABLE=5, OLD=6, INCOMPLETE=7;
    reg [2:0] phase;
    reg [13:0] written;
    reg [31:0] tag;
    reg [1:0] code;
    (* ram_style="distributed" *) reg [31:0] field0 [0:3];
    (* ram_style="distributed" *) reg [31:0] field1 [0:3];
    (* ram_style="distributed" *) reg [31:0] field2 [0:3];
    (* ram_style="distributed" *) reg [31:0] field3 [0:3];
    (* ram_style="distributed" *) reg [31:0] field4 [0:3];
    wire [1:0] read_row=phase==FILL || phase==HEADER ? 2'd1 : phase==RESERVED ? 2'd2 : 2'd0;
    wire [31:0] value0=field0[read_row], value1=field1[read_row], value2=field2[read_row];
    wire [31:0] value3=field3[read_row], value4=field4[read_row];
    assign config_data={value4,value3,value2,value1,value0};
    wire known_operation=value3>=1 && value3<=3;
    wire bad_header=(value0 & (value3==3 ? 32'hfffffef8 : 32'hfffffefc))!=0
        || (value3==1 && value1[31:16]!=0);
    wire bad_data=(config_operation==1 && value4[31:1]!=0)
        || (config_operation==2 && (value1!=0 || value2!=0 || value3!=0 || value4!=0));
    wire bad_reserved=value0!=0 || value1!=0 || value2!=0 || value3!=0 || value4!=0;
    wire complete=(phase==WAIT || (phase==ISSUE && config_ready)) && config_response_valid;
    wire abandon=halt && phase!=FILL && phase!=REPLY;
    wire refuse=(phase==HEADER && (!known_operation || bad_header))
        || (phase==RESERVED && bad_reserved) || (phase==DATA && bad_data);
    wire [159:0] copied_data=config_operation==1 ? {31'd0,config_response_data[128:0]}
        : config_operation==2 ? {128'd0,config_response_data[31:0]} : config_response_data;
    assign request_space=live && !halt && phase==FILL;
    assign response_pending=live && phase==REPLY;
    assign response_tag=response_pending ? tag : 32'd0;
    assign response_code=response_pending ? code : 2'd0;
    assign config_valid=live && !halt && phase==ISSUE;
    assign receipt=!live ? EPOCH : halt ? FAULT
        : operation==2 ? (phase!=REPLY ? UNAVAILABLE : word!=tag ? OLD : ACCEPTED)
        : operation>2 ? INVALID : phase!=FILL ? FULL
        : operation==0 ? (index>=14 ? INVALID : index!=0 && !written[0] ? INCOMPLETE : ACCEPTED)
        : word==0 ? INVALID : !(&written) ? INCOMPLETE : value2!=epoch ? EPOCH : ACCEPTED;
    wire accept=valid && receipt==ACCEPTED;
    wire stage=accept && operation==0;
    reg [2:0] stage_field;
    reg [1:0] stage_row;
    always @* begin
        stage_field=0; stage_row=0;
        case (index)
            0: begin stage_field=2; stage_row=1; end
            1: begin stage_field=3; stage_row=1; end
            2: begin stage_field=0; stage_row=1; end
            3: begin stage_field=1; stage_row=1; end
            4,5,6,7,8: stage_field=3'(index-4'd4);
            9,10,11,12,13: begin stage_field=3'(index-4'd9); stage_row=2; end
            default: ;
        endcase
        read_data=0;
        if (response_pending) case (read_index)
            0: read_data=value0;
            1: read_data=value1;
            2: read_data=value2;
            3: read_data=value3;
            4: read_data=value4;
            default: ;
        endcase
    end
    wire result_write=live && (complete || refuse || abandon);
    wire zero_result=refuse || abandon || config_response_error;
    wire [1:0] write_row=result_write ? 2'd0 : stage_row;
    wire [159:0] write_data=result_write ? (zero_result ? 160'd0 : copied_data) : {5{word}};
    wire [4:0] write_enable=result_write ? 5'b11111 : stage ? (5'd1<<stage_field) : 5'd0;
    always @(posedge clk) begin
        if (write_enable[0]) field0[write_row]<=write_data[31:0];
        if (write_enable[1]) field1[write_row]<=write_data[63:32];
        if (write_enable[2]) field2[write_row]<=write_data[95:64];
        if (write_enable[3]) field3[write_row]<=write_data[127:96];
        if (write_enable[4]) field4[write_row]<=write_data[159:128];
        if (!live) begin
            phase<=FILL; written<=0; tag<=0; code<=0;
            config_operation<=0; config_space<=0; config_write<=0; config_address<=0;
        end else begin
            if (accept) case (operation)
                0: if (index==0) written<=14'd1; else written[index]<=1;
                1: begin tag<=word; phase<=HEADER; end
                2: begin phase<=FILL; written<=0; end
                default: ;
            endcase
            case (phase)
                HEADER: begin
                    config_operation<=value3[1:0]; config_space<=value0[2:0];
                    config_write<=value0[8]; config_address<=value1;
                    phase<=RESERVED;
                end
                RESERVED: phase<=DATA;
                DATA: phase<=ISSUE;
                ISSUE: if (config_ready) phase<=WAIT;
                default: ;
            endcase
            if (complete || refuse || abandon) begin
                phase<=REPLY;
                code<=abandon || (complete && config_response_error) ? 2'd3
                    : refuse ? (phase==HEADER && !known_operation ? 2'd1 : 2'd2) : 2'd0;
            end
        end
    end
endmodule
