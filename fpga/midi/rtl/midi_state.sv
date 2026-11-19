module midi_state (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready,
    input wire command_initialize, input wire [63:0] command_base,
    input wire [23:0] command_packet, input wire [1:0] command_length,
    output wire response_valid, input wire response_ready, output reg [2:0] response_error,
    output wire changed_valid, input wire changed_ready,
    output wire [12:0] changed_cell, output wire [31:0] changed_data,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output reg [63:0] memory_address, output reg [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    `include "midi_functions.svh"
    localparam [5:0] IDLE=0, READ=1, READ_WAIT=2, WRITE=3, WRITE_WAIT=4, CHANGE=5,
        RESPONSE=6, INITIAL=7, INITIAL_NEXT=8, GLOBAL=9, PEDAL=10, PEDAL_READ=11,
        APPLY=12, NOTE=13, CONTROL=14, CONTROL_PAIR=15, PROGRAM_BANK0=16,
        PROGRAM_BANK0_READ=17, PROGRAM_BANK1=18, PROGRAM_BANK1_READ=19,
        NOTES=20, NOTES_READ=21, NOTES_NEXT=22, CONTROLS=23, CONTROLS_NEXT=24,
        DEFAULTS=25, DEFAULTS_NEXT=26, RESET_FIELDS=27, RESET_FIELDS_NEXT=28;
    reg [5:0] phase, return_phase;
    reg [63:0] base;
    reg [23:0] packet;
    reg [12:0] channel_base, write_index;
    reg [8:0] cursor;
    reg [2:0] action;
    reg reset_marker;
    reg [31:0] data_word;
    reg [7:0] sustain, sostenuto, hold2;
    wire [7:0] status=packet[7:0], a=packet[15:8], b=packet[23:16];
    wire holds=(sustain>=64 && sustain<128) || (hold2>=64 && hold2<128);
    assign command_ready=phase==IDLE && !reset;
    assign response_valid=phase==RESPONSE && !reset;
    assign memory_valid=(phase==READ || phase==WRITE) && !reset;
    assign memory_write=phase==WRITE;
    assign memory_response_ready=phase==READ_WAIT || phase==WRITE_WAIT;
    assign changed_valid=phase==CHANGE && !reset;
    assign changed_cell=write_index;
    assign changed_data=memory_write_data;
    task automatic read_cell(input integer field, input [5:0] next);
        begin memory_address<=base+(64'(channel_base)+64'(field))*4;return_phase<=next;phase<=READ;end
    endtask
    task automatic put(input integer field, input [31:0] value, input [5:0] next);
        begin
            write_index<=channel_base+13'(field);memory_address<=base+(64'(channel_base)+64'(field))*4;
            memory_write_data<=value;return_phase<=next;phase<=WRITE;
        end
    endtask
    always @(posedge clk) begin : execution
        reg [31:0] note;
        reg [7:0] cc;
        reg [5:0] next;
        if(reset) begin
            phase<=IDLE;return_phase<=0;base<=0;packet<=0;channel_base<=0;write_index<=0;
            cursor<=0;action<=0;reset_marker<=0;data_word<=0;sustain<=0;sostenuto<=0;hold2<=0;
            response_error<=0;memory_address<=0;memory_write_data<=0;
        end else case(phase)
            IDLE: if(command_valid) begin
                base<=command_base;packet<=command_packet;cursor<=0;
                reset_marker<=!command_initialize;
                response_error<=command_initialize ? 0 : packet_error(command_packet,command_length);
                channel_base<=command_initialize || command_packet[7:0]==8'hff ? 0 : 13'(command_packet[3:0])*264;
                if(!command_initialize && packet_error(command_packet,command_length)!=0) phase<=RESPONSE;
                else phase<=command_initialize || command_packet[7:0]==8'hff ? INITIAL : PEDAL;
            end
            READ: if(memory_ready) phase<=READ_WAIT;
            READ_WAIT: if(memory_response_valid) begin
                if(memory_response_error) begin response_error<=7;phase<=RESPONSE;end
                else begin data_word<=memory_read_data;phase<=return_phase;end
            end
            WRITE: if(memory_ready) phase<=WRITE_WAIT;
            WRITE_WAIT: if(memory_response_valid) begin
                if(memory_response_error) begin response_error<=7;phase<=RESPONSE;end
                else phase<=CHANGE;
            end
            CHANGE: if(changed_ready) phase<=return_phase;
            INITIAL: put(integer'(cursor),initial_cell(integer'(cursor)),INITIAL_NEXT);
            INITIAL_NEXT: if(cursor==263) begin
                    cursor<=0;
                    if(channel_base==3960) phase<=GLOBAL;else begin channel_base<=channel_base+264;phase<=INITIAL;end
                end else begin cursor<=cursor+1;phase<=INITIAL;end
            GLOBAL: begin
                write_index<=4224;memory_address<=base+16896;memory_write_data<={31'b0,reset_marker};
                return_phase<=RESPONSE;phase<=WRITE;
            end
            PEDAL: read_cell(cursor==0 ? 192 : cursor==1 ? 194 : 197,PEDAL_READ);
            PEDAL_READ: begin
                if(cursor==0) sustain<=data_word[7:0];
                else if(cursor==1) sostenuto<=data_word[7:0];else hold2<=data_word[7:0];
                if(cursor==2) begin cursor<=0;phase<=APPLY;end
                else begin cursor<=cursor+1;phase<=PEDAL;end
            end
            APPLY: case(status[7:4])
                8,9,10: read_cell(integer'(a),NOTE);
                11: begin
                    cursor<=0;
                    case(a)
                        120: begin action<=4;phase<=NOTES;end
                        121: phase<=CONTROLS;
                        123: begin action<=3;phase<=NOTES;end
                        default: read_cell(128+integer'(a),CONTROL);
                    endcase
                end
                12: put(256,{24'b0,a},PROGRAM_BANK0);
                13: put(260,{24'b0,a},RESPONSE);
                14: put(259,{18'b0,b[6:0],a[6:0]},RESPONSE);
                default: begin response_error<=1;phase<=RESPONSE;end
            endcase
            NOTE: begin
                note=data_word;
                if(status[7:4]==10) note[23:16]=b;
                else if(status[7:4]==9 && b!=0) begin note[7:0]=b;note[8]=1;end
                else begin note[8]=0;if(!holds && !note[9]) note[7:0]=0;end
                if(note==data_word) phase<=RESPONSE;else put(integer'(a),note,RESPONSE);
            end
            CONTROL: begin
                next=RESPONSE;
                if(a==66) begin
                    if(!(sostenuto>=64 && sostenuto<128) && b>=64) begin action<=1;next=NOTES;end
                    else if(sostenuto>=64 && sostenuto<128 && b<64) begin action<=2;next=NOTES;end
                    sostenuto<=b;
                end else if(a>=124 && a<=127) begin action<=3;next=CONTROL_PAIR;end
                else if(a==64) begin
                    if(sustain>=64 && sustain<128 && b<64) begin action<=0;next=NOTES;end
                    sustain<=b;
                end else if(a==69) begin
                    if(hold2>=64 && hold2<128 && b<64) begin action<=0;next=NOTES;end
                    hold2<=b;
                end
                if(data_word=={24'b0,b}) phase<=next;else put(128+integer'(a),{24'b0,b},next);
            end
            CONTROL_PAIR: put(128+(integer'(a)^1),128,NOTES);
            PROGRAM_BANK0: read_cell(128,PROGRAM_BANK0_READ);
            PROGRAM_BANK0_READ: put(257,data_word,PROGRAM_BANK1);
            PROGRAM_BANK1: read_cell(160,PROGRAM_BANK1_READ);
            PROGRAM_BANK1_READ: put(258,data_word,RESPONSE);
            NOTES: read_cell(integer'(cursor),NOTES_READ);
            NOTES_READ: begin
                note=data_word;
                if(action==1) note[9]=note[8];
                if(action==2 || action==5) note[9]=0;
                if(action==3 || action==4) note[8]=0;
                if(action==4) begin note[7:0]=0;note[9]=0;end
                else if(!note[8] && !note[9] && !holds) note[7:0]=0;
                if(action==5) note[23:16]=0;
                if(note==data_word) phase<=NOTES_NEXT;else put(integer'(cursor),note,NOTES_NEXT);
            end
            NOTES_NEXT: if(cursor==127) phase<=RESPONSE;else begin cursor<=cursor+1;phase<=NOTES;end
            CONTROLS: begin
                cc=8'(cursor);
                if(cc==0 || cc==32 || cc==7 || cc==39 || cc==10 || cc==42 ||
                    cc>=70 && cc<=79 || cc>=91 && cc<=95 || cc==122 || cc>=124) phase<=CONTROLS_NEXT;
                else put(128+integer'(cursor),128,CONTROLS_NEXT);
            end
            CONTROLS_NEXT: if(cursor==127) begin cursor<=0;phase<=DEFAULTS;end
                else begin cursor<=cursor+1;phase<=CONTROLS;end
            DEFAULTS: begin
                case(cursor)
                    0: cc=1;1: cc=64;2: cc=65;3: cc=66;4: cc=67;5: cc=69;default: cc=11;
                endcase
                put(128+integer'(cc),cursor==6 ? 127 : 0,DEFAULTS_NEXT);
            end
            DEFAULTS_NEXT: if(cursor==6) begin cursor<=0;sustain<=0;sostenuto<=0;hold2<=0;phase<=RESET_FIELDS;end
                else begin cursor<=cursor+1;phase<=DEFAULTS;end
            RESET_FIELDS: put(259+integer'(cursor),cursor==0 ? 8192 : cursor==2 ? 1 : 0,RESET_FIELDS_NEXT);
            RESET_FIELDS_NEXT: if(cursor==4) begin cursor<=0;action<=5;phase<=NOTES;end
                else begin cursor<=cursor+1;phase<=RESET_FIELDS;end
            RESPONSE: if(response_ready) phase<=IDLE;
            default: begin response_error<=7;phase<=RESPONSE;end
        endcase
    end
endmodule
