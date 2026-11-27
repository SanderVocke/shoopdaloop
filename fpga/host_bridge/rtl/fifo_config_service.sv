module fifo_config_service (
    input wire clk, live, halt, input wire [31:0] epoch,
    input wire valid, input wire [1:0] operation, input wire [3:0] index,
    input wire [31:0] word,
    output wire [3:0] receipt,
    output wire request_space, response_pending,
    output wire [1:0] response_code,
    input wire [3:0] read_index, output wire [31:0] read_data,
    input wire media_valid, input wire [1:0] media_operation, input wire [31:0] media_word,
    output wire [3:0] media_receipt, output wire [2:0] media_status,
    output wire [31:0] media_read_data,
    output wire config_valid, input wire config_ready,
    output reg [2:0] config_operation, output reg [2:0] config_space,
    output reg config_write, output wire [31:0] config_address,
    output wire [159:0] config_data, output wire [223:0] config_media_data,
    output wire config_media_write_valid, input wire config_media_write_ready,
    output wire [31:0] config_media_write_data,
    input wire config_media_read_valid, output wire config_media_read_ready,
    input wire [31:0] config_media_read_data, config_media_result_view,
    input wire config_media_result_empty,
    input wire config_response_valid, config_response_error,
    output wire config_response_ready,
    input wire [128:0] config_core_data,
    input wire [31:0] config_audio_data,
    input wire [159:0] config_store_data
);
    localparam [2:0] FILL=0, OPCODE=1, SCAN=2, ISSUE=3, WAIT=4, COPY=5, REPLY=6;
    localparam [3:0] ACCEPTED=0, FULL=1, EPOCH=2, FAULT=3, INVALID=4, UNAVAILABLE=5, OLD=6, INCOMPLETE=7;
    reg [2:0] phase;
    reg [3:0] cursor;
    reg [1:0] code;
    reg [223:0] working;
    reg media_authorized;
    wire media_active=config_operation==5 && (phase==ISSUE || phase==WAIT || (phase==REPLY && code==0));
    (* ram_style="distributed" *) reg [31:0] memory [0:15];
    wire [3:0] read_address=media_valid && media_operation==0 && media_active ? 4'd14
        : phase==FILL ? 4'd0 : phase==OPCODE ? 4'd1
        : phase==ISSUE || phase==WAIT ? 4'd3
        : phase==REPLY ? (valid && operation==2 ? 4'd14 : read_index) : cursor;
    wire [31:0] value=memory[read_address];
    assign config_address=value;
    assign config_data=working[159:0];
    assign config_media_data=working;
    wire [159:0] config_response_data=config_operation==5 ? {64'd0,
        (config_space==6 && !config_media_result_empty ? working[223:192] : 32'd0),
        31'd0,config_media_result_empty,config_media_result_view} : {config_store_data[159:129],
        (config_operation==1 ? config_core_data[128:32] : config_store_data[128:32]),
        (config_operation==1 ? config_core_data[31:0] : config_operation==2 ? config_audio_data : config_store_data[31:0])};
    assign request_space=live && !halt && phase==FILL;
    assign response_pending=live && phase==REPLY;
    assign response_code=response_pending ? code : 2'd0;
    assign read_data=!response_pending ? 32'd0 : read_index==14 ? value
        : code!=0 || config_operation==4 || read_index>=5 || (config_operation==2 && read_index!=0)
        ? 32'd0 : config_operation==1 && read_index==4 ? {31'd0,value[0]} : value;
    assign config_valid=live && !halt && phase==ISSUE;
    assign receipt=!live ? EPOCH : halt ? FAULT
        : operation==2 ? (phase!=REPLY ? UNAVAILABLE : word!=value ? OLD
            : config_operation==5 && config_space==6 && config_media_read_valid ? INCOMPLETE : ACCEPTED)
        : operation>2 ? INVALID : phase!=FILL ? FULL
        : operation==0 ? (index>=14 ? INVALID : index!=0 && index!=cursor ? INCOMPLETE : ACCEPTED)
        : word==0 ? INVALID : cursor!=14 ? INCOMPLETE : value!=epoch ? EPOCH : ACCEPTED;
    wire accept=valid && receipt==ACCEPTED;
    wire media_write_available=config_operation==5 && config_space==5 && phase==WAIT && config_media_write_ready;
    wire media_read_available=config_operation==5 && config_space==6 && phase==REPLY && code==0 && config_media_read_valid;
    assign media_status=!live || halt ? 3'd0 : {media_authorized && media_active,
        media_authorized && media_read_available,media_authorized && media_write_available};
    assign media_receipt=!live ? EPOCH : halt ? FAULT : media_operation>2 ? INVALID
        : !media_active ? UNAVAILABLE : media_operation==0 ? (media_word==value ? ACCEPTED : OLD)
        : !media_authorized ? OLD : media_operation==1 ? (media_write_available ? ACCEPTED : UNAVAILABLE)
        : media_read_available ? ACCEPTED : UNAVAILABLE;
    wire media_accept=media_valid && media_receipt==ACCEPTED;
    assign media_read_data=media_receipt==ACCEPTED && media_operation==2 ? config_media_read_data : 32'd0;
    assign config_media_write_valid=media_accept && media_operation==1;
    assign config_media_write_data=media_word;
    assign config_media_read_ready=(media_accept && media_operation==2)
        || (live && !halt && config_operation==5 && config_space==6 && phase==REPLY && code==3);
    assign config_response_ready=live && !halt && (phase==WAIT || phase==ISSUE);
    wire known_operation=(value>=1 && value<=4) || value==17;
    wire bad_control=(value & (config_operation==3 ? 32'hfffffef8 : 32'hfffffefc))!=0;
    wire bad_scan=config_operation==5 ? ((cursor==2 && value[31:3]!=0)
            || (cursor==9 && (config_space==5 || config_space==6) && value>32)
            || (cursor>=10 && value!=0)) : config_operation==4 ? (cursor>=4 && value!=0)
        : (cursor==2 && bad_control)
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
            config_operation<=0; config_space<=0; config_write<=0; media_authorized<=0;
        end else begin
            if (media_accept && media_operation==0) media_authorized<=1;
            if (accept && operation!=0) media_authorized<=0;
            if (accept) case (operation)
                0: if (index==0) cursor<=1; else cursor<=cursor+1'b1;
                1: phase<=OPCODE;
                2: begin phase<=FILL; cursor<=0; end
                default: ;
            endcase
            case (phase)
                OPCODE: begin config_operation<=value==17 ? 3'd5 : value[2:0]; cursor<=2; phase<=SCAN; end
                SCAN: begin
                    if (cursor==2) begin config_space<=value[2:0]; config_write<=value[8]; end
                    if (config_operation==5) begin
                        if (cursor>=3 && cursor<=9) working<={value,working[223:32]};
                    end else if (config_operation==4) begin
                        if (cursor==2) working<={192'd0,value};
                        if (cursor==3) working<={160'd0,value,working[31:0]};
                    end else if (cursor>=4 && cursor<=8) working<={64'd0,value,working[159:32]};
                    if (cursor==13) phase<=ISSUE; else cursor<=cursor+1'b1;
                end
                ISSUE: if (config_ready) phase<=WAIT;
                COPY: begin
                    working<={32'd0,working[223:32]};
                    if (cursor==4) phase<=REPLY; else cursor<=cursor+1'b1;
                end
                default: ;
            endcase
            if (complete) begin
                phase<=config_response_error ? REPLY : COPY; cursor<=0;
                working<={64'd0,config_response_data};
                code<=config_response_error ? 2'd3 : 2'd0;
            end
            if (refuse || abandon) begin
                phase<=REPLY;
                code<=abandon ? 2'd3 : phase==OPCODE ? 2'd1 : 2'd2;
            end
        end
    end
endmodule
