module fifo_config_service (
    input wire clk, live, halt, input wire [31:0] epoch,
    input wire valid, input wire [1:0] operation, input wire [3:0] index,
    input wire [31:0] word,
    output wire [3:0] receipt,
    output wire request_space, response_pending,
    output wire [1:0] response_code,
    input wire [3:0] read_index, output wire [31:0] read_data,
    output wire config_valid, input wire config_ready,
    output reg [1:0] config_operation, output reg [2:0] config_space,
    output reg config_write, output wire [31:0] config_address,
    output wire [159:0] config_data,
    input wire config_response_valid, config_response_error,
    input wire [159:0] config_response_data
);
    localparam [2:0] FILL=0, OPCODE=1, SCAN=2, ISSUE=3, WAIT=4, COPY=5, REPLY=6;
    localparam [3:0] ACCEPTED=0, FULL=1, EPOCH=2, FAULT=3, INVALID=4, UNAVAILABLE=5, OLD=6, INCOMPLETE=7;
    reg [2:0] phase;
    reg [3:0] cursor;
    reg [1:0] code;
    reg [159:0] working;
    (* ram_style="distributed" *) reg [31:0] memory [0:15];
    wire [3:0] read_address=phase==FILL ? 4'd0 : phase==OPCODE ? 4'd1
        : phase==ISSUE || phase==WAIT ? 4'd3
        : phase==REPLY ? (valid && operation==2 ? 4'd14 : read_index) : cursor;
    wire [31:0] value=memory[read_address];
    assign config_address=value;
    assign config_data=working;
    assign request_space=live && !halt && phase==FILL;
    assign response_pending=live && phase==REPLY;
    assign response_code=response_pending ? code : 2'd0;
    assign read_data=!response_pending ? 32'd0 : read_index==14 ? value
        : code!=0 || read_index>=5 || (config_operation==2 && read_index!=0)
        ? 32'd0 : config_operation==1 && read_index==4 ? {31'd0,value[0]} : value;
    assign config_valid=live && !halt && phase==ISSUE;
    assign receipt=!live ? EPOCH : halt ? FAULT
        : operation==2 ? (phase!=REPLY ? UNAVAILABLE : word!=value ? OLD : ACCEPTED)
        : operation>2 ? INVALID : phase!=FILL ? FULL
        : operation==0 ? (index>=14 ? INVALID : index!=0 && index!=cursor ? INCOMPLETE : ACCEPTED)
        : word==0 ? INVALID : cursor!=14 ? INCOMPLETE : value!=epoch ? EPOCH : ACCEPTED;
    wire accept=valid && receipt==ACCEPTED;
    wire known_operation=value>=1 && value<=3;
    wire bad_control=(value & (config_operation==3 ? 32'hfffffef8 : 32'hfffffefc))!=0;
    wire bad_scan=(cursor==2 && bad_control)
        || (cursor==3 && config_operation==1 && value[31:16]!=0)
        || (cursor==8 && config_operation==1 && value[31:1]!=0)
        || (cursor>=5 && cursor<=8 && config_operation==2 && value!=0)
        || (cursor>=9 && value!=0);
    wire refuse=(phase==OPCODE && !known_operation) || (phase==SCAN && bad_scan);
    wire complete=(phase==WAIT || (phase==ISSUE && config_ready)) && config_response_valid;
    wire abandon=halt && phase!=FILL && phase!=COPY && phase!=REPLY;
    wire copy_write=live && phase==COPY;
    wire write_enable=copy_write || (accept && operation!=2);
    wire [3:0] write_address=copy_write ? cursor : operation==1 ? 4'd14 : index;
    wire [31:0] write_data=copy_write ? working[31:0] : word;
    always @(posedge clk) begin
        if (write_enable) memory[write_address]<=write_data;
        if (!live) begin
            phase<=FILL; cursor<=0; code<=0;
            config_operation<=0; config_space<=0; config_write<=0;
        end else begin
            if (accept) case (operation)
                0: if (index==0) cursor<=1; else cursor<=cursor+1'b1;
                1: phase<=OPCODE;
                2: begin phase<=FILL; cursor<=0; end
                default: ;
            endcase
            case (phase)
                OPCODE: begin config_operation<=value[1:0]; cursor<=2; phase<=SCAN; end
                SCAN: begin
                    if (cursor==2) begin config_space<=value[2:0]; config_write<=value[8]; end
                    if (cursor>=4 && cursor<=8) working<={value,working[159:32]};
                    if (cursor==13) phase<=ISSUE; else cursor<=cursor+1'b1;
                end
                ISSUE: if (config_ready) phase<=WAIT;
                COPY: begin
                    working<={32'd0,working[159:32]};
                    if (cursor==4) phase<=REPLY; else cursor<=cursor+1'b1;
                end
                default: ;
            endcase
            if (complete) begin
                phase<=config_response_error ? REPLY : COPY; cursor<=0;
                working<=config_response_data;
                code<=config_response_error ? 2'd3 : 2'd0;
            end
            if (refuse || abandon) begin
                phase<=REPLY;
                code<=abandon ? 2'd3 : phase==OPCODE ? 2'd1 : 2'd2;
            end
        end
    end
endmodule
