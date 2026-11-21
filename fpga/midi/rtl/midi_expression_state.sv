// Direct data operations for the shared JP. Bulk performance data stays in DDR.
module midi_expression_state #(
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8, VIEWS=512
) (
    input wire clk, reset, input wire [63:0] arena_base, output wire [63:0] arena_bytes,
    input wire command_valid, output wire command_ready, input wire [3:0] command_op,
    input wire [63:0] arg0,arg1,arg2,arg3,
    output wire tracking_enabled,
    output wire done, output reg [3:0] error, output reg [31:0] result,
    output wire event_valid, input wire event_ready, output reg [63:0] event_view,
    output reg [31:0] event_index, input wire event_response_valid,
    input wire [3:0] event_error, input wire [63:0] event_frame, input wire [31:0] event_packet,
    output wire packet_valid, input wire packet_ready, packet_refused,
    output reg [31:0] packet, output reg [15:0] destination,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output reg [63:0] memory_address, output reg [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    initial if(LOOPS<1 || LOOPS>128 || INPUTS<1 || DESTINATIONS<1 || DESTINATIONS>8 || VIEWS<1) $fatal(1,"Invalid expression-state capacities");
    localparam [127:0] ALLOWED_CC=128'h00ffffc0ffffffdf00000000ffffffbe;
    localparam integer TRACKERS=LOOPS*DESTINATIONS, STATE_BYTES=32768, TRACK_BYTES=2048;
    localparam integer META0=(TRACKERS+1)>VIEWS ? TRACKERS+1 : VIEWS,
        META1=META0>DESTINATIONS*16 ? META0 : DESTINATIONS*16,
        META2=META1>INPUTS ? META1 : INPUTS;
    localparam integer LB=LOOPS>1 ? $clog2(LOOPS) : 1, IB=INPUTS>1 ? $clog2(INPUTS) : 1,
        DB=DESTINATIONS>1 ? $clog2(DESTINATIONS) : 1, VB=VIEWS>1 ? $clog2(VIEWS) : 1,
        TB=$clog2(TRACKERS+1), DIRB=$clog2(DESTINATIONS*16);
    localparam [63:0] OUTPUT_BASE=64'(INPUTS)*STATE_BYTES,
        SNAPSHOT_BASE=OUTPUT_BASE+64'(DESTINATIONS)*STATE_BYTES,
        SCRATCH_BASE=SNAPSHOT_BASE+64'(VIEWS)*STATE_BYTES,
        TRACK_BASE=SCRATCH_BASE+64'(STATE_BYTES);
    assign arena_bytes=TRACK_BASE+64'(TRACKERS+1)*TRACK_BYTES;
    localparam [31:0] KNOWN=32'h80000000, KEY=128, SOST=256, SUST=512, POLY=32'h00800000,
        CORE=1023;
    localparam [127:0] DEFAULT_CC=(128'b1<<1)|(128'b1<<2)|(128'b1<<4)|(128'b1<<11)|
        (128'b1<<64)|(128'b1<<66)|(128'b1<<67)|(128'b1<<74);
    typedef enum logic [8:0] {
        IDLE, REPLY, RD, RW, WR, WW, FORMAT, FORMAT_NEXT, FORMAT_META,
        BIND, DIRECTORY, DIRECTORY_READ, DIRECTORY_WRITE, REBIND, REBIND_NEXT, TRACK_CLEAR, TRACK_CLEARED, REFRESH, REFRESH_VALUE,
        APPLY, APPLY_VALUE, APPLY_PEDAL, APPLY_NOTES, APPLY_NOTE, APPLY_RESET, APPLY_RESET_VALUE, APPLY_FINISH, APPLY_OWNER, WATCH_DEAD, WATCH_TEMP, WATCH_TEMP_VALUE, RAM_WAIT, RAM_READY, FAST_TARGET, FAST_VALUE, RESTORE_WORD, RESTORE_WORD_VALUE, RESTORE_LOAD_CURRENT, RESTORE_OFF_PEDALS, RESTORE_CLAIM,
        STORE_FIELD, FIELD_STORED, WATCH, WATCH_VALUE, DIFF_SUMMARY, DIFF_WORD, DIFF_COUNT, DIFF_WRITTEN, DIFF_SUM_WRITTEN,
        ACTIVITY, ACTIVITY_VALUE, ACTIVITY_WRITTEN,
        CLONE, CLONE_NEXT, CLONE_READ, CLONE_WRITE, CLONE_FINISH, PREFIX, PREFIX_WAIT, PREFIX_APPLY,
        RESTORE, RESTORE_OUTPUT, RESTORE_CHANNEL, RESTORE_SUMMARY, RESTORE_LOAD, RESTORE_LOADED,
        RESTORE_DETECT, RESTORE_EDGE, RESTORE_GROUP, RESTORE_GROUP_NOTES, RESTORE_OFF,
        RESTORE_CONTROLS, RESTORE_CAPTURED, RESTORE_SOST, RESTORE_OTHER, RESTORE_RELEASE, RESTORE_PRESSURE,
        SEND, SENT, CLEANUP, CLEANUP_OUTPUT, CLEANUP_CHANNEL, CLEANUP_WORD, CLEANUP_KEYS, CLEANUP_NOTE, CLEANUP_PEDAL, CLEANUP_LOAD, CLEANUP_ADVANCE,
        SEED, SEEDED, SEED_CLEAR, SEED_CLEARED
    } phase_t;
    phase_t phase, after_memory, after_apply, after_field, after_diff, after_activity, after_clone, after_send, after_ram;
    reg [3:0] op;
    reg [31:0] data, old_value, value;
    reg [1:0] context_kind;
    reg [31:0] context_id, seq, output_id, tracker, owner_seq;
    reg [12:0] field;
    reg [8:0] local_field;
    reg [3:0] channel;
    reg [31:0] message_word, pedal64, pedal66;
    reg [7:0] apply_mode;
    reg [127:0] watchers;
    reg [31:0] watched_seq, diff_word, summary_word;
    reg [7:0] summary_index;
    reg dirty;
    reg [31:0] scan, clear_index, refresh_field;
    reg [63:0] work_view, source_view, position, owner;
    reg [15:0] clone_channels, clone_source_channels;
    reg [1:0] clone_kind;
    reg [31:0] clone_id, clone_source_id;
    reg [3:0] clone_channel;
    reg [8:0] clone_field;
    reg [15:0] restore_channels;
    reg seek_target, group_repair, edge_repair, any_voice;
    reg [8:0] restore_field;
    reg [31:0] dirty_notes[0:3];
    reg [31:0] dirty_controls[0:4];
    reg [31:0] dirty_summary;
    reg [31:0] activity_word;
    reg [2:0] activity_index;
    reg [6:0] activity_key;
    reg activity_set;
    reg [31:0] activity_tracker;
    reg [3:0] activity_channel;
    reg [31:0] cleanup_bits;
    reg [2:0] cleanup_word;
    reg [3:0] cleanup_channel;
    reg [31:0] cleanup_tracker;
    reg cleanup_mode;
    (* ram_style="block" *) reg [31:0] current_channel[0:257];
    (* ram_style="block" *) reg [31:0] target_channel[0:257];
    reg [31:0] current_q, target_q;
    reg [31:0] old64,old66,new64,new66,load_current;
    reg [3:0] restore_word;
    reg refresh_compare;
    reg [12:0] changed_field;
    reg [8:0] ram_address;
    reg [63:0] bound_view[0:LOOPS-1], bound_owner[0:LOOPS-1];
    reg [15:0] bound_channels[0:LOOPS-1];
    reg [7:0] bound_outputs[0:LOOPS-1], bound_input[0:LOOPS-1];
    reg [LOOPS-1:0] binding_valid;
    (* ram_style="block" *) reg [LOOPS-1:0] directory[0:DESTINATIONS*16-1];
    reg [15:0] input_channels[0:INPUTS-1], view_channels[0:VIEWS-1];
    reg [127:0] policies[0:DESTINATIONS-1];
    (* ram_style="block" *) reg [13:0] counts[0:TRACKERS];
    reg [TRACKERS:0] nonempty;
    reg [13:0] count_q;
    reg [LOOPS-1:0] directory_q;
    wire [DIRB-1:0] directory_address=(phase==DIRECTORY || phase==DIRECTORY_READ || phase==DIRECTORY_WRITE) ? DIRB'(scan) : DIRB'(context_id*16+32'(field[12:9]));
    wire [7:0] status=message_word[7:0];
    wire [6:0] a=message_word[14:8], b=message_word[22:16];
    wire [63:0] tracker_address=arena_base+TRACK_BASE+(64'(tracker)<<11);
    assign tracking_enabled=binding_valid!=0;
    assign command_ready=phase==IDLE && !reset;
    assign done=phase==REPLY && !reset;
    assign memory_valid=(phase==RD || phase==WR) && !reset;
    assign memory_write=phase==WR;
    assign memory_response_ready=phase==RW || phase==WW;
    assign packet_valid=phase==SEND && !reset;
    assign event_valid=phase==PREFIX && !reset;
    function automatic ordinary(input [6:0] cc);
        ordinary=(cc>=1 && cc<=5)||(cc>=7 && cc<=31)||(cc>=64 && cc<=68)||
            (cc>=70 && cc<=95)||(cc>=102 && cc<=119);
    endfunction
    function automatic managed(input [31:0] port, input [12:0] f);
        managed=f[8:0]<128 || f[8:0]==256 || f[8:0]==257 ||
            (f[8:0]>=128 && f[8:0]<256 && policies[DB'(port)][f[6:0]]);
    endfunction
    function automatic different(input [31:0] target_value, input [31:0] actual, input [12:0] f);
        different=target_value[31] && (!actual[31] || (f[8:0]<128 ?
            (target_value[9:0]!=actual[9:0] || (target_value[6:0]!=0 && target_value[23] &&
                (!actual[23] || target_value[22:16]!=actual[22:16]))) : target_value!=actual));
    endfunction
    function automatic [63:0] state_address(input [1:0] kind,input [31:0] id,input [12:0] f);
        reg [63:0] offset;
        begin
            case(kind)
                0: offset=64'(id)*STATE_BYTES;
                1: offset=OUTPUT_BASE+64'(id)*STATE_BYTES;
                2: offset=SNAPSHOT_BASE+64'(id)*STATE_BYTES;
                default: offset=SCRATCH_BASE;
            endcase
            state_address=arena_base+offset+{49'b0,f,2'b0};
        end
    endfunction
    function automatic [63:0] activity_address(input [31:0] t,input [3:0] ch,input [2:0] w);
        activity_address=arena_base+TRACK_BASE+(64'(t)<<11)+1056+64'(ch)*20+64'(w)*4;
    endfunction
    task automatic read_word(input [63:0] address,input phase_t next);
        begin memory_address<=address;after_memory<=next;phase<=RD;end
    endtask
    task automatic write_word(input [63:0] address,input [31:0] word_value,input phase_t next);
        begin memory_address<=address;memory_write_data<=word_value;after_memory<=next;phase<=WR;end
    endtask
    task automatic fail(input [3:0] code);
        begin error<=code;phase<=REPLY;end
    endtask
    task automatic apply_packet(input [1:0] kind,input [31:0] id,input [31:0] msg,input phase_t next);
        begin context_kind<=kind;context_id<=id;message_word<=msg;channel<=msg[3:0];after_apply<=next;phase<=APPLY;end
    endtask
    task automatic store_field(input [12:0] f,input [31:0] before_value,input [31:0] next_value,input phase_t next);
        begin field<=f;old_value<=before_value;value<=next_value;after_field<=next;phase<=STORE_FIELD;end
    endtask
    task automatic compare_field(input [31:0] t,input [12:0] f,input [31:0] target_value,input [31:0] actual,input phase_t next);
        begin tracker<=t;field<=f;dirty<=different(target_value,actual,f);after_diff<=next;
            read_word(arena_base+TRACK_BASE+(64'(t)<<11)+1024+{59'b0,f[12:10],2'b0},DIFF_SUMMARY);end
    endtask
    task automatic activity_bit(input [31:0] t,input [3:0] ch,input [2:0] w,input [6:0] key_index,input bit_value,input phase_t next);
        begin activity_tracker<=t;activity_channel<=ch;activity_index<=w;activity_key<=key_index;activity_set<=bit_value;after_activity<=next;phase<=ACTIVITY;end
    endtask
    task automatic send_packet(input [7:0] s,input [6:0] x,input [6:0] y,input phase_t next);
        begin packet<={6'b0,(s[7:4]==4'hd ? 2'd2 : 2'd3),1'b0,y,1'b0,x,s};after_send<=next;phase<=SEND;end
    endtask
    task automatic clone_state(input [1:0] source_kind,input [31:0] source_id,input [15:0] mask,input phase_t next);
        begin clone_kind<=source_kind;clone_source_id<=source_id;clone_source_channels<=mask;clone_channel<=0;clone_field<=0;after_clone<=next;phase<=CLONE;end
    endtask
    always @(posedge clk) begin
        current_q<=current_channel[ram_address];target_q<=target_channel[ram_address];
        count_q<=counts[TB'(tracker)];directory_q<=directory[directory_address];
    end
    always @(posedge clk) begin
        if(reset) begin
            phase<=IDLE;error<=0;result<=0;binding_valid<=0;nonempty<=0;memory_address<=0;memory_write_data<=0;
            op<=0;data<=0;packet<=0;destination<=0;context_kind<=0;context_id<=0;seq<=0;output_id<=0;tracker<=0;owner_seq<=32'hffffffff;
            field<=0;local_field<=0;channel<=0;message_word<=0;pedal64<=0;pedal66<=0;apply_mode<=0;old_value<=0;value<=0;
            watchers<=0;watched_seq<=0;dirty<=0;diff_word<=0;summary_word<=0;summary_index<=0;scan<=0;clear_index<=0;refresh_field<=0;
            work_view<=0;source_view<=0;position<=0;owner<=0;clone_channels<=0;clone_source_channels<=0;clone_kind<=0;clone_id<=0;clone_source_id<=0;clone_channel<=0;clone_field<=0;
            restore_channels<=0;seek_target<=0;group_repair<=0;edge_repair<=0;any_voice<=0;restore_field<=0;dirty_summary<=0;ram_address<=0;
            activity_word<=0;activity_index<=0;activity_key<=0;activity_set<=0;activity_tracker<=0;activity_channel<=0;
            cleanup_bits<=0;cleanup_word<=0;cleanup_channel<=0;cleanup_tracker<=0;cleanup_mode<=0;event_view<=0;event_index<=0;
            after_memory<=IDLE;after_apply<=IDLE;after_field<=IDLE;after_diff<=IDLE;after_activity<=IDLE;after_clone<=IDLE;after_send<=IDLE;after_ram<=IDLE;refresh_compare<=0;changed_field<=0;old64<=0;old66<=0;new64<=0;new66<=0;load_current<=0;restore_word<=0;
        end else case(phase)
            IDLE: if(command_valid) begin
                op<=command_op;error<=0;result<=0;seq<=arg0[31:0];output_id<=0;owner_seq<=32'hffffffff;
                work_view<=arg1;position<=arg2;owner<=arg3;seek_target<=0;cleanup_mode<=0;
                case(command_op)
                    0: begin scan<=0;binding_valid<=0;nonempty<=0;phase<=FORMAT;end
                    1: if(arg0>=64'(INPUTS)) fail(4);else begin input_channels[IB'(arg0)]<=input_channels[IB'(arg0)]|(arg1[7:0]==8'hff ? 16'hffff : (16'b1<<arg1[3:0]));apply_packet(0,arg0[31:0],arg1[31:0],REPLY);end
                    2: if(arg0>=64'(DESTINATIONS)) fail(4);else begin owner_seq<=arg2[31:0];apply_packet(1,arg0[31:0],arg1[31:0],REPLY);end
                    3: if(arg0>=64'(LOOPS)) fail(4);else begin
                        clone_id<=arg1[31:0];clone_channels<=binding_valid[LB'(arg0)] ? bound_channels[LB'(arg0)] : 0;
                        clone_state(0,32'(bound_input[LB'(arg0)]),binding_valid[LB'(arg0)] ? input_channels[IB'(bound_input[LB'(arg0)])] & bound_channels[LB'(arg0)] : 0,CLONE_FINISH);
                    end
                    4,9: if(arg0>=64'(LOOPS)) fail(4);else if(command_op==9) begin
                        if(arg3[7:0]>=8'(INPUTS) || arg3[15:8]==0 || (16'(arg3[15:8])>>DESTINATIONS)!=0 || arg3[31:16]==0) fail(4);
                        else begin bound_owner[LB'(arg0)]<=arg2;bound_input[LB'(arg0)]<=arg3[7:0];bound_outputs[LB'(arg0)]<=arg3[15:8];bound_channels[LB'(arg0)]<=arg3[31:16];binding_valid[LB'(arg0)]<=1;scan<=0;phase<=DIRECTORY;end
                    end else phase<=REBIND;
                    5,6: if(arg0>=64'(LOOPS) || !binding_valid[LB'(arg0)] || bound_owner[LB'(arg0)]!=arg3) fail(4);
                        else if(command_op==6) begin cleanup_mode<=1;phase<=CLEANUP;end
                        else if(arg1==0 || bound_view[LB'(arg0)]!=arg1) fail(4);
                        else if(arg2!=0) begin seek_target<=1;clone_id<=0;clone_channels<=bound_channels[LB'(arg0)];source_view<=arg1;
                            clone_state(2,arg1[31:0],view_channels[VB'(arg1[31:0])] & bound_channels[LB'(arg0)],CLONE_FINISH);end
                        else phase<=RESTORE;
                    7: begin source_view<=arg1;work_view<=arg0;clone_id<=arg0[31:0];clone_channels<=16'hffff;
                        clone_state(2,arg1[31:0],view_channels[VB'(arg1[31:0])],CLONE_FINISH);end
                    8: if(arg0[31:0]>=32'(VIEWS)) fail(4);else begin view_channels[VB'(arg0[31:0])]<=0;phase<=REPLY;end
                    10,11: begin context_kind<=arg0[1:0];context_id<=arg1[31:0];field<=arg2[12:0];value<=arg3[31:0];
                        if(arg0>2 || arg2>=8192 || arg2[8:0]>257 || (arg0==0 && arg1>=64'(INPUTS)) ||
                            (arg0==1 && arg1>=64'(DESTINATIONS)) || (arg0==2 && arg1[31:0]>=32'(VIEWS))) fail(4);else phase<=SEED;end
                    12: if(arg0>=64'(DESTINATIONS)) fail(4);else if(({arg2,arg1} & ~ALLOWED_CC)!=0) fail(2);else begin policies[DB'(arg0)]<={arg2,arg1}|(128'b1<<64)|(128'b1<<66);scan<=0;phase<=BIND;end
                    default: fail(2);
                endcase
            end
            RD: if(memory_ready) phase<=RW;
            RW: if(memory_response_valid) begin if(memory_response_error) fail(7);else begin data<=memory_read_data;phase<=after_memory;end end
            WR: if(memory_ready) phase<=WW;
            WW: if(memory_response_valid) begin if(memory_response_error) fail(7);else phase<=after_memory;end
            FORMAT: if(scan<32'(INPUTS+DESTINATIONS)*8192) write_word(state_address(scan<32'(INPUTS)*8192 ? 2'd0 : 2'd1,(scan>>13)-(scan<32'(INPUTS)*8192 ? 0 : 32'(INPUTS)),scan[12:0]),0,FORMAT_NEXT);
                else begin scan<=0;phase<=FORMAT_META;end
            FORMAT_NEXT: begin scan<=scan+1;phase<=FORMAT;end
            FORMAT_META: begin
                if(scan<32'(LOOPS)) begin bound_view[LB'(scan)]<=0;bound_owner[LB'(scan)]<=0;bound_channels[LB'(scan)]<=0;bound_outputs[LB'(scan)]<=0;bound_input[LB'(scan)]<=0;end
                if(scan<32'(VIEWS)) view_channels[VB'(scan)]<=0;
                if(scan<32'(INPUTS)) input_channels[IB'(scan)]<=0;
                if(scan<32'(DESTINATIONS)) policies[DB'(scan)]<=DEFAULT_CC;
                if(scan<32'(DESTINATIONS*16)) directory[DIRB'(scan)]<=0;
                if(scan<32'(TRACKERS+1)) counts[TB'(scan)]<=0;
                if(scan==32'(META2)-1) begin tracker<=0;clear_index<=0;phase<=TRACK_CLEAR;end else scan<=scan+1;
            end
            DIRECTORY: phase<=DIRECTORY_READ;
            DIRECTORY_READ: phase<=DIRECTORY_WRITE;
            DIRECTORY_WRITE: begin
                if(bound_outputs[LB'(seq)][scan[6:4]] && bound_channels[LB'(seq)][scan[3:0]]) directory[DIRB'(scan)]<=directory_q|(LOOPS'(1)<<seq);
                else directory[DIRB'(scan)]<=directory_q & ~(LOOPS'(1)<<seq);
                if(scan==32'(DESTINATIONS*16)-1) phase<=REBIND;else scan<=scan+1;
            end
            REBIND: begin bound_view[LB'(seq)]<=work_view;output_id<=0;phase<=REBIND_NEXT;end
            REBIND_NEXT: if(output_id==32'(DESTINATIONS)) phase<=op==12 ? BIND : REPLY;
                else begin tracker<=seq*32'(DESTINATIONS)+output_id;clear_index<=0;refresh_field<=0;phase<=TRACK_CLEAR;end
            TRACK_CLEAR: begin counts[TB'(tracker)]<=0;nonempty[TB'(tracker)]<=0;
                if(clear_index<8) write_word(tracker_address+1024+64'(clear_index)*4,0,TRACK_CLEARED);
                else if(op==0 && clear_index<88) write_word(tracker_address+1056+64'(clear_index-8)*4,0,TRACK_CLEARED);
                else if(op==0) begin if(tracker==32'(TRACKERS)) phase<=REPLY;else begin tracker<=tracker+1;clear_index<=0;end end
                else if(op==4 || op==9 || op==3 || op==12) phase<=REFRESH;
                else if(op==5) phase<=REFRESH;
                else phase<=RESTORE_CHANNEL;
            end
            TRACK_CLEARED: begin clear_index<=clear_index+1;phase<=TRACK_CLEAR;end
            REFRESH: if(refresh_field>=8192) begin if(op==5) begin channel<=0;phase<=RESTORE_CHANNEL;end else begin output_id<=output_id+1;phase<=REBIND_NEXT;end end
                else if(!binding_valid[LB'(seq)] || !bound_outputs[LB'(seq)][output_id] || work_view==0 ||
                    !bound_channels[LB'(seq)][refresh_field[12:9]] || (!seek_target && !view_channels[VB'(work_view[31:0])][refresh_field[12:9]])) begin refresh_field<=(refresh_field & 32'hfffffe00)+512;end
                else if(refresh_field[8:0]>257) refresh_field<=(refresh_field & 32'hfffffe00)+512;
                else if(!managed(output_id,refresh_field[12:0])) refresh_field<=refresh_field+1;
                else begin field<=refresh_field[12:0];read_word(state_address(seek_target ? 2'd3 : 2'd2,work_view[31:0],refresh_field[12:0]),REFRESH_VALUE);end
            REFRESH_VALUE: begin value<=data;read_word(state_address(1,output_id,field),DIFF_WORD);after_diff<=REFRESH;refresh_compare<=1;end
            // FIELD_STORED enumerates only the changed field's relevant output/channel set.
            STORE_FIELD: if(old_value==value) phase<=after_field;
                else write_word(state_address(context_kind,context_id,field),value,FIELD_STORED);
            FIELD_STORED: begin
                if(context_kind==1) begin
                    if(op==5 && field[12:9]==channel && field[8:0]<=257) current_channel[field[8:0]]<=value;
                    watchers<=128'(directory_q);output_id<=context_id;phase<=WATCH;
                end else phase<=after_field;
            end
            WATCH: if(watchers==0) begin
                    if(op==5 && seek_target && context_id==32'(destination) && bound_channels[LB'(seq)][field[12:9]] && managed(context_id,field))
                        read_word(state_address(3,0,field),WATCH_TEMP);
                    else phase<=after_field;
                end
                else begin
                    watched_seq<=0;
                    for(integer i=LOOPS-1;i>=0;i=i-1) if(watchers[i]) watched_seq<=32'(i);
                    phase<=WATCH_VALUE;
                end
            WATCH_VALUE: begin
                watchers[watched_seq]<=0;
                if(bound_view[LB'(watched_seq)]!=0 && managed(output_id,field) && view_channels[VB'(bound_view[LB'(watched_seq)][31:0])][field[12:9]])
                    read_word(state_address(2,bound_view[LB'(watched_seq)][31:0],field),DIFF_WORD);
                else if(field[8:0]<128 && value[6:0]==0) activity_bit(watched_seq*32'(DESTINATIONS)+output_id,field[12:9],{1'b0,field[6:5]},field[6:0],0,WATCH);
                else phase<=WATCH;
                refresh_compare<=0;
            end
            WATCH_TEMP: compare_field(32'(TRACKERS),field,data,value,after_field);
            WATCH_DEAD: if(field[8:0]<128 && value[6:0]==0)
                activity_bit(watched_seq*32'(DESTINATIONS)+output_id,field[12:9],{1'b0,field[6:5]},field[6:0],0,WATCH);
                else phase<=WATCH;
            DIFF_WORD: begin
                if(refresh_compare) begin
                    refresh_field<=refresh_field+1;compare_field(tracker,field,value,data,REFRESH);
                end else begin
                    compare_field(watched_seq*32'(DESTINATIONS)+output_id,field,data,value,WATCH_DEAD);
                end
            end
            DIFF_SUMMARY: begin summary_word<=data;
                if(!data[field[9:5]]) begin diff_word<=0;phase<=DIFF_COUNT;end
                else read_word(tracker_address+{54'b0,field[12:5],2'b0},DIFF_COUNT);
            end
            DIFF_COUNT: begin
                if(summary_word[field[9:5]]) diff_word<=data;
                if((summary_word[field[9:5]] && data[field[4:0]])==dirty) phase<=after_diff;
                else begin
                    counts[TB'(tracker)]<=dirty ? count_q+1'b1 : count_q-1'b1;
                    nonempty[TB'(tracker)]<=dirty || count_q!=1;
                    diff_word<=dirty ? ((summary_word[field[9:5]] ? data : 0)|(32'b1<<field[4:0])) : (data & ~(32'b1<<field[4:0]));
                    write_word(tracker_address+{54'b0,field[12:5],2'b0},dirty ? ((summary_word[field[9:5]] ? data : 0)|(32'b1<<field[4:0])) : (data & ~(32'b1<<field[4:0])),DIFF_WRITTEN);
                end
            end
            DIFF_WRITTEN: if(summary_word[field[9:5]]==(diff_word!=0)) phase<=after_diff;
                else write_word(tracker_address+1024+{59'b0,field[12:10],2'b0},diff_word!=0 ? summary_word|(32'b1<<field[9:5]) : summary_word & ~(32'b1<<field[9:5]),DIFF_SUM_WRITTEN);
            DIFF_SUM_WRITTEN: phase<=after_diff;
            ACTIVITY: read_word(activity_address(activity_tracker,activity_channel,activity_index),ACTIVITY_VALUE);
            ACTIVITY_VALUE: write_word(activity_address(activity_tracker,activity_channel,activity_index),activity_set ? data|(32'b1<<activity_key[4:0]) : data & ~(32'b1<<activity_key[4:0]),ACTIVITY_WRITTEN);
            ACTIVITY_WRITTEN: phase<=after_activity;
            APPLY: begin
                apply_mode<=0;
                if(status==8'hff) begin channel<=0;local_field<=128;apply_mode<=4;phase<=APPLY_RESET;end
                else if(status>=8'hf0) phase<=APPLY_FINISH;
                else if(status[7:4]==8 || status[7:4]==9 || status[7:4]==10) begin
                    local_field<={2'b0,a};read_word(state_address(context_kind,context_id,{channel,9'd192}),APPLY_PEDAL);
                end else if(status[7:4]==11 && a==120) begin local_field<=0;apply_mode<=2;phase<=APPLY_NOTES;end
                else if(status[7:4]==11 && a>=123) begin local_field<=0;apply_mode<=3;read_word(state_address(context_kind,context_id,{channel,9'd192}),APPLY_PEDAL);end
                else if(status[7:4]==11 && a==121) begin local_field<=128;apply_mode<=5;phase<=APPLY_RESET;end
                else if(status[7:4]==11 && ordinary(a)) begin
                    local_field<=128+{2'b0,a};read_word(state_address(context_kind,context_id,{channel,2'b01,a}),APPLY_VALUE);
                end else if(status[7:4]==13) begin local_field<=257;read_word(state_address(context_kind,context_id,{channel,9'd257}),APPLY_VALUE);end
                else if(status[7:4]==14) begin local_field<=256;read_word(state_address(context_kind,context_id,{channel,9'd256}),APPLY_VALUE);end
                else phase<=APPLY_FINISH;
            end
            APPLY_PEDAL: begin pedal64<=data;
                if(apply_mode==3) phase<=APPLY_NOTES;
                else read_word(state_address(context_kind,context_id,{channel,local_field}),APPLY_VALUE);
            end
            APPLY_VALUE: begin
                if(status[7:4]==9 && b!=0) store_field({channel,local_field},data,KNOWN|KEY|POLY|(data&SOST)|{25'b0,b},APPLY_FINISH);
                else if(status[7:4]==8 || status[7:4]==9) begin
                    if((data & (SOST|SUST))!=0 || (data[6:0]!=0 && pedal64[6:0]>=64))
                        store_field({channel,local_field},data,KNOWN|(data & ~KEY)|(data[6:0]!=0 && pedal64[6:0]>=64 ? SUST : 0),APPLY_FINISH);
                    else store_field({channel,local_field},data,KNOWN,APPLY_FINISH);
                end else if(status[7:4]==10) begin
                    if(data[6:0]!=0) store_field({channel,local_field},data,(data & ~32'h007f0000)|POLY|({25'b0,b}<<16),APPLY_FINISH);else phase<=APPLY_FINISH;
                end else if(status[7:4]==11) begin
                    if((a==64 || a==66) && (data[6:0]>=64)!=(b>=64)) begin apply_mode<=1;local_field<=0;store_field({channel,2'b01,a},data,KNOWN|{25'b0,b},APPLY_NOTES);end
                    else store_field({channel,local_field},data,KNOWN|{25'b0,b},APPLY_FINISH);
                end else if(status[7:4]==13) store_field({channel,local_field},data,KNOWN|{25'b0,a},APPLY_FINISH);
                else store_field({channel,local_field},data,KNOWN|{18'b0,b,a},APPLY_FINISH);
            end
            APPLY_NOTES: if(local_field>=128) phase<=APPLY_FINISH;
                else read_word(state_address(context_kind,context_id,{channel,local_field}),APPLY_NOTE);
            APPLY_NOTE: begin
                local_field<=local_field+1;
                if(apply_mode==2) store_field({channel,local_field},data,KNOWN,APPLY_NOTES);
                else if(apply_mode==6 || apply_mode==7) begin
                    store_field({channel,local_field},data,apply_mode==7 && data[7] ? (data & ~(SUST|SOST|32'h007f0000))|POLY : (apply_mode==6 || data[31] ? KNOWN : 0),APPLY_NOTES);
                end else if(apply_mode==3 && data[31]) begin
                    if((data & (SOST|SUST))!=0 || (data[6:0]!=0 && pedal64[6:0]>=64))
                        store_field({channel,local_field},data,KNOWN|(data & ~KEY)|(data[6:0]!=0 && pedal64[6:0]>=64 ? SUST : 0),APPLY_NOTES);
                    else store_field({channel,local_field},data,KNOWN,APPLY_NOTES);
                end else if(apply_mode==1 && data[31] && data[6:0]!=0) begin
                    if(a==64) begin
                        if(b>=64 && !data[7]) store_field({channel,local_field},data,data|SUST,APPLY_NOTES);
                        else if(b<64) store_field({channel,local_field},data,(data & (KEY|SOST))==0 ? KNOWN : data & ~SUST,APPLY_NOTES);
                        else phase<=APPLY_NOTES;
                    end else if(b>=64 && data[7]) store_field({channel,local_field},data,data|SOST,APPLY_NOTES);
                    else if(b<64) store_field({channel,local_field},data,(data & (KEY|SUST))==0 ? KNOWN : data & ~SOST,APPLY_NOTES);
                    else phase<=APPLY_NOTES;
                end else phase<=APPLY_NOTES;
            end
            APPLY_RESET: if(local_field<=257) begin
                    if(local_field>=128 && local_field<256 && (!ordinary(local_field[6:0]) ||
                        (apply_mode==5 && (local_field==135 || local_field==138 || (local_field>=219 && local_field<=223))))) local_field<=local_field+1;
                    else read_word(state_address(context_kind,context_id,{channel,local_field}),APPLY_RESET_VALUE);
                end else begin local_field<=0;phase<=APPLY_RESET_VALUE;data<=0;end
            APPLY_RESET_VALUE: begin
                if(local_field<=257 && local_field>=128) begin
                    local_field<=local_field==257 ? 0 : local_field+1;
                    store_field({channel,local_field},data,local_field==256 ? KNOWN|8192 :
                        (local_field==257 || local_field==129 || (local_field>=192 && local_field<=195)) ? KNOWN : local_field==139 ? KNOWN|127 : 0,
                        local_field==257 ? APPLY_NOTES : APPLY_RESET);
                    if(local_field==257) apply_mode<=apply_mode==4 ? 6 : 7;
                end else phase<=APPLY_NOTES;
            end
            APPLY_FINISH: begin
                if(apply_mode==6 && channel!=15) begin channel<=channel+1;local_field<=128;apply_mode<=4;phase<=APPLY_RESET;end
                else if(context_kind==1 && owner_seq<32'(LOOPS) && binding_valid[LB'(owner_seq)]) begin
                    if(status[7:4]==8 || status[7:4]==9) read_word(state_address(1,context_id,{message_word[3:0],2'b0,a}),APPLY_OWNER);
                    else if(status[7:4]==11 && (a==64 || a==66))
                        activity_bit(owner_seq*32'(DESTINATIONS)+context_id,message_word[3:0],4,a==64 ? 0 : 1,b>=64,after_apply);
                    else phase<=after_apply;
                end else phase<=after_apply;
            end
            APPLY_OWNER: activity_bit(owner_seq*32'(DESTINATIONS)+context_id,message_word[3:0],{1'b0,a[6:5]},a,data[6:0]!=0,after_apply);
            CLONE: if(clone_channel==0 && clone_field==0) begin
                    if(op==3 || op==7) view_channels[VB'(clone_id)]<=clone_channels;
                    phase<=CLONE_NEXT;
                end else phase<=CLONE_NEXT;
            CLONE_NEXT: if(!clone_channels[clone_channel]) begin
                    if(clone_channel==15) phase<=after_clone;else clone_channel<=clone_channel+1;
                end else if(!clone_source_channels[clone_channel]) write_word(state_address(op==5 ? 2'd3 : 2'd2,clone_id,{clone_channel,clone_field}),0,CLONE_WRITE);
                else read_word(state_address(clone_kind,clone_source_id,{clone_channel,clone_field}),CLONE_READ);
            CLONE_READ: write_word(state_address(op==5 ? 2'd3 : 2'd2,clone_id,{clone_channel,clone_field}),data,CLONE_WRITE);
            CLONE_WRITE: if(clone_field==257) begin clone_field<=0;if(clone_channel==15) phase<=after_clone;else begin clone_channel<=clone_channel+1;phase<=CLONE_NEXT;end end
                else begin clone_field<=clone_field+1;phase<=CLONE_NEXT;end
            CLONE_FINISH: if(op==3) phase<=REBIND;
                else begin event_view<=source_view;event_index<=0;phase<=PREFIX;end
            PREFIX: if(event_ready) phase<=PREFIX_WAIT;
            PREFIX_WAIT: if(event_response_valid) begin
                if(event_error!=0) fail(event_error);
                else if(event_packet[25:24]==0 || event_frame>=position) begin if(op==5) phase<=RESTORE;else phase<=REPLY;end
                else begin
                    if(op==7) view_channels[VB'(clone_id)]<=view_channels[VB'(clone_id)]|(event_packet[7:0]==8'hff ? 16'hffff : (16'b1<<event_packet[3:0]));
                    apply_packet(op==5 ? 2'd3 : 2'd2,clone_id,event_packet,PREFIX_APPLY);
                end
            end
            PREFIX_APPLY: begin event_index<=event_index+1;phase<=PREFIX;end
            SEED: if(op==11) begin
                    if(context_kind==2 && !view_channels[VB'(context_id)][field[12:9]]) begin result<=0;phase<=REPLY;end
                    else read_word(state_address(context_kind,context_id,field),SEEDED);
                end else if(context_kind==1 && binding_valid!=0) fail(5);
                else if(context_kind==2 && !view_channels[VB'(context_id)][field[12:9]]) begin
                    local_field<=0;view_channels[VB'(context_id)][field[12:9]]<=1;phase<=SEED_CLEAR;
                end else begin
                    if(context_kind==0) input_channels[IB'(context_id)]<=input_channels[IB'(context_id)]|(16'b1<<field[12:9]);
                    if(context_kind==2) view_channels[VB'(context_id)]<=view_channels[VB'(context_id)]|(16'b1<<field[12:9]);
                    write_word(state_address(context_kind,context_id,field),value,REPLY);
                end
            SEED_CLEAR: write_word(state_address(2,context_id,{field[12:9],local_field}),0,SEED_CLEARED);
            SEED_CLEARED: if(local_field==257) write_word(state_address(2,context_id,field),value,REPLY);else begin local_field<=local_field+1;phase<=SEED_CLEAR;end
            SEEDED: begin result<=data;phase<=REPLY;end
            BIND: if(scan==32'(LOOPS)) phase<=REPLY;else if(binding_valid[LB'(scan)]) begin seq<=scan;work_view<=bound_view[LB'(scan)];output_id<=0;scan<=scan+1;phase<=REBIND_NEXT;end else scan<=scan+1;
            RESTORE: begin output_id<=0;phase<=RESTORE_OUTPUT;end
            RESTORE_OUTPUT: if(output_id>=32'(DESTINATIONS)) phase<=REPLY;
                else if(!bound_outputs[LB'(seq)][output_id]) output_id<=output_id+1;
                else begin destination<=output_id[15:0];restore_channels<=bound_channels[LB'(seq)];channel<=0;
                    tracker<=seek_target ? 32'(TRACKERS) : seq*32'(DESTINATIONS)+output_id;
                    if(seek_target) begin clear_index<=0;refresh_field<=0;phase<=TRACK_CLEAR;end
                    else if(nonempty[TB'(seq*32'(DESTINATIONS)+output_id)]) phase<=RESTORE_CHANNEL;
                    else output_id<=output_id+1;
                end
            RESTORE_CHANNEL: begin
                tracker<=seek_target ? 32'(TRACKERS) : seq*32'(DESTINATIONS)+32'(destination);
                if(!restore_channels[channel]) begin if(channel==15) begin output_id<=32'(destination)+1;phase<=RESTORE_OUTPUT;end else channel<=channel+1;end
                else read_word(arena_base+TRACK_BASE+(64'(seek_target ? 32'(TRACKERS) : 32'(seq*32'(DESTINATIONS)+32'(destination)))<<11)+1024+{59'b0,channel[3:1],2'b0},RESTORE_SUMMARY);
            end
            RESTORE_SUMMARY: if((channel[0] ? data[31:16] : data[15:0])==0) begin
                    if(channel==15) begin output_id<=32'(destination)+1;phase<=RESTORE_OUTPUT;end else begin channel<=channel+1;phase<=RESTORE_CHANNEL;end
                end else begin dirty_summary<=data;restore_word<=0;phase<=RESTORE_WORD;end
            RESTORE_WORD: if(restore_word==9) begin
                    if((dirty_notes[0]|dirty_notes[1]|dirty_notes[2]|dirty_notes[3])!=0 || dirty_controls[2][0] || dirty_controls[2][2]) begin
                        restore_field<=0;any_voice<=0;group_repair<=0;phase<=RESTORE_LOAD;
                    end else begin restore_field<=128;phase<=RESTORE_CONTROLS;end
                end else if(!dirty_summary[(channel[0] ? 5'd16 : 5'd0)+5'(restore_word)]) begin
                    if(restore_word<4) dirty_notes[restore_word[1:0]]<=0;else dirty_controls[restore_word-4]<=0;
                    restore_word<=restore_word+1;
                end else read_word(tracker_address+{54'b0,channel,restore_word,2'b0},RESTORE_WORD_VALUE);
            RESTORE_WORD_VALUE: begin
                if(restore_word<4) dirty_notes[restore_word[1:0]]<=data;else dirty_controls[restore_word-4]<=data;
                restore_word<=restore_word+1;phase<=RESTORE_WORD;
            end
            RESTORE_LOAD: read_word(state_address(1,32'(destination),{channel,restore_field}),RESTORE_LOAD_CURRENT);
            RESTORE_LOAD_CURRENT: begin
                load_current<=data;
                if(!seek_target && !view_channels[VB'(work_view[31:0])][channel]) begin data<=0;phase<=RESTORE_LOADED;end
                else read_word(state_address(seek_target ? 2'd3 : 2'd2,work_view[31:0],{channel,restore_field}),RESTORE_LOADED);
            end
            RESTORE_LOADED: begin
                current_channel[restore_field]<=load_current;target_channel[restore_field]<=data[31] ? data : load_current;
                if(restore_field<128 && (load_current[6:0]!=0 || (data[31] && data[6:0]!=0))) any_voice<=1;
                if(restore_field==192) begin old64<=load_current;new64<=data[31] ? data : load_current;end
                if(restore_field==194) begin old66<=load_current;new66<=data[31] ? data : load_current;end
                if(restore_field==257) begin restore_field<=0;ram_address<=0;phase<=RAM_WAIT;after_ram<=RESTORE_DETECT;end
                else begin restore_field<=restore_field+1;phase<=RESTORE_LOAD;end
            end
            RAM_WAIT: phase<=RAM_READY;
            RAM_READY: phase<=after_ram;
            RESTORE_DETECT: begin
                if(current_q[9:0]!=target_q[9:0] && ((current_q|target_q)&(SUST|SOST))!=0) group_repair<=1;
                if(current_q[9:0]!=target_q[9:0] && (old64[6:0]>=64 || new64[6:0]>=64 || old66[6:0]>=64 || new66[6:0]>=64)) group_repair<=1;
                if(restore_field==127) phase<=RESTORE_EDGE;
                else begin restore_field<=restore_field+1;ram_address<=restore_field+1;phase<=RAM_WAIT;end
            end
            RESTORE_EDGE: begin
                group_repair<=group_repair || (any_voice && ((old64[6:0]>=64)!=(new64[6:0]>=64) || (old66[6:0]>=64)!=(new66[6:0]>=64)));
                restore_field<=0;phase<=RESTORE_GROUP;
            end
            RESTORE_GROUP: if(group_repair) begin restore_field<=192;ram_address<=192;after_ram<=RESTORE_OFF_PEDALS;phase<=RAM_WAIT;end
                else begin restore_field<=0;ram_address<=0;after_ram<=RESTORE_OFF;phase<=RAM_WAIT;end
            RESTORE_OFF_PEDALS: if(current_q[6:0]!=0) send_packet(8'hb0|{4'b0,channel},restore_field==192 ? 7'd64 : 7'd66,0,RESTORE_GROUP_NOTES);
                else phase<=RESTORE_GROUP_NOTES;
            RESTORE_GROUP_NOTES: if(restore_field==192) begin restore_field<=194;ram_address<=194;phase<=RAM_WAIT;after_ram<=RESTORE_OFF_PEDALS;end
                else begin restore_field<=0;ram_address<=0;phase<=RAM_WAIT;after_ram<=RESTORE_OFF;end
            RESTORE_OFF: begin
                if((group_repair && current_q[6:0]!=0) || (!group_repair && dirty_notes[restore_field[6:5]][restore_field[4:0]] &&
                    ((target_q[9:0]!=current_q[9:0] && current_q[6:0]!=0) || (target_q[31] && target_q[6:0]==0 && !current_q[31]))))
                    send_packet(8'h80|{4'b0,channel},restore_field[6:0],0,RESTORE_CLAIM);
                else phase<=RESTORE_CLAIM;
            end
            RESTORE_CLAIM: if(restore_field==127) begin restore_field<=128;phase<=RESTORE_CONTROLS;end
                else begin restore_field<=restore_field+1;ram_address<=restore_field+1;phase<=RAM_WAIT;after_ram<=RESTORE_OFF;end
            RESTORE_CONTROLS: if(restore_field>257) begin
                    if(group_repair) begin restore_field<=0;ram_address<=0;phase<=RAM_WAIT;after_ram<=RESTORE_CAPTURED;end
                    else if((dirty_notes[0]|dirty_notes[1]|dirty_notes[2]|dirty_notes[3])!=0 || dirty_controls[2][0] || dirty_controls[2][2]) begin restore_field<=194;ram_address<=194;phase<=RAM_WAIT;after_ram<=RESTORE_SOST;end
                    else begin
                        if(channel==15) begin output_id<=32'(destination)+1;phase<=RESTORE_OUTPUT;end else begin channel<=channel+1;phase<=RESTORE_CHANNEL;end
                    end
                end else if(restore_field==194 || !managed(32'(destination),{channel,restore_field}) || (!group_repair && !dirty_controls[(restore_field-128)>>5][restore_field[4:0]])) restore_field<=restore_field+1;
                else begin
                    if((dirty_notes[0]|dirty_notes[1]|dirty_notes[2]|dirty_notes[3])!=0 || dirty_controls[2][0] || dirty_controls[2][2]) begin ram_address<=restore_field;phase<=RAM_WAIT;after_ram<=FAST_VALUE;end
                    else read_word(state_address(seek_target ? 2'd3 : 2'd2,work_view[31:0],{channel,restore_field}),FAST_TARGET);
                end
            FAST_TARGET: begin value<=data;read_word(state_address(1,32'(destination),{channel,restore_field}),FAST_VALUE);end
            FAST_VALUE: begin
                if((dirty_notes[0]|dirty_notes[1]|dirty_notes[2]|dirty_notes[3])!=0 || dirty_controls[2][0] || dirty_controls[2][2]) begin
                    if(different(target_q,current_q,{channel,restore_field})) begin
                        if(restore_field==256) send_packet(8'he0|{4'b0,channel},target_q[6:0],target_q[13:7],RESTORE_CONTROLS);
                        else if(restore_field==257) send_packet(8'hd0|{4'b0,channel},target_q[6:0],0,RESTORE_CONTROLS);
                        else send_packet(8'hb0|{4'b0,channel},restore_field[6:0],target_q[6:0],RESTORE_CONTROLS);
                    end else phase<=RESTORE_CONTROLS;
                end else if(different(value,data,{channel,restore_field})) begin
                    if(restore_field==256) send_packet(8'he0|{4'b0,channel},value[6:0],value[13:7],RESTORE_CONTROLS);
                    else if(restore_field==257) send_packet(8'hd0|{4'b0,channel},value[6:0],0,RESTORE_CONTROLS);
                    else send_packet(8'hb0|{4'b0,channel},restore_field[6:0],value[6:0],RESTORE_CONTROLS);
                end else phase<=RESTORE_CONTROLS;
                restore_field<=restore_field+1;
            end
            RESTORE_CAPTURED: if(restore_field==128) begin restore_field<=194;ram_address<=194;phase<=RAM_WAIT;after_ram<=RESTORE_SOST;end
                else begin
                    if(target_q[6:0]!=0 && target_q[8]) send_packet(8'h90|{4'b0,channel},restore_field[6:0],target_q[6:0],RESTORE_CAPTURED);
                    else begin phase<=RAM_WAIT;after_ram<=RESTORE_CAPTURED;end
                    restore_field<=restore_field+1;ram_address<=restore_field+1;
                end
            RESTORE_SOST: begin
                if(different(target_q,current_q,{channel,9'd194})) send_packet(8'hb0|{4'b0,channel},66,target_q[6:0],RESTORE_OTHER);
                else begin phase<=RAM_WAIT;after_ram<=RESTORE_OTHER;end
                restore_field<=0;ram_address<=0;
            end
            RESTORE_OTHER: if(restore_field==128) begin restore_field<=0;ram_address<=0;phase<=RAM_WAIT;after_ram<=RESTORE_RELEASE;end
                else begin
                    if(target_q[6:0]!=0 && (group_repair ? !target_q[8] : dirty_notes[restore_field[6:5]][restore_field[4:0]] && target_q[9:0]!=current_q[9:0]))
                        send_packet(8'h90|{4'b0,channel},restore_field[6:0],target_q[6:0],RESTORE_OTHER);
                    else begin phase<=RAM_WAIT;after_ram<=RESTORE_OTHER;end
                    restore_field<=restore_field+1;ram_address<=restore_field+1;
                end
            RESTORE_RELEASE: if(restore_field==128 || !group_repair) begin restore_field<=0;ram_address<=0;phase<=RAM_WAIT;after_ram<=RESTORE_PRESSURE;end
                else begin
                    if(target_q[6:0]!=0 && !target_q[7]) send_packet(8'h80|{4'b0,channel},restore_field[6:0],0,RESTORE_RELEASE);
                    else begin phase<=RAM_WAIT;after_ram<=RESTORE_RELEASE;end
                    restore_field<=restore_field+1;ram_address<=restore_field+1;
                end
            RESTORE_PRESSURE: if(restore_field==128) begin
                    if(channel==15) begin output_id<=32'(destination)+1;phase<=RESTORE_OUTPUT;end else begin channel<=channel+1;phase<=RESTORE_CHANNEL;end
                end else begin
                    if((group_repair || dirty_notes[restore_field[6:5]][restore_field[4:0]]) && target_q[6:0]!=0 && target_q[23] && (!current_q[23] || target_q[22:16]!=current_q[22:16]))
                        send_packet(8'ha0|{4'b0,channel},restore_field[6:0],target_q[22:16],RESTORE_PRESSURE);
                    else begin phase<=RAM_WAIT;after_ram<=RESTORE_PRESSURE;end
                    restore_field<=restore_field+1;ram_address<=restore_field+1;
                end
            SEND: if(packet_ready) begin if(packet_refused) fail(9);else begin owner_seq<=seq;apply_packet(1,32'(destination),packet,SENT);end end
            SENT: begin
                if(after_send==RESTORE_CAPTURED || after_send==RESTORE_OTHER || after_send==RESTORE_RELEASE || after_send==RESTORE_PRESSURE) begin phase<=RAM_WAIT;after_ram<=after_send;end
                else phase<=after_send;
            end
            CLEANUP: begin output_id<=0;phase<=CLEANUP_OUTPUT;end
            CLEANUP_OUTPUT: if(output_id>=32'(DESTINATIONS)) phase<=REPLY;
                else if(!bound_outputs[LB'(seq)][output_id]) output_id<=output_id+1;
                else begin destination<=output_id[15:0];cleanup_tracker<=seq*32'(DESTINATIONS)+output_id;cleanup_channel<=0;cleanup_word<=4;phase<=CLEANUP_CHANNEL;end
            CLEANUP_CHANNEL: read_word(activity_address(cleanup_tracker,cleanup_channel,cleanup_word),CLEANUP_WORD);
            CLEANUP_WORD: begin cleanup_bits<=data;if(cleanup_word==4) begin activity_key<=0;phase<=CLEANUP_PEDAL;end else phase<=CLEANUP_KEYS;end
            CLEANUP_PEDAL: if(activity_key>=2) write_word(activity_address(cleanup_tracker,cleanup_channel,4),0,CLEANUP_ADVANCE);
                else if(!cleanup_bits[{4'b0,activity_key[0]}]) activity_key<=activity_key+1;
                else begin restore_field<=activity_key==0 ? 192 : 194;channel<=cleanup_channel;read_word(state_address(1,32'(destination),{cleanup_channel,(activity_key==0 ? 9'd192 : 9'd194)}),CLEANUP_NOTE);end
            CLEANUP_KEYS: if(cleanup_bits==0) write_word(activity_address(cleanup_tracker,cleanup_channel,cleanup_word),0,CLEANUP_ADVANCE);
                else begin
                    for(integer i=31;i>=0;i=i-1) if(cleanup_bits[i]) begin activity_key<={cleanup_word[1:0],5'(i)};restore_field<={2'b0,cleanup_word[1:0],5'(i)};end
                    phase<=CLEANUP_LOAD;
                end
            CLEANUP_ADVANCE: if(cleanup_word==4) begin cleanup_word<=0;phase<=CLEANUP_CHANNEL;end
                else begin
                    if(cleanup_word==3) begin
                        if(cleanup_channel==15) begin output_id<=32'(destination)+1;phase<=CLEANUP_OUTPUT;end
                        else begin cleanup_channel<=cleanup_channel+1;cleanup_word<=4;phase<=CLEANUP_CHANNEL;end
                    end else begin cleanup_word<=cleanup_word+1;phase<=CLEANUP_CHANNEL;end
                end
            CLEANUP_LOAD: read_word(state_address(1,32'(destination),{cleanup_channel,restore_field}),CLEANUP_NOTE);
            CLEANUP_NOTE: if(cleanup_word==4) begin
                    activity_key<=activity_key+1;
                    if(data[6:0]>=64) send_packet(8'hb0|{4'b0,cleanup_channel},restore_field[6:0],0,CLEANUP_PEDAL);else phase<=CLEANUP_PEDAL;
                end else begin
                    cleanup_bits[activity_key[4:0]]<=0;
                    if(data[6:0]!=0) send_packet(8'h80|{4'b0,cleanup_channel},activity_key,0,CLEANUP_KEYS);else phase<=CLEANUP_KEYS;
                end
            REPLY: phase<=IDLE;
            default: fail(4);
        endcase
    end
endmodule
