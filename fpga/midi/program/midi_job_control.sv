module midi_job_control #(
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8,
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, RETIREMENTS=256,
    parameter integer INPUT_EVENTS=256, MAINTENANCE=64,
    parameter integer TABLE_ENTRIES=(MAX_EVENTS+2*BLOCK_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1
) (
    input wire clk, input wire reset, input wire [63:0] arena_base, input wire [31:0] quantum_frames,
    output wire [63:0] arena_bytes,
    input wire command_valid, output wire command_ready, input wire [5:0] command_op,
    input wire [63:0] command_arg0, input wire [63:0] command_arg1, input wire [63:0] command_arg2,
    input wire [63:0] command_arg3, input wire [63:0] command_arg4,
    output wire response_valid, input wire response_ready, output reg [3:0] response_error,
    output reg [63:0] response_handle, output reg [63:0] response_frame,
    output reg [31:0] response_count,
    output reg [23:0] response_packet, output reg [1:0] response_length,
    output wire packet_valid, input wire packet_ready, input wire packet_refused,
    output wire [23:0] packet_data, output wire [1:0] packet_length,
    output reg [31:0] packet_destination, output reg [31:0] packet_offset,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    `include "midi_functions.svh"
    initial begin
        if(LOOPS<1 || LOOPS>128 || INPUTS<1 || DESTINATIONS<1 || INPUT_EVENTS<1 || MAINTENANCE<0)
            $fatal(1,"Invalid MIDI runtime capacities");
    end
    localparam [63:0] CONTENT_BYTES=64'(VIEWS)*64+64'(VIEWS)*64'(TABLE_ENTRIES)*4+64'(BLOCKS)*8+
        64'(BLOCKS)*64'(BLOCK_EVENTS)*16+64'(RETIREMENTS)*8;
    localparam [63:0] CONTEXT_OFFSET=CONTENT_BYTES, HISTORY_OFFSET=CONTEXT_OFFSET+64'(LOOPS)*128;
    assign arena_bytes=HISTORY_OFFSET+64'(INPUTS)*96;
    initial if(HISTORY_OFFSET+64'(INPUTS)*96>64'h100000000) $fatal(1,"MIDI arena offsets exceed 32 bits");
    reg [15:0] pc,service_return,memory_return,context_return,history_return,search_return,event_return,head_return,range_return,packet_return;
    reg [5:0] op;
    reg prepared,running,seen_quantum;
    reg [63:0] a0,a1,a2,a3,a4,next_frame,start_frame,frames,event_base;
    reg [31:0] context_slot,input_slot,scan,cursor,index,event_count,event_index,data_word,c[0:31],h[0:23];
    reg [63:0] own_address;
    reg [31:0] own_data,density[0:INPUTS-1];
    reg [31:0] transfer_events,transfer_metadata,publication_bindings;
    reg [31:0] event_input,event_offset,last_input_offset,trim_order;
    reg [23:0] event_packet,send_packet;
    reg [1:0] event_length,send_length;
    reg [31:0] low,high,middle,range_index;
    reg [63:0] query_frame,range_start,range_end,range_view,pending_writer;
    reg [31:0] range_input,target_count,target_index;
    reg [63:0] target_list;
    reg [127:0] target_seen;
    reg [31:0] reserved_blocks,reservation_scan,reservation_saved_scan,reservation_context,import_needed;
    reg [15:0] reservation_return,alignment_return;
    reg [63:0] alignment_value,alignment_remainder;
    reg [5:0] alignment_bit;
    reg [3:0] store_op;
    reg [63:0] store_view,store_arg0,store_arg1;
    reg [23:0] store_packet;
    reg [1:0] store_length;
    reg [2:0] last_error;
    reg [63:0] last_handle,last_frame;
    reg [31:0] last_events,last_status,last_order,last_blocks,last_owner,last_references;
    reg [23:0] last_packet;
    reg [1:0] last_length;
    localparam integer LOOP_BITS=LOOPS>1 ? $clog2(LOOPS) : 1, INPUT_BITS=INPUTS>1 ? $clog2(INPUTS) : 1;
    localparam integer DEST_BITS=DESTINATIONS>1 ? $clog2(DESTINATIONS) : 1,HEAD_BITS=59+DEST_BITS;
    reg [HEAD_BITS-1:0] head_word;
    reg head_write;
    reg [LOOP_BITS-1:0] head_write_index;
    reg [HEAD_BITS-1:0] head_write_data;
    reg [31:0] best_frame,best_slot;
    reg [23:0] best_packet;
    reg [1:0] best_length;
    reg found;
    wire store_ready,store_valid;
    wire [2:0] store_error;
    wire [63:0] store_handle,store_frame;
    wire [31:0] store_events,store_status,store_order,store_work,store_blocks,store_owner,store_references;
    wire [31:0] free_blocks,free_views,free_retirements;
    wire [23:0] store_record_packet;
    wire [1:0] store_record_length;
    localparam [15:0] IDLE=0,
        RESPONSE=1,
        READ_REQUEST=2,
        READ_WAIT=3,
        WRITE_REQUEST=4,
        WRITE_WAIT=5,
        STORE_START=6,
        STORE_WAIT=7,
        CONTEXT_READ=8,
        CONTEXT_READ_NEXT=9,
        CONTEXT_WRITE=10,
        CONTEXT_WRITE_NEXT=11,
        HISTORY_READ=12,
        HISTORY_READ_NEXT=13,
        HISTORY_WRITE=14,
        HISTORY_WRITE_NEXT=15,
        SEARCH=16,
        SEARCH_COMPARE=17,
        JOB_BEGIN_CLEAR=18,
        JOB_CONTEXT=19,
        JOB_EXECUTE=20,
        JOB_APPEND=21,
        JOB_APPEND_EVENT=22,
        JOB_APPENDED=23,
        JOB_APPEND_ADVANCE=24,
        JOB_READ_DONE=25,
        JOB_HEAD=26,
        JOB_HEAD_RECORD=27,
        JOB_SELECT_BEGIN=28,
        JOB_SELECT=29,
        JOB_SELECT_COMPARE=30,
        DISPATCH=31,
        BOOT_CONTEXTS=32,
        BOOT_CONTEXT=33,
        BOOT_CONTEXT_NEXT=34,
        BOOT_HISTORIES=35,
        BOOT_HISTORY=36,
        BOOT_HISTORY_WRITER=37,
        BOOT_HISTORY_OWNED=38,
        BOOT_HISTORY_NEXT=39,
        CONTROL_CONTEXT=40,
        CONTROL_DUPLICATE=41,
        UNBOUND=42,
        CONTROL_EXPORTED=43,
        CONTROL_EXPORT_DESCRIPTION=44,
        CONTROL_RELEASED=45,
        EXPORT_READ=46,
        CONTROL_SENT=47,
        CONTROL_RETENTION=48,
        IMPORT_WRITER=49,
        IMPORT_APPEND=50,
        IMPORT_APPENDED=51,
        IMPORT_FINISH=52,
        IMPORT_FINISHED=53,
        CONTROL_REPLACE=54,
        CONTROL_REPLACE_DESCRIPTION=55,
        CONTROL_REPLACE_ALIGNED=56,
        CONTROL_REPLACE_RETAINED=57,
        CONTROL_REPLACE_OLD=58,
        CONTROL_REPLACE_WRITER=59,
        CONTROL_RANGE=60,
        GRAB_ALIGNED=61,
        GRAB_VALIDATE=62,
        GRAB_VALIDATE_SLOT=63,
        GRAB_VALIDATE_CONTEXT=64,
        RANGE_CREATED=65,
        RANGE_OWNED=66,
        GRAB_RETAIN=67,
        GRAB_RETAINED=68,
        GRAB_PUBLISH=69,
        GRAB_PUBLISH_SLOT=70,
        GRAB_PUBLISH_CONTEXT=71,
        GRAB_PUBLISH_OLD=72,
        GRAB_PUBLISH_WRITER=73,
        GRAB_PUBLISH_NEXT=74,
        VALIDATE_INPUTS_CLEAR=75,
        VALIDATE_INPUT=76,
        EVENT_READ=77,
        EVENT_READ_NEXT=78,
        VALIDATE_EVENT=79,
        PROCESS_RECORD_CONTEXT=80,
        PROCESS_RECORD_FINISHED=81,
        PROCESS_RECORD_OWNER_RELEASED=82,
        PROCESS_RECORD_REPLACED=83,
        PROCESS_RECORD_BEGIN=84,
        PROCESS_RECORD_WRITER=85,
        PROCESS_RECORD_WRITER_OWNED=86,
        PROCESS_RECORD_WRITER_OLD=87,
        PROCESS_RECORD_NEXT=88,
        PROCESS_INPUT=89,
        PROCESS_INPUT_EVENT=90,
        PROCESS_INPUT_HISTORY=91,
        PROCESS_INPUT_CAPTURED=92,
        PROCESS_INPUT_HISTORY_SAVED=93,
        PROCESS_INPUT_RECORDS_START=94,
        PROCESS_ADVANCE_HISTORY=95,
        PROCESS_ADVANCE_HISTORY_LOADED=96,
        PROCESS_HISTORY_ADVANCED=97,
        PROCESS_HISTORY_TRIM=98,
        PROCESS_HISTORY_TRIM_INDEX=99,
        PROCESS_HISTORY_DISCARD=100,
        PROCESS_HISTORY_DISCARDED=101,
        PROCESS_ADVANCE_HISTORY_NEXT=102,
        PROCESS_RECORD_ADVANCED=103,
        PROCESS_CLEANUP_LOADED=104,
        PROCESS_CLEANUP_RELEASED=105,
        PROCESS_CLEANUP_NEXT=106,
        PROCESS_PLAY_LOADED=107,
        PROCESS_PLAY_DESCRIPTION=108,
        PROCESS_PLAY_PINNED=109,
        PROCESS_PLAY_NEXT=110,
        PROCESS_EMIT=111,
        PROCESS_EMITTED=112,
        PROCESS_RESCAN=113,
        PROCESS_FINISH_LOADED=114,
        PROCESS_FINISH_NEXT=115,
        PROCESS_FINISHED=116,
        BIND_UNIQUE=117,
        BIND_UNIQUE_CHECK=118,
        BIND_COMMIT=119,
        RESERVE_HISTORY=120,
        RESERVE_HISTORY_LOADED=121,
        RESERVE_HISTORY_VIEW=122,
        RESERVE_RECORD=123,
        RESERVE_RECORD_LOADED=124,
        RESERVE_RECORD_VIEW=125,
        IMPORT_RESERVED=126,
        IMPORT_APPEND_CHECK=127,
        IMPORT_APPEND_RESERVED=128,
        PROCESS_RECORD_RESERVED=129,
        PROCESS_RECORD_RESERVATION=130,
        CONTROL_MAINTAINED=131,
        ALIGN_DURATION=132,
        GROUP_DESCRIPTION=133,
        GROUP_START=134,
        EMIT_PACKET=135;
    assign command_ready=pc==IDLE;
    assign response_valid=pc==RESPONSE;
    assign packet_valid=pc==EMIT_PACKET;
    assign packet_data=send_packet;
    assign packet_length=send_length;
    wire store_memory_valid,store_memory_ready,store_memory_write,store_memory_response_valid,store_memory_response_ready;
    wire [63:0] store_memory_address;
    wire [31:0] store_memory_write_data;
    wire select_store=pc==STORE_START || pc==STORE_WAIT;
    assign memory_valid=select_store ? store_memory_valid : (pc==READ_REQUEST || pc==WRITE_REQUEST) && !reset;
    assign memory_write=select_store ? store_memory_write : pc==WRITE_REQUEST;
    assign memory_address=select_store ? store_memory_address : own_address;
    assign memory_write_data=select_store ? store_memory_write_data : own_data;
    assign memory_response_ready=select_store ? store_memory_response_ready : pc==READ_WAIT || pc==WRITE_WAIT;
    assign store_memory_ready=select_store && memory_ready;
    assign store_memory_response_valid=select_store && memory_response_valid;
    midi_store #(.BLOCK_EVENTS(BLOCK_EVENTS),.BLOCKS(BLOCKS),.MAX_EVENTS(MAX_EVENTS),.VIEWS(VIEWS),
        .RETIREMENTS(RETIREMENTS),.TABLE_ENTRIES(TABLE_ENTRIES)) content (
        .clk(clk),.reset(reset),.arena_base(arena_base),.arena_bytes(),
        .command_valid(pc==STORE_START),.command_ready(store_ready),.command_op(store_op),
        .command_view(store_view),.command_arg0(store_arg0),.command_arg1(store_arg1),
        .command_packet(store_packet),.command_length(store_length),
        .response_valid(store_valid),.response_ready(pc==STORE_WAIT),.response_error(store_error),
        .response_handle(store_handle),.response_frame(store_frame),.response_order(store_order),
        .response_packet(store_record_packet),.response_length(store_record_length),.response_events(store_events),
        .response_status(store_status),.response_work(store_work),.response_blocks(store_blocks),
        .response_owner(store_owner),.response_references(store_references),.free_block_slots(free_blocks),
        .free_view_slots(free_views),.free_retirement_slots(free_retirements),
        .memory_valid(store_memory_valid),.memory_ready(store_memory_ready),.memory_write(store_memory_write),
        .memory_address(store_memory_address),.memory_write_data(store_memory_write_data),
        .memory_response_valid(store_memory_response_valid),.memory_response_ready(store_memory_response_ready),
        .memory_response_error(memory_response_error),.memory_read_data(memory_read_data)
    );
    function automatic [63:0] context_address(input [31:0] slot);
        reg [31:0] offset;
        begin offset=32'(CONTEXT_OFFSET)+32'(LOOP_BITS'(slot))*128;context_address=arena_base+{32'b0,offset};end
    endfunction
    function automatic [63:0] history_address(input [31:0] slot);
        reg [31:0] offset;
        begin offset=32'(HISTORY_OFFSET)+32'(INPUT_BITS'(slot))*96;history_address=arena_base+{32'b0,offset};end
    endfunction
    function automatic [31:0] first_set_bit(input [31:0] bits);
        begin
            first_set_bit=0;
            for(integer n=31;n>=0;n=n-1) if(bits[n]) first_set_bit=32'(n);
        end
    endfunction
    task automatic fail(input [3:0] error_code);
        begin if(response_error==0) response_error<=4'(error_code);pc<=RESPONSE;end
    endtask
    task automatic memory_fault;
        begin
            prepared<=0;
            fail(7);response_error<=4'(7);
        end
    endtask
    task automatic read_word(input [63:0] address,input [15:0] next);
        begin own_address<=address;memory_return<=next;pc<=READ_REQUEST;end
    endtask
    task automatic write_word(input [63:0] address,input [31:0] value,input [15:0] next);
        begin own_address<=address;own_data<=value;memory_return<=next;pc<=WRITE_REQUEST;end
    endtask
    task automatic call_store(input [3:0] code,input [63:0] view,input [63:0] x,input [63:0] y,input [15:0] next);
        begin store_op<=code;store_view<=view;store_arg0<=x;store_arg1<=y;
            store_packet<=event_packet;store_length<=event_length;service_return<=next;pc<=STORE_START;end
    endtask
    task automatic load_context(input [31:0] slot,input [15:0] next);
        begin context_slot<=slot;cursor<=0;context_return<=next;pc<=CONTEXT_READ;end
    endtask
    task automatic save_context(input [15:0] next);
        begin cursor<=0;context_return<=next;pc<=CONTEXT_WRITE;end
    endtask
    task automatic load_history(input [31:0] slot,input [15:0] next);
        begin input_slot<=slot;cursor<=0;history_return<=next;pc<=HISTORY_READ;end
    endtask
    task automatic save_history(input [15:0] next);
        begin cursor<=0;history_return<=next;pc<=HISTORY_WRITE;end
    endtask
    task automatic aligned_duration(input [63:0] duration,input [15:0] next);
        begin alignment_value<=duration;alignment_remainder<=0;alignment_bit<=63;alignment_return<=next;pc<=ALIGN_DURATION;end
    endtask
    task automatic lower_bound(input [63:0] frame,input [15:0] next);
        begin query_frame<=frame;low<=0;high<=last_events;search_return<=next;pc<=SEARCH;end
    endtask
    task automatic reserve_capture(input [15:0] next);
        begin reserved_blocks<=2;reservation_scan<=0;reservation_return<=next;pc<=RESERVE_HISTORY;end
    endtask
    always @(posedge clk) begin : execution
        reg [63:0] x,y,z;
        reg [31:0] value;
        head_write<=0;
        if(reset) begin
            pc<=IDLE;prepared<=0;running<=0;seen_quantum<=0;
            response_error<=0;response_handle<=0;response_frame<=0;response_count<=0;response_packet<=0;response_length<=0;
            transfer_events<=32'hffffffff;transfer_metadata<=32'hffffffff;publication_bindings<=32'hffffffff;
        end else case(pc)
            IDLE: if(command_valid) begin
                op<=command_op;a0<=command_arg0;a1<=command_arg1;a2<=command_arg2;a3<=command_arg3;a4<=command_arg4;
                response_error<=4'(0);response_count<=0;response_length<=0;last_error<=0;pc<=DISPATCH;
            end
            RESPONSE: if(response_ready) pc<=IDLE;
            READ_REQUEST: if(memory_ready) pc<=READ_WAIT;
            READ_WAIT: if(memory_response_valid) begin
                if(memory_response_error) memory_fault();
                else begin data_word<=memory_read_data;pc<=memory_return;end
            end
            WRITE_REQUEST: if(memory_ready) pc<=WRITE_WAIT;
            WRITE_WAIT: if(memory_response_valid) begin
                if(memory_response_error) memory_fault(); else pc<=memory_return;
            end
            STORE_START: if(store_ready) pc<=STORE_WAIT;
            STORE_WAIT: if(store_valid) begin
                last_error<=store_error;last_handle<=store_handle;last_frame<=store_frame;
                last_events<=store_events;last_blocks<=store_blocks;last_owner<=store_owner;last_references<=store_references;last_status<=store_status;last_order<=store_order;
                last_packet<=store_record_packet;last_length<=store_record_length;pc<=service_return;
                if(store_error==7) memory_fault();
            end
            CONTEXT_READ: read_word(context_address(context_slot)+64'(cursor)*4,CONTEXT_READ_NEXT);
            CONTEXT_READ_NEXT: begin c[cursor[4:0]]<=data_word;
                if(cursor==31) pc<=context_return;else begin cursor<=cursor+1;pc<=CONTEXT_READ;end end
            CONTEXT_WRITE: write_word(context_address(context_slot)+64'(cursor)*4,c[cursor[4:0]],CONTEXT_WRITE_NEXT);
            CONTEXT_WRITE_NEXT: if(cursor==31) pc<=context_return;else begin cursor<=cursor+1;pc<=CONTEXT_WRITE;end
            HISTORY_READ: read_word(history_address(input_slot)+64'(cursor)*4,HISTORY_READ_NEXT);
            HISTORY_READ_NEXT: begin h[cursor[4:0]]<=data_word;
                if(cursor==23) pc<=history_return;else begin cursor<=cursor+1;pc<=HISTORY_READ;end end
            HISTORY_WRITE: write_word(history_address(input_slot)+64'(cursor)*4,h[cursor[4:0]],HISTORY_WRITE_NEXT);
            HISTORY_WRITE_NEXT: if(cursor==23) pc<=history_return;else begin cursor<=cursor+1;pc<=HISTORY_WRITE;end
            SEARCH: if(low==high) begin range_index<=low;pc<=search_return;end
                else begin middle<=low+(high-low)/2;call_store(10,{h[1],h[0]},{32'b0,low+(high-low)/2},0,SEARCH_COMPARE);end
            SEARCH_COMPARE: if(last_error!=0) fail({1'b0,last_error});
                else begin if(last_frame<query_frame) low<=middle+1;else high<=middle;pc<=SEARCH;end

            JOB_BEGIN_CLEAR: if(scan==32'(LOOPS)) begin scan<=0;pc<=RESPONSE;end
                else begin begin head_write<=1;head_write_index<=LOOP_BITS'(scan);head_write_data<=0;end scan<=scan+1;end
            JOB_CONTEXT: if(c[0]==0 || {c[5],c[4]}!=a4) fail(4);else pc<=JOB_EXECUTE;
            JOB_EXECUTE: case(op)
                25: if(c[16]==1 && {c[9],c[8]}==0) fail(8);
                    else begin c[17]<=0;pc<=PROCESS_RECORD_CONTEXT;end
                26: begin c[17]<=1;c[22]<=1;pc<=PROCESS_RECORD_BEGIN;end
                28: begin event_index<=0;pc<=JOB_APPEND;end
                29: begin c[17]<=a3[1:0]==1 ? 2 : a3[1:0]==2 ? 1 : 0;
                    c[18]<=a1[31:0];c[19]<=a1[63:32];c[20]<=a2[31:0];c[21]<=a2[63:32];
                    c[22]<={31'b0,a3[2]};pc<=PROCESS_CLEANUP_LOADED;end
                30: pc<=RESPONSE;
                31: begin c[17]<=2;c[18]<=a1[31:0];c[19]<=a1[63:32];c[20]<=a2[31:0];c[21]<=a2[63:32];pc<=PROCESS_PLAY_LOADED;end
                33: begin c[17]<=a1==1 ? 2 : a1==2 ? 1 : 0;pc<=PROCESS_FINISH_LOADED;end
                default: fail(4);
            endcase
            JOB_APPEND: if(c[13]!=1 || {c[9],c[8]}==0) pc<=RESPONSE;
                else if(event_index==event_count) pc<=JOB_APPEND_ADVANCE;
                else begin cursor<=0;event_return<=JOB_APPEND_EVENT;pc<=EVENT_READ;end
            JOB_APPEND_EVENT: if(event_input!=c[1] || (event_packet[7:0]!=8'hff && !c[3][{1'b0,event_packet[3:0]}])) begin event_index<=event_index+1;pc<=JOB_APPEND;end
                else call_store(5,{c[9],c[8]},{c[25],c[24]}+64'(event_offset),0,JOB_APPENDED);
            JOB_APPENDED: if(last_error!=0) begin c[13]<=3;if(response_error==0) response_error<=4'(last_error);save_context(RESPONSE);end
                else begin event_index<=event_index+1;pc<=JOB_APPEND;end
            JOB_APPEND_ADVANCE: call_store(13,{c[9],c[8]},{c[25],c[24]}+frames,0,PROCESS_RECORD_ADVANCED);
            JOB_READ_DONE: begin
                head_return<=RESPONSE;
                if(c[23]==0 && {c[11],c[10]}!=0 && {c[15],c[14]}!={c[19],c[18]}) begin
                    if(response_error==0) response_error<=4'(2);begin head_write<=1;head_write_index<=LOOP_BITS'(context_slot);head_write_data<=0;end save_context(RESPONSE);
                end else pc<=JOB_HEAD;
            end
            JOB_HEAD: if(c[12]>=c[29] || c[0]==0 || c[23]!=0 || c[17]!=2 ||
                    {c[11],c[10]}==0 || {c[19],c[18]}>={c[21],c[20]}) begin begin head_write<=1;head_write_index<=LOOP_BITS'(context_slot);head_write_data<=0;end save_context(head_return);end
                else call_store(10,{c[11],c[10]},{32'b0,c[12]},0,JOB_HEAD_RECORD);
            JOB_HEAD_RECORD: if(last_error!=0) begin if(response_error==0) response_error<=4'(last_error);begin head_write<=1;head_write_index<=LOOP_BITS'(context_slot);head_write_data<=0;end save_context(head_return);end
                else if(last_length==0 || last_frame>={c[15],c[14]}+frames) begin begin head_write<=1;head_write_index<=LOOP_BITS'(context_slot);head_write_data<=0;end save_context(head_return);end
                else if(last_frame<{c[15],c[14]}) begin c[23]<=1;if(response_error==0) response_error<=4'(4);begin head_write<=1;head_write_index<=LOOP_BITS'(context_slot);head_write_data<=0;end save_context(head_return);end
                else if(last_packet[7:0]!=8'hff && !c[3][{1'b0,last_packet[3:0]}]) begin c[12]<=c[12]+1;pc<=JOB_HEAD;end
                else begin
                    x=last_frame-{c[15],c[14]};begin head_write<=1;head_write_index<=LOOP_BITS'(context_slot);head_write_data<={c[2][DEST_BITS-1:0],1'b1,last_length,last_packet,x[31:0]};end save_context(head_return);
                end
            JOB_SELECT_BEGIN: begin scan<=0;found<=0;pc<=JOB_SELECT;end
            JOB_SELECT: if(scan==32'(LOOPS)) begin
                    if(found) load_context(best_slot,PROCESS_EMIT);else pc<=RESPONSE;
                end else begin pc<=JOB_SELECT_COMPARE;end
            JOB_SELECT_COMPARE: begin
                if(head_word[58] && (!found || head_word[31:0]<best_frame)) begin
                    found<=1;best_frame<=head_word[31:0];best_slot<=scan;best_packet<=head_word[55:32];best_length<=head_word[57:56];
                end
                scan<=scan+1;pc<=JOB_SELECT;
            end
            DISPATCH: if(op==0 && quantum_frames==0) fail(4);
                else if(op==0) begin prepared<=0;running<=0;seen_quantum<=0;call_store(0,0,0,0,BOOT_CONTEXTS);end
                else if(!prepared) fail(4);
                else case(op)
                    1,2,5,6,7,11,21: if(a0>=64'(LOOPS) || (op==6 && a1>=64'(LOOPS))) fail(4);else load_context(a0[31:0],CONTROL_CONTEXT);
                    24: if(a0<{32'b0,quantum_frames} || (seen_quantum && a0-{32'b0,quantum_frames}!=next_frame) || a2>64'(INPUTS)*64'(INPUT_EVENTS)) fail(1);
                        else begin start_frame<=a0-{32'b0,quantum_frames};frames<={32'b0,quantum_frames};event_base<=a1;event_count<=a2[31:0];
                            running<=1;scan<=0;pc<=VALIDATE_INPUTS_CLEAR;end
                    25,26,28,29,30,31,33: if(a0>=64'(LOOPS)) fail(4);else load_context(a0[31:0],JOB_CONTEXT);
                    27: begin event_index<=0;pc<=PROCESS_INPUT;end
                    32: pc<=JOB_SELECT_BEGIN;
                    34: call_store(11,0,64'(MAINTENANCE),0,PROCESS_FINISHED);
                    4,15: if(a0>=64'(INPUTS) || (op==4 && (a4==0 || a4>64'(LOOPS)))) fail(4);
                        else if(a1>=a2) fail(1);
                        else if(transfer_metadata==0 || (op==4 && 64'(publication_bindings)<a4)) fail(5);
                        else begin range_input<=a0[31:0];range_start<=a1;range_end<=a2;target_list<=a3;target_count<=a4[31:0];
                            load_history(a0[31:0],CONTROL_RANGE);end
                    8: if(transfer_metadata==0) fail(5);else reserve_capture(IMPORT_RESERVED);
                    9: if(transfer_events==0) fail(5);else begin event_packet<=a2[23:0];event_length<=a2[25:24];call_store(12,a0,0,0,IMPORT_APPEND_CHECK);end
                    10: call_store(12,a0,0,0,IMPORT_FINISH);
                    12: if(transfer_events==0) fail(5);else call_store(10,a0,a1,0,EXPORT_READ);
                    13: if(transfer_metadata==0) fail(5);else call_store(9,a0,0,0,CONTROL_RELEASED);
                    14: if(a0>=64'(DESTINATIONS)) fail(4);
                        else if(a2[63:32]!=0) fail(1);
                        else if(packet_error(a1[23:0],a1[25:24])!=0) fail({1'b0,packet_error(a1[23:0],a1[25:24])});else begin packet_destination<=a0[31:0];packet_offset<=a2[31:0];
                        send_packet<=a1[23:0];send_length<=a1[25:24];packet_return<=CONTROL_SENT;pc<=EMIT_PACKET;end
                    16: if(seen_quantum || a0>=64'(INPUTS)) fail(5);else load_history(a0[31:0],CONTROL_RETENTION);
                    17,19: fail(2);
                    18: begin prepared<=0;pc<=RESPONSE;end
                    22: if(a1==0 || a1>64'(LOOPS)) fail(4);
                        else if(publication_bindings<a1[31:0] || free_retirements<a1[31:0]*2) fail(5);
                        else begin range_view<=0;range_start<=0;range_end<=0;target_list<=a0;target_count<=a1[31:0];target_index<=0;target_seen<=0;pc<=GRAB_VALIDATE;end
                    23: if(a2==0 || a2>64'(LOOPS)) fail(4);
                        else if(publication_bindings<a2[31:0] || free_retirements<a2[31:0]*2) fail(5);
                        else begin range_view<=a0;target_list<=a1;target_count<=a2[31:0];target_index<=0;target_seen<=0;
                            call_store(12,a0,0,0,GROUP_DESCRIPTION);end
                    20: call_store(11,0,a0>64 ? 64 : a0,0,CONTROL_MAINTAINED);
                    default: fail(4);
                endcase
            BOOT_CONTEXTS: begin scan<=0;pc<=BOOT_CONTEXT;end
            BOOT_CONTEXT: if(scan==32'(LOOPS)*32) begin scan<=0;pc<=BOOT_HISTORIES;end
                else write_word(arena_base+CONTEXT_OFFSET+64'(scan)*4,0,BOOT_CONTEXT_NEXT);
            BOOT_CONTEXT_NEXT: begin scan<=scan+1;pc<=BOOT_CONTEXT;end
            BOOT_HISTORIES: begin scan<=0;pc<=BOOT_HISTORY;end
            BOOT_HISTORY: if(scan==32'(INPUTS)) begin
                    prepared<=1;running<=0;next_frame<=0;transfer_events<=32'hffffffff;
                    transfer_metadata<=32'hffffffff;publication_bindings<=32'hffffffff;pc<=RESPONSE;
                end else begin input_slot<=scan;call_store(4,0,64'(MAX_EVENTS)+64'(BLOCK_EVENTS)*2,0,BOOT_HISTORY_WRITER);end
            BOOT_HISTORY_WRITER: if(last_error!=0) fail({1'b0,last_error});else begin
                for(integer j=0;j<24;j=j+1) h[j]<=0;
                h[0]<=last_handle[31:0];h[1]<=last_handle[63:32];h[12]<=30*48000;
                write_word(arena_base+64'(last_handle[31:0])*64+60,1,BOOT_HISTORY_OWNED);
            end
            BOOT_HISTORY_OWNED: save_history(BOOT_HISTORY_NEXT);
            BOOT_HISTORY_NEXT: begin scan<=scan+1;pc<=BOOT_HISTORY;end
            CONTROL_CONTEXT: begin
                if(op==1) begin
                    if(c[0]!=0) fail(5);
                    else if(a1[63:32]==0 || a2>=64'(INPUTS) || a3>=64'(DESTINATIONS) || a4==0 || a4>65535) fail(4);
                    else begin scan<=0;pc<=BIND_UNIQUE;end
                end else if(op==2) begin
                    if(c[16]!=0 || {c[9],c[8]}!=0 || {c[11],c[10]}!=0) fail(5);
                    else if({c[7],c[6]}!=0) call_store(9,{c[7],c[6]},0,0,UNBOUND);
                    else begin last_error<=0;pc<=UNBOUND;end
                end else if(c[0]==0) fail(4);
                else if(op==21) begin
                    if(a1==0 || a1>64'(MAX_EVENTS)) fail(3);
                    else if(c[16]!=0 || {c[9],c[8]}!=0) fail(5);else begin c[31]<=a1[31:0];save_context(RESPONSE);end
                end else if(op==11) begin if(transfer_metadata==0) fail(5);else call_store(8,{c[7],c[6]},0,0,CONTROL_EXPORTED);end
                else if(op==6) begin range_view<={c[7],c[6]};load_context(a1[31:0],CONTROL_DUPLICATE);end
                else if(op==5) begin range_view<=a1;pc<=CONTROL_REPLACE;end
                else if(op==7) begin range_view<=0;pc<=CONTROL_REPLACE;end
            end
            CONTROL_DUPLICATE: if(a1>=64'(LOOPS) || c[0]==0 || range_view==0) fail(4);else pc<=CONTROL_REPLACE;
            UNBOUND: if(last_error!=0) fail({1'b0,last_error});
                else begin
                    for(integer field=0;field<32;field=field+1) c[field]<=0;
                    c[31]<=32'(MAX_EVENTS);save_context(RESPONSE);
                end
            CONTROL_EXPORTED: if(last_error!=0) fail({1'b0,last_error});else begin
                transfer_metadata<=transfer_metadata-1;response_handle<=last_handle;range_view<=last_handle;call_store(12,last_handle,0,0,CONTROL_EXPORT_DESCRIPTION);end
            CONTROL_EXPORT_DESCRIPTION: begin response_frame<=last_frame;response_count<=last_events;pc<=RESPONSE;end
            CONTROL_RELEASED: if(last_error!=0) fail({1'b0,last_error});else begin transfer_metadata<=transfer_metadata-1;pc<=RESPONSE;end
            EXPORT_READ: if(last_error!=0) fail({1'b0,last_error});else begin transfer_events<=transfer_events-1;
                response_frame<=last_frame;response_count<=last_order;response_packet<=last_packet;response_length<=last_length;pc<=RESPONSE;end
            CONTROL_SENT: if(packet_refused) fail(3);else pc<=RESPONSE;
            CONTROL_RETENTION: begin h[12]<=a1[31:0];h[13]<=a1[63:32];save_history(RESPONSE);end
            IMPORT_WRITER: if(last_error!=0) fail({1'b0,last_error});else begin transfer_metadata<=transfer_metadata-1;response_handle<=last_handle;pc<=RESPONSE;end
            IMPORT_APPEND: call_store(5,a0,a1,0,IMPORT_APPENDED);
            IMPORT_APPENDED: if(last_error!=0) fail({1'b0,last_error});else begin transfer_events<=transfer_events-1;pc<=RESPONSE;end
            IMPORT_FINISH: if(last_owner!=0) fail(4);else if(last_error!=0) fail({1'b0,last_error});else if(last_status==1) call_store(6,a0,a1,0,IMPORT_FINISHED);
                else if(last_status==2 && last_frame==a1) pc<=IMPORT_FINISHED;else fail(4);
            IMPORT_FINISHED: if(last_error!=0) fail({1'b0,last_error});else begin response_handle<=a0;response_count<=1;pc<=RESPONSE;end
            CONTROL_REPLACE: if(publication_bindings==0 || free_retirements<2) fail(5);
                else if(range_view==0) begin last_frame<=0;pc<=CONTROL_REPLACE_RETAINED;end
                else call_store(12,range_view,0,0,CONTROL_REPLACE_DESCRIPTION);
            CONTROL_REPLACE_DESCRIPTION: if(last_error!=0) fail({1'b0,last_error});
                else if(last_status!=2 || last_frame==0) fail(4);
                else aligned_duration(last_frame,CONTROL_REPLACE_ALIGNED);
            CONTROL_REPLACE_ALIGNED: begin response_frame<=last_frame;call_store(8,range_view,0,0,CONTROL_REPLACE_RETAINED);end
            CONTROL_REPLACE_RETAINED: if(last_error!=0) fail({1'b0,last_error});
                else if({c[7],c[6]}!=0) call_store(9,{c[7],c[6]},0,0,CONTROL_REPLACE_OLD);
                else pc<=CONTROL_REPLACE_OLD;
            CONTROL_REPLACE_OLD: if(last_error!=0) fail({1'b0,last_error});
                else if({c[9],c[8]}!=0) call_store(9,{c[9],c[8]},0,0,CONTROL_REPLACE_WRITER);
                else pc<=CONTROL_REPLACE_WRITER;
            CONTROL_REPLACE_WRITER: if(last_error!=0) fail({1'b0,last_error});else begin
                c[6]<=range_view[31:0];c[7]<=range_view[63:32];c[8]<=0;c[9]<=0;c[13]<=range_view==0 ? 0 : 2;
                c[17]<=0;c[18]<=0;c[19]<=0;
                if(c[16]==1) c[16]<=0;
                if(range_view==0) begin c[20]<=0;c[21]<=0;response_frame<=0;end
                else begin c[20]<=response_frame[31:0];c[21]<=response_frame[63:32];end
                publication_bindings<=publication_bindings-1;save_context(RESPONSE);
            end
            CONTROL_RANGE: if(op==4 && free_retirements<target_count*2) fail(5);
                else if((op==4 && range_start==range_end) || range_start<{h[9],h[8]} || range_end>{h[11],h[10]}) fail(1);
                else if(op==4) aligned_duration(range_end-range_start,GRAB_ALIGNED);
                else call_store(7,{h[1],h[0]},range_start,range_end,RANGE_CREATED);
            GRAB_ALIGNED: begin target_index<=0;target_seen<=0;pc<=GRAB_VALIDATE;end
            GRAB_VALIDATE: if(target_index==target_count) begin
                    if(op==22 || op==23) pc<=GROUP_START;else call_store(7,{h[1],h[0]},range_start,range_end,RANGE_CREATED);end
                else read_word(target_list+64'(target_index)*4,GRAB_VALIDATE_SLOT);
            GRAB_VALIDATE_SLOT: if(data_word>=32'(LOOPS) || target_seen[data_word[6:0]]) fail(4);
                else begin target_seen[data_word[6:0]]<=1;load_context(data_word,GRAB_VALIDATE_CONTEXT);end
            GRAB_VALIDATE_CONTEXT: if(c[0]==0) fail(4);else begin target_index<=target_index+1;pc<=GRAB_VALIDATE;end
            RANGE_CREATED: if(last_error!=0) fail({1'b0,last_error});else begin range_view<=last_handle;pc<=RANGE_OWNED;end
            RANGE_OWNED: begin
                transfer_metadata<=transfer_metadata-1;response_handle<=range_view;response_frame<=range_end-range_start;
                response_count<=last_events;
                if(op==15) pc<=RESPONSE;else begin target_index<=1;pc<=GRAB_RETAIN;end
            end
            GRAB_RETAIN: if(target_index==target_count) begin target_index<=0;pc<=GRAB_PUBLISH;end
                else call_store(8,range_view,0,0,GRAB_RETAINED);
            GRAB_RETAINED: if(last_error!=0) fail({1'b0,last_error});else begin target_index<=target_index+1;pc<=GRAB_RETAIN;end
            GRAB_PUBLISH: if(target_index==target_count) begin publication_bindings<=publication_bindings-target_count;pc<=RESPONSE;end
                else read_word(target_list+64'(target_index)*4,GRAB_PUBLISH_SLOT);
            GRAB_PUBLISH_SLOT: load_context(data_word,GRAB_PUBLISH_CONTEXT);
            GRAB_PUBLISH_CONTEXT: if({c[7],c[6]}!=0) call_store(9,{c[7],c[6]},0,0,GRAB_PUBLISH_OLD);
                else pc<=GRAB_PUBLISH_OLD;
            GRAB_PUBLISH_OLD: if(last_error!=0) fail({1'b0,last_error});
                else if({c[9],c[8]}!=0) call_store(9,{c[9],c[8]},0,0,GRAB_PUBLISH_WRITER);else pc<=GRAB_PUBLISH_WRITER;
            GRAB_PUBLISH_WRITER: if(last_error!=0) fail({1'b0,last_error});else begin
                c[6]<=range_view[31:0];c[7]<=range_view[63:32];c[8]<=0;c[9]<=0;c[13]<=range_view==0 ? 0 : 2;
                c[17]<=0;c[18]<=0;c[19]<=0;
                x=range_end-range_start;c[20]<=x[31:0];c[21]<=x[63:32];if(c[16]==1) c[16]<=0;
                save_context(GRAB_PUBLISH_NEXT);
            end
            GRAB_PUBLISH_NEXT: begin target_index<=target_index+1;pc<=GRAB_PUBLISH;end
            VALIDATE_INPUTS_CLEAR: if(scan==32'(INPUTS)) begin event_index<=0;last_input_offset<=0;pc<=VALIDATE_INPUT;end
                else begin density[scan]<=0;scan<=scan+1;end
            VALIDATE_INPUT: if(event_index==event_count) begin scan<=0;pc<=JOB_BEGIN_CLEAR;end
                else begin cursor<=0;event_return<=VALIDATE_EVENT;pc<=EVENT_READ;end
            EVENT_READ: read_word(event_base+64'(event_index)*16+64'(cursor)*4,EVENT_READ_NEXT);
            EVENT_READ_NEXT: begin
                if(cursor==0) event_input<=data_word;
                else if(cursor==1) event_offset<=data_word;
                else begin event_packet<=data_word[23:0];event_length<=data_word[25:24];end
                if(cursor==2) pc<=event_return;else begin cursor<=cursor+1;pc<=EVENT_READ;end
            end
            VALIDATE_EVENT: if(event_input>=32'(INPUTS) || {32'b0,event_offset}>=frames || event_offset<last_input_offset) fail(1);
                else if(packet_error(event_packet,event_length)!=0) fail({1'b0,packet_error(event_packet,event_length)});
                else if(density[event_input]>=32'(INPUT_EVENTS)) fail(3);
                else begin density[event_input]<=density[event_input]+1;last_input_offset<=event_offset;
                    event_index<=event_index+1;pc<=VALIDATE_INPUT;end
            PROCESS_RECORD_CONTEXT: if(c[0]==0) pc<=PROCESS_RECORD_NEXT;
                else if(c[16]==1 && c[17]!=1 && {c[9],c[8]}!=0) call_store(6,{c[9],c[8]},{c[25],c[24]},0,PROCESS_RECORD_FINISHED);
                else pc<=PROCESS_RECORD_BEGIN;
            PROCESS_RECORD_FINISHED: if(last_error!=0) begin
                    if(response_error==0) response_error<=4'(c[13]==3 ? 4'd8 : {1'b0,last_error});c[13]<=3;save_context(PROCESS_RECORD_NEXT);
                end else begin write_word(arena_base+64'(c[8])*64+60,0,PROCESS_RECORD_OWNER_RELEASED);end
            PROCESS_RECORD_OWNER_RELEASED: begin
                    if({c[7],c[6]}!=0) call_store(9,{c[7],c[6]},0,0,PROCESS_RECORD_REPLACED);
                    else pc<=PROCESS_RECORD_REPLACED;end
            PROCESS_RECORD_REPLACED: if(last_error!=0) begin
                    c[13]<=3;if(response_error==0) response_error<=4'(last_error);save_context(PROCESS_RECORD_NEXT);
                end else begin
                c[6]<=c[8];c[7]<=c[9];c[8]<=0;c[9]<=0;c[13]<=2;
                save_context(RESPONSE);
            end
            PROCESS_RECORD_BEGIN: if(c[17]==1 && c[22][0]) begin
                    reservation_saved_scan<=scan;reservation_context<=context_slot;reserve_capture(PROCESS_RECORD_RESERVED);end
                else pc<=PROCESS_RECORD_NEXT;
            PROCESS_RECORD_WRITER: if(last_error!=0) begin
                    c[13]<=3;if(response_error==0) response_error<=4'(last_error);save_context(PROCESS_RECORD_NEXT);
                end else begin pending_writer<=last_handle;write_word(arena_base+64'(last_handle[31:0])*64+60,2,PROCESS_RECORD_WRITER_OWNED);end
            PROCESS_RECORD_WRITER_OWNED: begin
                    if({c[9],c[8]}!=0) call_store(9,{c[9],c[8]},0,0,PROCESS_RECORD_WRITER_OLD);else pc<=PROCESS_RECORD_WRITER_OLD;end
            PROCESS_RECORD_WRITER_OLD: begin c[8]<=pending_writer[31:0];c[9]<=pending_writer[63:32];c[24]<=0;c[25]<=0;c[13]<=1;save_context(PROCESS_RECORD_NEXT);end
            PROCESS_RECORD_NEXT: pc<=RESPONSE;
            PROCESS_INPUT: if(event_index==event_count) begin scan<=0;pc<=PROCESS_ADVANCE_HISTORY;end
                else begin cursor<=0;event_return<=PROCESS_INPUT_EVENT;pc<=EVENT_READ;end
            PROCESS_INPUT_EVENT: load_history(event_input,PROCESS_INPUT_HISTORY);
            PROCESS_INPUT_HISTORY: if(h[7]!=0) pc<=PROCESS_INPUT_HISTORY_SAVED;
                else call_store(5,{h[1],h[0]},start_frame+64'(event_offset),0,PROCESS_INPUT_CAPTURED);
            PROCESS_INPUT_CAPTURED: if(last_error!=0) begin h[7]<=1;if(response_error==0) response_error<=4'(last_error);pc<=PROCESS_INPUT_HISTORY_SAVED;end
                else begin h[3]<=h[3]+1;pc<=PROCESS_INPUT_HISTORY_SAVED;end
            PROCESS_INPUT_HISTORY_SAVED: save_history(PROCESS_INPUT_RECORDS_START);
            PROCESS_INPUT_RECORDS_START: begin event_index<=event_index+1;pc<=PROCESS_INPUT;end
            PROCESS_ADVANCE_HISTORY: if(scan==32'(INPUTS)) pc<=RESPONSE;
                else load_history(scan,PROCESS_ADVANCE_HISTORY_LOADED);
            PROCESS_ADVANCE_HISTORY_LOADED: if(h[7]!=0) pc<=PROCESS_ADVANCE_HISTORY_NEXT;
                else call_store(13,{h[1],h[0]},start_frame+frames,0,PROCESS_HISTORY_ADVANCED);
            PROCESS_HISTORY_ADVANCED: if(last_error!=0) begin h[7]<=1;if(response_error==0) response_error<=4'(last_error);save_history(PROCESS_ADVANCE_HISTORY_NEXT);end
                else begin
                    x=start_frame+frames;y={h[13],h[12]};z=x>y ? x-y : 0;
                    h[10]<=x[31:0];h[11]<=x[63:32];h[8]<=z[31:0];h[9]<=z[63:32];
                    call_store(12,{h[1],h[0]},0,0,PROCESS_HISTORY_TRIM);
                end
            PROCESS_HISTORY_TRIM: lower_bound({h[9],h[8]},PROCESS_HISTORY_TRIM_INDEX);
            PROCESS_HISTORY_TRIM_INDEX: begin trim_order<=range_index;pc<=PROCESS_HISTORY_DISCARD;end
            PROCESS_HISTORY_DISCARD: if(trim_order==0) save_history(PROCESS_ADVANCE_HISTORY_NEXT);
                else call_store(14,{h[1],h[0]},{32'b0,trim_order},0,PROCESS_HISTORY_DISCARDED);
            PROCESS_HISTORY_DISCARDED: if(last_error!=0) fail({1'b0,last_error});else begin h[6]<=h[6]+trim_order;save_history(PROCESS_ADVANCE_HISTORY_NEXT);end
            PROCESS_ADVANCE_HISTORY_NEXT: begin scan<=scan+1;pc<=PROCESS_ADVANCE_HISTORY;end
            PROCESS_RECORD_ADVANCED: begin
                if(last_error!=0) begin c[13]<=3;if(response_error==0) response_error<=4'(last_error);end
                else begin x={c[25],c[24]}+frames;c[24]<=x[31:0];c[25]<=x[63:32];end
                save_context(RESPONSE);
            end
            PROCESS_CLEANUP_LOADED: if({c[11],c[10]}!=0 && (c[17]!=2 || {c[19],c[18]}>={c[21],c[20]} ||
                    {c[19],c[18]}==0 || {c[11],c[10]}!={c[7],c[6]}))
                    call_store(9,{c[11],c[10]},0,0,PROCESS_CLEANUP_RELEASED);
                else save_context(PROCESS_CLEANUP_NEXT);
            PROCESS_CLEANUP_RELEASED: if(last_error!=0) fail({1'b0,last_error});else begin c[10]<=0;c[11]<=0;save_context(PROCESS_CLEANUP_NEXT);end
            PROCESS_CLEANUP_NEXT: pc<=RESPONSE;
            PROCESS_PLAY_LOADED: if(c[0]==0 || c[17]!=2 || {c[19],c[18]}>={c[21],c[20]}) pc<=PROCESS_PLAY_NEXT;
                else if(c[23]!=0 && {c[19],c[18]}!=0) begin if(response_error==0) response_error<=4'(5);pc<=PROCESS_PLAY_NEXT;end
                else if({c[11],c[10]}==0) begin
                    if(c[13]!=2) begin if(response_error==0) response_error<=4'(8);pc<=PROCESS_PLAY_NEXT;end
                    else if({c[19],c[18]}!=0) begin if(response_error==0) response_error<=4'(2);pc<=PROCESS_PLAY_NEXT;end
                    else call_store(12,{c[7],c[6]},0,0,PROCESS_PLAY_DESCRIPTION);
                end else pc<=PROCESS_PLAY_NEXT;
            PROCESS_PLAY_DESCRIPTION: if(last_error!=0 || last_status!=2 || last_frame!={c[21],c[20]}) begin
                    if(response_error==0) response_error<=4'(8);pc<=PROCESS_PLAY_NEXT;
                end else call_store(8,{c[7],c[6]},0,0,PROCESS_PLAY_PINNED);
            PROCESS_PLAY_PINNED: if(last_error!=0) begin if(response_error==0) response_error<=4'(last_error);pc<=PROCESS_PLAY_NEXT;end
                else begin c[10]<=c[6];c[11]<=c[7];c[12]<=0;c[14]<=0;c[15]<=0;c[23]<=0;c[29]<=last_events;
                    save_context(PROCESS_PLAY_NEXT);end
            PROCESS_PLAY_NEXT: pc<=JOB_READ_DONE;
            PROCESS_EMIT: begin packet_destination<=c[2];packet_offset<=best_frame;
                send_packet<=best_packet;send_length<=best_length;packet_return<=PROCESS_EMITTED;pc<=EMIT_PACKET;end
            PROCESS_EMITTED: if(packet_refused) begin c[23]<=1;if(response_error==0) response_error<=4'(3);
                    begin head_write<=1;head_write_index<=LOOP_BITS'(context_slot);head_write_data<=0;end save_context(JOB_SELECT_BEGIN);
                end else begin c[12]<=c[12]+1;save_context(PROCESS_RESCAN);end
            PROCESS_RESCAN: begin head_return<=JOB_SELECT_BEGIN;pc<=JOB_HEAD;end
            PROCESS_FINISH_LOADED: begin
                if(c[0]!=0) begin
                    if({c[11],c[10]}!=0 && c[23]==0) begin x={c[15],c[14]}+frames;c[14]<=x[31:0];c[15]<=x[63:32];end
                    c[16]<=c[17]==2 && {c[19],c[18]}>={c[21],c[20]} ? 0 : c[17];c[22]<=0;
                end
                save_context(PROCESS_FINISH_NEXT);
            end
            PROCESS_FINISH_NEXT: pc<=RESPONSE;
            PROCESS_FINISHED: begin
                running<=0;seen_quantum<=1;next_frame<=start_frame+frames;response_frame<=start_frame+frames;
                transfer_events<=64;transfer_metadata<=64;publication_bindings<=128;pc<=RESPONSE;
            end
            BIND_UNIQUE: if(scan==32'(LOOPS)) load_context(a0[31:0],BIND_COMMIT);
                else load_context(scan,BIND_UNIQUE_CHECK);
            BIND_UNIQUE_CHECK: if(c[0]!=0 && {c[5],c[4]}==a1) fail(4);else begin scan<=scan+1;pc<=BIND_UNIQUE;end
            BIND_COMMIT: begin c[0]<=1;c[1]<=a2[31:0];c[2]<=a3[31:0];c[3]<=a4[31:0];c[4]<=a1[31:0];c[5]<=a1[63:32];save_context(RESPONSE);end
            RESERVE_HISTORY: if(reservation_scan==32'(INPUTS)) begin reservation_scan<=0;pc<=RESERVE_RECORD;end
                else load_history(reservation_scan,RESERVE_HISTORY_LOADED);
            RESERVE_HISTORY_LOADED: call_store(12,{h[1],h[0]},0,0,RESERVE_HISTORY_VIEW);
            RESERVE_HISTORY_VIEW: begin
                value={h[13],h[12]}==0 ? 3 : 32'((MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS)+3;
                if(value>last_blocks) reserved_blocks<=reserved_blocks+value-last_blocks;
                reservation_scan<=reservation_scan+1;pc<=RESERVE_HISTORY;
            end
            RESERVE_RECORD: if(reservation_scan==32'(LOOPS)) pc<=reservation_return;
                else load_context(reservation_scan,RESERVE_RECORD_LOADED);
            RESERVE_RECORD_LOADED: if(c[13]==1 && {c[9],c[8]}!=0) call_store(12,{c[9],c[8]},0,0,RESERVE_RECORD_VIEW);
                else begin reservation_scan<=reservation_scan+1;pc<=RESERVE_RECORD;end
            RESERVE_RECORD_VIEW: begin
                value=32'((MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS);
                if(value>last_blocks) reserved_blocks<=reserved_blocks+value-last_blocks;
                reservation_scan<=reservation_scan+1;pc<=RESERVE_RECORD;
            end
            IMPORT_RESERVED: if(free_views<=2) fail(3);else call_store(4,0,0,0,IMPORT_WRITER);
            IMPORT_APPEND_CHECK: if(last_error!=0) fail({1'b0,last_error});else if(last_owner!=0 || last_status!=1) fail(4);
                else begin value=(last_events+1+32'(BLOCK_EVENTS)-1)/32'(BLOCK_EVENTS);
                    import_needed<=value>last_blocks ? value-last_blocks : 0;reserve_capture(IMPORT_APPEND_RESERVED);end
            IMPORT_APPEND_RESERVED: if(free_blocks<reserved_blocks+import_needed) fail(3);else pc<=IMPORT_APPEND;
            PROCESS_RECORD_RESERVED: begin scan<=reservation_saved_scan;load_context(reservation_context,PROCESS_RECORD_RESERVATION);end
            PROCESS_RECORD_RESERVATION: if(free_blocks<reserved_blocks+32'((MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS)) begin
                    c[13]<=3;if(response_error==0) response_error<=4'(3);save_context(PROCESS_RECORD_NEXT);
                end else call_store(4,0,c[31]==0 ? 0 : {32'b0,c[31]},0,PROCESS_RECORD_WRITER);
            CONTROL_MAINTAINED: begin response_count<=store_work;pc<=RESPONSE;end
            ALIGN_DURATION: begin
                x=(alignment_remainder<<1)|64'(alignment_value[alignment_bit]);
                if(x>={32'b0,quantum_frames}) x=x-{32'b0,quantum_frames};
                alignment_remainder<=x;
                if(alignment_bit==0) begin if(x!=0) fail(4);else pc<=alignment_return;end
                else alignment_bit<=alignment_bit-1;
            end
            GROUP_DESCRIPTION: if(last_error!=0) fail({1'b0,last_error});else if(last_status!=2 || last_frame==0) fail(4);
                else if(last_references>32'hffffffff-target_count) fail(6);
                else begin range_start<=0;range_end<=last_frame;aligned_duration(last_frame,GRAB_VALIDATE);end
            GROUP_START: begin target_index<=0;response_handle<=range_view;response_frame<=range_end;
                pc<=op==22 ? GRAB_PUBLISH : GRAB_RETAIN;end
            EMIT_PACKET: if(packet_ready) pc<=packet_return;
            default: memory_fault();
        endcase
    end
endmodule
