module midi_store #(
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, RETIREMENTS=256,
    parameter integer TABLE_ENTRIES=(MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1
) (
    input wire clk, input wire reset, input wire [63:0] arena_base,
    output wire [63:0] arena_bytes,
    input wire command_valid, output wire command_ready, input wire [3:0] command_op,
    input wire [63:0] command_view,
    input wire [63:0] command_arg0, input wire [63:0] command_arg1,
    input wire [63:0] command_source, input wire [31:0] command_count,
    input wire [23:0] command_packet, input wire [1:0] command_length,
    output wire response_valid, input wire response_ready, output reg [3:0] response_error,
    output reg [63:0] response_handle,
    output reg [63:0] response_frame, output reg [31:0] response_order,
    output reg [23:0] response_packet, output reg [1:0] response_length,
    output wire [31:0] response_events, output wire [31:0] response_status,
    output reg [31:0] response_work,
    output wire [31:0] response_blocks, output wire [31:0] response_owner, output wire [31:0] response_references,
    output wire [31:0] free_block_slots,
    output wire [31:0] free_view_slots, output wire [31:0] free_retirement_slots,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output reg [63:0] memory_address, output reg [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    `include "midi_functions.svh"
    initial begin
        if(BLOCK_EVENTS<1 || BLOCKS<1 || MAX_EVENTS<1 || VIEWS<1 || RETIREMENTS<1 ||
            TABLE_ENTRIES<(MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1)
            $fatal(1,"Invalid MIDI storage capacities");
    end
    localparam [63:0] TABLE_BASE=64'(VIEWS)*64,
        REF_BASE=TABLE_BASE+64'(VIEWS)*64'(TABLE_ENTRIES)*4,
        FREE_BASE=REF_BASE+64'(BLOCKS)*4,
        EVENT_BASE=FREE_BASE+64'(BLOCKS)*4,
        RETIRE_BASE=EVENT_BASE+64'(BLOCKS)*64'(BLOCK_EVENTS)*16;
    assign arena_bytes=RETIRE_BASE+64'(RETIREMENTS)*8;
    localparam [15:0] IDLE=0,
        READ_REQ=1,
        READ_WAIT=2,
        WRITE_REQ=3,
        WRITE_WAIT=4,
        DISPATCH=5,
        FORMAT=6,
        FORMAT_NEXT=7,
        LOAD_VIEW=8,
        LOADED_VIEW=9,
        CHECK_VIEW=10,
        SAVE_VIEW=11,
        SAVED_VIEW=12,
        VIEW_ALLOC=13,
        VIEW_ALLOC_GEN=14,
        VIEW_ALLOC_REFS=15,
        VIEW_ALLOC_STATUS=16,
        VIEW_ALLOC_CHECK=17,
        VIEW_CREATED=18,
        VIEW_READY=19,
        APPEND=20,
        APPEND_LAST=21,
        APPEND_PREPARE=22,
        APPEND_BLOCK=23,
        APPEND_BLOCK_REF=24,
        APPEND_TABLE=25,
        APPEND_BLOCK_ID=26,
        APPEND_PAYLOAD=27,
        APPEND_PAYLOAD_NEXT=28,
        RECORD=29,
        RECORD_BLOCK=30,
        RECORD_PAYLOAD=31,
        RECORD_PAYLOAD_NEXT=32,
        FINISH=33,
        FINISH_LAST=34,
        RETAIN=35,
        RELEASE=36,
        RELEASE_REF=37,
        RELEASE_QUEUE=38,
        RELEASE_QUEUE_GEN=39,
        RANGE=40,
        SEARCH=41,
        SEARCH_READ=42,
        SEARCH_COMPARE=43,
        RANGE_BOUNDS=44,
        RANGE_TABLE=45,
        RANGE_TABLE_ID=46,
        RANGE_REF=47,
        RANGE_REF_WRITE=48,
        RANGE_TABLE_NEXT=49,
        MAINTAIN=50,
        MAINTAIN_SLOT=51,
        MAINTAIN_GEN=52,
        MAINTAIN_LOADED=53,
        MAINTAIN_BLOCK_ID=54,
        BLOCK_RELEASE=55,
        BLOCK_RELEASE_READ=56,
        BLOCK_RELEASE_WRITE=57,
        BLOCK_FREE=58,
        MAINTAIN_UPDATE=59,
        MAINTAIN_FINAL=60,
        MAINTAIN_RETIRED=61,
        MAINTAIN_NEXT=62,
        DISCARD=63,
        DISCARD_BLOCK=64,
        DISCARD_ID=65,
        DISCARD_RELEASED=66,
        DISCARD_TABLE=67,
        DISCARD_TABLE_WRITE=68,
        DISCARD_TABLE_NEXT=69,
        DISCARD_FINISH=70,
        RESPONSE=71, BATCH_START=72, BATCH_FETCH=73, BATCH_OFFSET=74, BATCH_PACKET=75,
        BATCH_NEXT=76, BATCH_FINISH=77;
    reg [15:0] phase, return_phase, after_load, after_save, after_record, after_block;
    reg [3:0] op;
    reg descriptor_valid;
    reg [63:0] descriptor_handle, batch_source, batch_position, batch_last_frame;
    reg [31:0] batch_count, batch_index, batch_offset, append_ordinal, append_block;
    reg batch_has_last, append_block_valid, record_block_valid;
    reg [31:0] record_block_ordinal, record_block_id;
    reg formatted, searching_end;
    reg [63:0] view_handle, arg0, arg1;
    reg [23:0] packet;
    reg [1:0] length;
    reg [31:0] desc [0:15];
    localparam integer VIEW_COUNT_BITS = $clog2(64'(VIEWS)+1);
    localparam integer QUEUE_COUNT_BITS = $clog2(64'(RETIREMENTS)+1);
    localparam integer QUEUE_INDEX_BITS = RETIREMENTS > 1 ? $clog2(RETIREMENTS) : 1;
    localparam integer FORMAT_BITS = $clog2(64'(VIEWS)*16+64'(BLOCKS)*2+1);
    reg [VIEW_COUNT_BITS-1:0] candidate;
    reg [3:0] cursor;
    reg [FORMAT_BITS-1:0] scan;
    reg [QUEUE_COUNT_BITS-1:0] queue_count;
    reg [QUEUE_INDEX_BITS-1:0] queue_head, queue_tail;
    reg [31:0] view_slot, source_slot, generation, refs;
    reg [31:0] data_word, block_id, free_count;
    reg [31:0] used_views;
    assign response_blocks=desc[3];
    assign response_owner=desc[15];
    assign response_references=desc[1];
    assign free_block_slots=free_count;
    assign free_view_slots=32'(VIEWS)-used_views;
    assign free_retirement_slots=32'(RETIREMENTS)-32'(queue_count);
    reg [31:0] record_index, payload [0:3];
    reg [1:0] record_word;
    reg [63:0] record_address, record_frame;
    reg [31:0] low, high, middle, range_first, range_count, first_table, table_count, table_cursor;
    reg [31:0] removed_blocks, discard_count, released_refs;
    reg [63:0] search_frame;
    assign command_ready=phase==IDLE && !reset;
    assign response_valid=phase==RESPONSE && !reset;
    assign memory_valid=(phase==READ_REQ || phase==WRITE_REQ) && !reset;
    assign memory_write=phase==WRITE_REQ;
    assign memory_response_ready=phase==READ_WAIT || phase==WRITE_WAIT;
    assign response_events=desc[5];
    assign response_status=desc[2];
    localparam integer VIEW_BITS = VIEWS > 1 ? $clog2(VIEWS) : 1;
    localparam integer TABLE_BITS = TABLE_ENTRIES > 1 ? $clog2(TABLE_ENTRIES) : 1;
    initial if(RETIRE_BASE+64'(RETIREMENTS)*8>64'h100000000) $fatal(1,"MIDI storage offsets exceed 32 bits");
    function automatic [63:0] view_address(input [31:0] slot, input integer field);
        reg [31:0] offset;
        begin offset=32'(VIEW_BITS'(slot))*64+32'(field[3:0])*4;view_address=arena_base+{32'b0,offset};end
    endfunction
    function automatic [63:0] table_address(input [31:0] slot, input [31:0] ordinal);
        reg [31:0] offset;
        begin offset=32'(TABLE_BASE)+(32'(VIEW_BITS'(slot))*32'(TABLE_ENTRIES)+32'(TABLE_BITS'(ordinal)))*4;table_address=arena_base+{32'b0,offset};end
    endfunction
    task automatic read_word(input [63:0] address, input [15:0] next);
        begin memory_address<=address;return_phase<=next;phase<=READ_REQ;end
    endtask
    task automatic write_word(input [63:0] address, input [31:0] value, input [15:0] next);
        begin memory_address<=address;memory_write_data<=value;return_phase<=next;phase<=WRITE_REQ;end
    endtask
    task automatic fail(input [3:0] error_code);
        begin response_error<=error_code;phase<=RESPONSE;descriptor_valid<=0;end
    endtask
    task automatic save_view(input [15:0] next);
        begin cursor<=0;after_save<=next;phase<=SAVE_VIEW;end
    endtask
    task automatic load_view(input [15:0] next);
        begin cursor<=0;after_load<=next;phase<=LOAD_VIEW;record_block_valid<=0;end
    endtask
    task automatic read_record(input [31:0] index, input [15:0] next);
        begin record_index<=index;after_record<=next;phase<=RECORD;end
    endtask
    always @(posedge clk) begin : execution
        reg [63:0] origin, frames;
        reg [31:0] position;
        if (reset) begin
            phase<=IDLE;formatted<=0;used_views<=0;descriptor_valid<=0;descriptor_handle<=0;
            batch_source<=0;batch_position<=0;batch_last_frame<=0;batch_count<=0;batch_index<=0;batch_offset<=0;
            append_ordinal<=0;append_block<=0;batch_has_last<=0;append_block_valid<=0;
            record_block_valid<=0;record_block_ordinal<=0;record_block_id<=0;response_error<=0;response_handle<=0;
            response_frame<=0;response_order<=0;response_packet<=0;response_length<=0;response_work<=0;
            memory_address<=0;memory_write_data<=0;free_count<=0;queue_head<=0;queue_tail<=0;queue_count<=0;
            op<=0;view_handle<=0;arg0<=0;arg1<=0;packet<=0;length<=0;
            cursor<=0;scan<=0;data_word<=0;view_slot<=0;source_slot<=0;candidate<=0;generation<=0;
            refs<=0;record_index<=0;record_word<=0;record_address<=0;record_frame<=0;
            block_id<=0;low<=0;high<=0;middle<=0;range_first<=0;range_count<=0;
            first_table<=0;table_count<=0;table_cursor<=0;removed_blocks<=0;discard_count<=0;released_refs<=0;
            searching_end<=0;search_frame<=0;return_phase<=0;after_load<=0;after_save<=0;
            after_record<=0;after_block<=0;
        end else case (phase)
            IDLE: if(command_valid) begin
                op<=command_op;view_handle<=command_view;
                batch_source<=command_source;batch_count<=command_count;
                arg0<=command_arg0;arg1<=command_arg1;packet<=command_packet;length<=command_length;
                response_error<=0;response_length<=0;response_work<=0;phase<=DISPATCH;
            end
            READ_REQ: if(memory_ready) phase<=READ_WAIT;
            READ_WAIT: if(memory_response_valid) begin
                if(memory_response_error) begin formatted<=0;fail(7);end
                else begin data_word<=memory_read_data;phase<=return_phase;end
            end
            WRITE_REQ: if(memory_ready) phase<=WRITE_WAIT;
            WRITE_WAIT: if(memory_response_valid) begin
                if(memory_response_error) begin formatted<=0;fail(7);end
                else phase<=return_phase;
            end
            DISPATCH: begin
                if(op==0) begin formatted<=0;descriptor_valid<=0;scan<=0;phase<=FORMAT;end
                else if(!formatted) fail(4);
                else if(op==4) begin candidate<=0;phase<=VIEW_ALLOC;end
                else if(op==1 || op==2 || op==3) fail(2);
                else if(op==11) phase<=MAINTAIN;
                else if(view_handle[31:0]>=VIEWS || view_handle[63:32]==0) fail(4);
                else begin view_slot<=view_handle[31:0];if(descriptor_valid && descriptor_handle==view_handle) phase<=CHECK_VIEW;else load_view(CHECK_VIEW);end
            end
            FORMAT: begin
                if(32'(scan)<32'(VIEWS)*16) write_word(arena_base+64'(scan)*4,0,FORMAT_NEXT);
                else if(32'(scan)<32'(VIEWS)*16+32'(BLOCKS)*2) begin
                    position=32'(scan)-32'(VIEWS)*16;
                    write_word(arena_base+REF_BASE+64'(position)*4,
                        position<32'(BLOCKS) ? 0 : 32'(BLOCKS)*2-position-1,FORMAT_NEXT);
                end else begin
                    formatted<=1;used_views<=0;free_count<=32'(BLOCKS);queue_head<=0;queue_tail<=0;queue_count<=0;phase<=RESPONSE;
                end
            end
            FORMAT_NEXT: begin scan<=scan+1'b1;phase<=FORMAT;end
            LOAD_VIEW: read_word(view_address(view_slot,integer'(cursor)),LOADED_VIEW);
            LOADED_VIEW: begin
                desc[cursor[3:0]]<=data_word;
                if(cursor==15) begin phase<=after_load;descriptor_valid<=1;descriptor_handle<=view_handle;end
                else begin cursor<=cursor+1'b1;phase<=LOAD_VIEW;end
            end
            CHECK_VIEW: begin
                if(desc[0]!=view_handle[63:32] || desc[1]==0 || desc[2]==4) fail(4);
                else case(op)
                    5: begin append_block_valid<=0;phase<=APPEND;end
                    15: phase<=BATCH_START;
                    6,13: phase<=FINISH;
                    7: phase<=RANGE;
                    8: phase<=RETAIN;
                    9: phase<=RELEASE;
                    10: if(arg0>={32'b0,desc[5]}) phase<=RESPONSE;else read_record(arg0[31:0],RESPONSE);
                    12: begin response_handle<=view_handle;response_frame<={desc[9],desc[8]};phase<=RESPONSE;end
                    14: phase<=DISCARD;
                    default: fail(4);
                endcase
            end
            SAVE_VIEW: write_word(view_address(view_slot,integer'(cursor)),desc[cursor[3:0]],SAVED_VIEW);
            SAVED_VIEW: if(cursor==15) phase<=after_save;else begin cursor<=cursor+1'b1;phase<=SAVE_VIEW;end
            VIEW_ALLOC: if(32'(candidate)>=32'(VIEWS)) fail(3);else read_word(view_address(32'(candidate),0),VIEW_ALLOC_GEN);
            VIEW_ALLOC_GEN: begin generation<=data_word;read_word(view_address(32'(candidate),1),VIEW_ALLOC_REFS);end
            VIEW_ALLOC_REFS: begin refs<=data_word;read_word(view_address(32'(candidate),2),VIEW_ALLOC_STATUS);end
            VIEW_ALLOC_STATUS: if(refs!=0 || data_word==4) begin candidate<=candidate+1'b1;phase<=VIEW_ALLOC;end
                else if(generation==32'hffffffff) fail(6);
                else begin
                    generation<=generation+1;view_slot<=32'(candidate);
                    response_handle<={generation+1,32'(candidate)};phase<=VIEW_ALLOC_CHECK;
                end
            VIEW_ALLOC_CHECK: phase<=VIEW_CREATED;
            VIEW_CREATED: begin
                begin
                    descriptor_valid<=1;descriptor_handle<={generation,view_slot};record_block_valid<=0;
                    used_views<=used_views+1;desc[0]<=generation;desc[1]<=1;desc[2]<=op==7 ? 2 : 1;
                    desc[3]<=op==7 ? table_count : 0;desc[4]<=op==7 ? (desc[4]+range_first)%32'(BLOCK_EVENTS) : 0;
                    desc[5]<=op==7 ? range_count : 0;
                    origin=op==7 ? {desc[7],desc[6]}+arg0 : 0;
                    desc[6]<=origin[31:0];desc[7]<=origin[63:32];
                    frames=op==7 ? arg1-arg0 : 0;
                    desc[8]<=frames[31:0];desc[9]<=frames[63:32];desc[10]<=0;desc[11]<=0;
                    desc[12]<=0;desc[13]<=op==4 && arg0!=0 ? arg0[31:0] : 32'(MAX_EVENTS);desc[14]<=0;desc[15]<=0;
                    phase<=VIEW_READY;
                end
            end
            VIEW_READY: if(op==7) begin table_cursor<=0;phase<=RANGE_TABLE;end else save_view(RESPONSE);
            APPEND: begin
                if(desc[2]!=1 || desc[1]!=1) fail(4);
                else if(packet_error(packet,length)!=0) fail({1'b0,packet_error(packet,length)});
                else if(desc[12]==32'hffffffff) fail(6);
                else if(desc[5]>=desc[13]) begin desc[2]<=3;response_error<=3;save_view(RESPONSE);end
                else if(arg0<{desc[9],desc[8]} || arg0+{desc[7],desc[6]}<arg0) fail(1);
                else if(op==15 && batch_has_last) begin if(arg0<batch_last_frame) fail(1);else phase<=APPEND_PREPARE;end
                else if(desc[5]!=0) read_record(desc[5]-1,APPEND_LAST);
                else phase<=APPEND_PREPARE;
            end
            APPEND_LAST: if(arg0<response_frame) fail(1);else phase<=APPEND_PREPARE;
            APPEND_PREPARE: begin
                position=desc[4]+desc[5];
                if(position/32'(BLOCK_EVENTS)>=desc[3]) begin
                    if(free_count==0 || desc[3]>=32'(TABLE_ENTRIES)) begin
                        desc[2]<=3;response_error<=3;save_view(RESPONSE);
                    end
                    else read_word(arena_base+FREE_BASE+64'(32'(free_count-1))*4,APPEND_BLOCK);
                end else if(op==15 && append_block_valid && append_ordinal==position/32'(BLOCK_EVENTS)) begin data_word<=append_block;phase<=APPEND_BLOCK_ID;end
                else read_word(table_address(view_slot,position/32'(BLOCK_EVENTS)),APPEND_BLOCK_ID);
            end
            APPEND_BLOCK: begin block_id<=data_word;write_word(arena_base+REF_BASE+64'(data_word)*4,1,APPEND_BLOCK_REF);end
            APPEND_BLOCK_REF: write_word(table_address(view_slot,desc[3]),block_id,APPEND_TABLE);
            APPEND_TABLE: begin free_count<=free_count-1;desc[3]<=desc[3]+1;phase<=APPEND_BLOCK_ID;data_word<=block_id;end
            APPEND_BLOCK_ID: begin
                position=desc[4]+desc[5];origin={desc[7],desc[6]}+arg0;
                append_block_valid<=1;append_ordinal<=position/32'(BLOCK_EVENTS);append_block<=data_word;
                record_address<=arena_base+EVENT_BASE+64'(32'(data_word*32'(BLOCK_EVENTS)+position%32'(BLOCK_EVENTS)))*16;
                payload[0]<=origin[31:0];payload[1]<=origin[63:32];payload[2]<=desc[12];payload[3]<={6'b0,length,packet};
                record_word<=0;phase<=APPEND_PAYLOAD;
            end
            APPEND_PAYLOAD: write_word(record_address+64'(record_word)*4,payload[record_word[1:0]],APPEND_PAYLOAD_NEXT);
            APPEND_PAYLOAD_NEXT: if(record_word==3) begin
                    desc[5]<=desc[5]+1;desc[12]<=desc[12]+1;
                    if(op==15) begin batch_has_last<=1;batch_last_frame<=arg0;phase<=BATCH_NEXT;end
                    else save_view(RESPONSE);
                end else begin record_word<=record_word+1'b1;phase<=APPEND_PAYLOAD;end
            RECORD: begin
                position=desc[4]+record_index;
                if(record_block_valid && record_block_ordinal==position/32'(BLOCK_EVENTS)) begin data_word<=record_block_id;phase<=RECORD_BLOCK;end
                else read_word(table_address(view_slot,position/32'(BLOCK_EVENTS)),RECORD_BLOCK);
            end
            RECORD_BLOCK: begin
                position=desc[4]+record_index;
                record_block_valid<=1;record_block_ordinal<=position/32'(BLOCK_EVENTS);record_block_id<=data_word;
                record_address<=arena_base+EVENT_BASE+64'(32'(data_word*32'(BLOCK_EVENTS)+position%32'(BLOCK_EVENTS)))*16;
                record_word<=0;phase<=RECORD_PAYLOAD;
            end
            RECORD_PAYLOAD: read_word(record_address+64'(record_word)*4,RECORD_PAYLOAD_NEXT);
            RECORD_PAYLOAD_NEXT: begin
                payload[record_word[1:0]]<=data_word;
                if(record_word==3) begin
                    record_frame<={payload[1],payload[0]};response_frame<={payload[1],payload[0]}-{desc[7],desc[6]};
                    response_order<=payload[2];response_packet<=data_word[23:0];response_length<=data_word[25:24];
                    phase<=after_record;
                end else begin record_word<=record_word+1'b1;phase<=RECORD_PAYLOAD;end
            end
            FINISH: if(desc[2]==3) fail(8);else if(desc[2]!=1 || arg0<{desc[9],desc[8]}) fail(4);
                else if(desc[5]!=0) read_record(desc[5]-1,FINISH_LAST);else phase<=FINISH_LAST;
            FINISH_LAST: if(desc[5]!=0 && response_frame>=arg0) fail(1);
                else begin desc[8]<=arg0[31:0];desc[9]<=arg0[63:32];if(op==6) desc[2]<=2;save_view(RESPONSE);end
            RETAIN: if(desc[2]!=2) fail(8);else if(desc[1]==32'hffffffff) fail(6);
                else begin desc[1]<=desc[1]+1;response_handle<=view_handle;save_view(RESPONSE);end
            RELEASE: if(desc[1]==1 && 32'(queue_count)>=32'(RETIREMENTS)) fail(3);
                else begin desc[1]<=desc[1]-1;if(desc[1]==1) desc[2]<=4;save_view(RELEASE_REF);end
            RELEASE_REF: if(desc[1]!=0) phase<=RESPONSE;
                else write_word(arena_base+RETIRE_BASE+64'(queue_tail)*8,view_slot,RELEASE_QUEUE);
            RELEASE_QUEUE: write_word(arena_base+RETIRE_BASE+64'(queue_tail)*8+4,desc[0],RELEASE_QUEUE_GEN);
            RELEASE_QUEUE_GEN: begin queue_tail<=QUEUE_INDEX_BITS'((32'(queue_tail)+1)%32'(RETIREMENTS));queue_count<=queue_count+1'b1;phase<=RESPONSE;end
            RANGE: if(desc[2]==3 || arg0>arg1 || arg1>{desc[9],desc[8]} || arg0+{desc[7],desc[6]}<arg0) fail(1);
                else begin source_slot<=view_slot;low<=0;high<=desc[5];search_frame<=arg0;searching_end<=0;phase<=SEARCH;end
            SEARCH: if(low==high) begin
                    if(!searching_end) begin range_first<=low;low<=0;high<=desc[5];search_frame<=arg1;searching_end<=1;end
                    else begin range_count<=low-range_first;phase<=RANGE_BOUNDS;end
                end else begin middle<=low+(high-low)/2;phase<=SEARCH_READ;end
            SEARCH_READ: read_record(middle,SEARCH_COMPARE);
            SEARCH_COMPARE: begin if(response_frame<search_frame) low<=middle+1;else high<=middle;phase<=SEARCH;end
            RANGE_BOUNDS: if(range_count>32'(MAX_EVENTS)) fail(3);
                else begin
                    first_table<=(desc[4]+range_first)/32'(BLOCK_EVENTS);
                    table_count<=range_count==0 ? 0 : ((desc[4]+range_first)%32'(BLOCK_EVENTS)+range_count+32'(BLOCK_EVENTS)-1)/32'(BLOCK_EVENTS);
                    candidate<=0;phase<=VIEW_ALLOC;
                end
            RANGE_TABLE: if(table_cursor==table_count) save_view(RESPONSE);
                else read_word(table_address(source_slot,first_table+table_cursor),RANGE_TABLE_ID);
            RANGE_TABLE_ID: begin block_id<=data_word;read_word(arena_base+REF_BASE+64'(data_word)*4,RANGE_REF);end
            RANGE_REF: if(data_word==32'hffffffff) begin formatted<=0;fail(7);end
                else write_word(arena_base+REF_BASE+64'(block_id)*4,data_word+1,RANGE_REF_WRITE);
            RANGE_REF_WRITE: write_word(table_address(view_slot,table_cursor),block_id,RANGE_TABLE_NEXT);
            RANGE_TABLE_NEXT: begin table_cursor<=table_cursor+1;phase<=RANGE_TABLE;end
            MAINTAIN: if(queue_count==0 || response_work>=arg0[31:0]) phase<=RESPONSE;
                else read_word(arena_base+RETIRE_BASE+64'(queue_head)*8,MAINTAIN_SLOT);
            MAINTAIN_SLOT: begin view_slot<=data_word;read_word(arena_base+RETIRE_BASE+64'(queue_head)*8+4,MAINTAIN_GEN);end
            MAINTAIN_GEN: begin view_handle<={data_word,view_slot};load_view(MAINTAIN_LOADED);end
            MAINTAIN_LOADED: if(desc[0]!=view_handle[63:32] || desc[2]!=4 || desc[1]!=0) begin formatted<=0;fail(7);end
                else if(desc[3]!=0) read_word(table_address(view_slot,desc[3]-1),MAINTAIN_BLOCK_ID);
                else phase<=MAINTAIN_FINAL;
            MAINTAIN_BLOCK_ID: begin block_id<=data_word;after_block<=MAINTAIN_UPDATE;phase<=BLOCK_RELEASE;end
            BLOCK_RELEASE: read_word(arena_base+REF_BASE+64'(block_id)*4,BLOCK_RELEASE_READ);
            BLOCK_RELEASE_READ: if(data_word==0) begin formatted<=0;fail(7);end
                else begin released_refs<=data_word-1;write_word(arena_base+REF_BASE+64'(block_id)*4,data_word-1,BLOCK_RELEASE_WRITE);end
            BLOCK_RELEASE_WRITE: if(released_refs==0) write_word(arena_base+FREE_BASE+64'(free_count)*4,block_id,BLOCK_FREE);
                else phase<=after_block;
            BLOCK_FREE: begin free_count<=free_count+1;phase<=after_block;end
            MAINTAIN_UPDATE: begin desc[3]<=desc[3]-1;save_view(MAINTAIN_NEXT);end
            MAINTAIN_FINAL: begin used_views<=used_views-1;desc[2]<=0;save_view(MAINTAIN_RETIRED);end
            MAINTAIN_RETIRED: begin queue_head<=QUEUE_INDEX_BITS'((32'(queue_head)+1)%32'(RETIREMENTS));queue_count<=queue_count-1'b1;phase<=MAINTAIN_NEXT;end
            MAINTAIN_NEXT: begin response_work<=response_work+1;phase<=MAINTAIN;end
            DISCARD: if(desc[2]!=1 || desc[1]!=1 || arg0>{32'b0,desc[5]}) fail(4);
                else begin record_block_valid<=0;discard_count<=arg0[31:0];removed_blocks<=(desc[4]+arg0[31:0])/32'(BLOCK_EVENTS);table_cursor<=0;phase<=DISCARD_BLOCK;end
            DISCARD_BLOCK: if(table_cursor==removed_blocks) begin table_cursor<=0;phase<=DISCARD_TABLE;end
                else read_word(table_address(view_slot,table_cursor),DISCARD_ID);
            DISCARD_ID: begin block_id<=data_word;after_block<=DISCARD_RELEASED;phase<=BLOCK_RELEASE;end
            DISCARD_RELEASED: begin table_cursor<=table_cursor+1;phase<=DISCARD_BLOCK;end
            DISCARD_TABLE: if(table_cursor==desc[3]-removed_blocks) phase<=DISCARD_FINISH;
                else read_word(table_address(view_slot,table_cursor+removed_blocks),DISCARD_TABLE_WRITE);
            DISCARD_TABLE_WRITE: write_word(table_address(view_slot,table_cursor),data_word,DISCARD_TABLE_NEXT);
            DISCARD_TABLE_NEXT: begin table_cursor<=table_cursor+1;phase<=DISCARD_TABLE;end
            DISCARD_FINISH: begin
                desc[3]<=desc[3]-removed_blocks;desc[4]<=(desc[4]+discard_count)%32'(BLOCK_EVENTS);desc[5]<=desc[5]-discard_count;save_view(RESPONSE);
            end
            BATCH_START: begin
                if(desc[2]!=1 || desc[1]!=1 || arg0<{desc[9],desc[8]} || arg1<arg0) fail(4);
                else if(batch_source[3:0]!=0 || batch_source<arena_base+arena_bytes) fail(1);
                else begin batch_position<=arg0;batch_index<=0;batch_has_last<=0;append_block_valid<=0;phase<=BATCH_FETCH;end
            end
            BATCH_FETCH: if(batch_index==batch_count) phase<=BATCH_FINISH;
                else read_word(batch_source+64'(batch_index)*16,BATCH_OFFSET);
            BATCH_OFFSET: begin batch_offset<=data_word;read_word(batch_source+64'(batch_index)*16+12,BATCH_PACKET);end
            BATCH_PACKET: begin
                if({32'b0,batch_offset}>=arg1-batch_position || batch_position+{32'b0,batch_offset}<batch_position || data_word[31:26]!=0) fail(1);
                else begin arg0<=batch_position+{32'b0,batch_offset};packet<=data_word[23:0];length<=data_word[25:24];phase<=APPEND;end
            end
            BATCH_NEXT: begin batch_index<=batch_index+1;phase<=BATCH_FETCH;end
            BATCH_FINISH: begin desc[8]<=arg1[31:0];desc[9]<=arg1[63:32];save_view(RESPONSE);end
            RESPONSE: if(response_ready) phase<=IDLE;
            default: begin formatted<=0;fail(7);end
        endcase
    end
endmodule
