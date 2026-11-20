`include "state_processor_defs.svh"
module midi_data_jobs #(
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8,
    parameter integer BUFFERS=256, CURSORS=128, SCRATCH_EVENTS=131072,
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, RETIREMENTS=256, MAINTENANCE=64,
    parameter integer TABLE_ENTRIES=(MAX_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1
) (
    input wire clk, input wire reset, input wire [63:0] arena_base,
    input wire [31:0] quantum_frames, output wire [63:0] arena_bytes,
    input wire prepare_valid, output wire prepare_ready, output wire prepare_done,
    output wire [3:0] prepare_error, input wire [63:0] event_address, input wire [31:0] event_count,
    input wire finish_valid, output wire finish_ready, output wire finish_done,
    output wire [3:0] finish_error,
    output wire idle, input wire control_enable, input wire fault_request,
    output wire publication_valid, input wire publication_commit,
    input wire job_valid, output wire job_ready, input wire [255:0] job_data,
    output wire job_done, output wire [3:0] job_error,
    input wire command_valid, output wire command_ready, input wire [5:0] command_op,
    input wire [63:0] command_arg0, input wire [63:0] command_arg1, input wire [63:0] command_arg2,
    input wire [63:0] command_arg3, input wire [63:0] command_arg4,
    output wire response_valid, input wire response_ready, output reg [3:0] response_error,
    output reg [63:0] response_handle, output reg [63:0] response_frame,
    output reg [31:0] response_count, output reg [23:0] response_packet, output reg [1:0] response_length,
    output wire packet_valid, input wire packet_ready, input wire packet_refused,
    output wire [23:0] packet_data, output wire [1:0] packet_length,
    output reg [31:0] packet_destination, output wire [31:0] packet_offset,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    `include "midi_functions.svh"
    localparam [63:0] RAW_CONTENT_BYTES=64'(VIEWS)*64+64'(VIEWS)*64'(TABLE_ENTRIES)*4+
        64'(BLOCKS)*8+64'(BLOCKS)*64'(BLOCK_EVENTS)*16+64'(RETIREMENTS)*8;
    localparam [63:0] CONTENT_BYTES=(RAW_CONTENT_BYTES+15)&~64'd15;
    assign arena_bytes=CONTENT_BYTES+64'(SCRATCH_EVENTS)*16;
    localparam integer SEQ_BITS=LOOPS>1 ? $clog2(LOOPS) : 1;
    localparam integer BUFFER_BITS=BUFFERS>1 ? $clog2(BUFFERS) : 1;
    localparam integer CURSOR_BITS=CURSORS>1 ? $clog2(CURSORS) : 1;
    initial begin
        if(LOOPS<1 || BUFFERS<1 || CURSORS<1 || CURSORS>256 || SCRATCH_EVENTS<1 ||
            INPUTS<1 || DESTINATIONS<1 || MAINTENANCE<0 || MAINTENANCE>64)
            $fatal(1,"Invalid MIDI data-job capacities");
    end
    typedef enum logic [7:0] {
        IDLE, DISPATCH, REPLY, STORE_REQUEST, STORE_WAIT, READ_REQUEST, READ_WAIT, WRITE_REQUEST, WRITE_WAIT,
        LOAD_WORD, LOAD_CAPTURE, COPY_WORD, COPY_NEXT, SEQUENCE_REQUEST, SEQUENCE_LATCH, SEQUENCE_WRITE,
        BUFFER_REQUEST, BUFFER_LATCH, BUFFER_WRITE, FORMAT_STORE, FORMAT_SEQUENCES,
        PREPARE_NEXT, PREPARE_CHECK, INPUT_NEXT, INPUT_CHECK, INPUT_ADVANCE,
        BEGIN_CAPTURE, BEGIN_OLD_DESCRIBED, BEGIN_CREATED, BEGIN_RELEASED, FINISH_CAPTURE, FINISH_DESCRIBED,
        APPEND_SEQUENCE, APPEND_BUFFER, CURSOR_REQUEST, CURSOR_LATCH, READ_BEGIN,
        READ_DESCRIBED, SEARCH, SEARCH_COMPARE, READ_NEXT, READ_FETCHED, READ_HEAD, READ_CONSUME, READ_PUBLISH,
        MERGE_LEFT, MERGE_RIGHT, MERGE_NEXT, MERGE_A, MERGE_B, MERGE_CHOOSE, MERGE_ADVANCE,
        OUTPUT_BUFFER, OUTPUT_NEXT, OUTPUT_SEND,
        CONTROL_SEQUENCE, CONTROL_DUPLICATE, REPLACE_SEQUENCE, REPLACE_DESCRIBED,
        REPLACE_CHECK_OLD, REPLACE_OLD_DESCRIBED, REPLACE_RETAIN, REPLACE_RELEASE, REPLACE_WRITE, REPLACE_PUBLISH,
        EXPORT_SEQUENCE, EXPORT_DESCRIBED, EXPORT_RETAINED, CONTROL_RESULT
    } phase_t;
    phase_t phase, after_memory, after_store, after_record, after_copy, after_sequence, after_buffer, after_write;
    reg [1:0] reply_kind;
    localparam [1:0] CONTROL_REPLY=0, JOB_REPLY=1, PREPARE_REPLY=2, FINISH_REPLY=3;
    reg prepared, publishes, publication_committed;
    reg [255:0] job;
    wire [7:0] kind=job[7:0];
    wire [15:0] id0=job[23:8], id1=job[39:24];
    wire [63:0] p0=job[127:64], p1=job[191:128], p2=job[255:192];
    reg [5:0] control_op;
    reg [63:0] a0,a1,a2;
    reg [63:0] input_address;
    reg [31:0] input_count, input_index, previous_offset, run_quantum;
    reg [31:0] used, list_start, destination, scan;
    reg [63:0] own_address;
    reg [31:0] own_write_data, word_data;
    reg [127:0] record_data, copy_data, head_a, head_b;
    reg [63:0] record_address;
    reg [1:0] record_word, record_last, copy_word;
    reg [31:0] sequence_index, buffer_index;
    reg [63:0] sequence_q, sequence_value, handle;
    reg [63:0] buffer_q, buffer_value, span_a, span_b;
    reg [BUFFERS-1:0] buffers_ready;
    (* ram_style="block" *) reg [63:0] sequences[0:LOOPS-1];
    (* ram_style="block" *) reg [63:0] buffers[0:BUFFERS-1];
    typedef struct packed {
        reg [63:0] view_handle, owner, frame;
        reg [31:0] index, order, packet;
        reg has_head, ended;
    } cursor_t;
    cursor_t cursor, cursor_q;
    (* ram_style="block" *) reg [$bits(cursor_t)-1:0] cursors[0:CURSORS-1];
    reg [CURSORS-1:0] cursor_valid;
    wire [7:0] cursor_index=p1[39:32];
    reg [31:0] low,high,middle,index_a,index_b,remaining_a,remaining_b;
    reg valid_a,valid_b,pick_a;
    reg [63:0] new_handle, old_handle, new_frames;
    reg [31:0] new_count;

    reg [3:0] store_op;
    reg [63:0] store_view,store_arg0,store_arg1,store_source;
    reg [31:0] store_count;
    reg [23:0] store_packet;
    reg [1:0] store_length;
    wire store_ready,store_valid;
    wire [3:0] store_error;
    wire [63:0] store_handle,store_frame;
    wire [31:0] store_events,store_order,store_status,store_refs,free_retirements;
    wire [23:0] store_response_packet;
    wire [1:0] store_response_length;
    reg [63:0] result_handle,result_frame;
    reg [31:0] result_events,result_order,result_status,result_refs;
    reg [31:0] result_packet;
    wire store_active=phase==STORE_REQUEST || phase==STORE_WAIT;
    wire store_memory_valid,store_memory_write,store_memory_response_ready;
    wire [63:0] store_memory_address;
    wire [31:0] store_memory_data;
    assign memory_valid=store_active ? store_memory_valid : (phase==READ_REQUEST || phase==WRITE_REQUEST) && !reset;
    assign memory_write=store_active ? store_memory_write : phase==WRITE_REQUEST;
    assign memory_address=store_active ? store_memory_address : own_address;
    assign memory_write_data=store_active ? store_memory_data : own_write_data;
    assign memory_response_ready=store_active ? store_memory_response_ready : phase==READ_WAIT || phase==WRITE_WAIT;
    assign idle=phase==IDLE && !reset;
    assign prepare_ready=idle;
    assign finish_ready=idle && !prepare_valid;
    assign job_ready=idle && !prepare_valid && !finish_valid;
    assign command_ready=idle && control_enable && !job_valid && !prepare_valid && !finish_valid;
    assign job_done=phase==REPLY && reply_kind==JOB_REPLY;
    assign job_error=response_error;
    assign prepare_done=phase==REPLY && reply_kind==PREPARE_REPLY;
    assign prepare_error=response_error;
    assign finish_done=phase==REPLY && reply_kind==FINISH_REPLY;
    assign finish_error=response_error;
    assign publication_valid=phase==REPLY && reply_kind==CONTROL_REPLY && publishes && response_error==0 && !publication_committed;
    assign response_valid=phase==REPLY && reply_kind==CONTROL_REPLY && (!publishes || response_error!=0 || publication_committed);
    assign packet_valid=phase==OUTPUT_SEND && !reset;
    assign packet_data=record_data[119:96];
    assign packet_length=record_data[121:120];
    assign packet_offset=record_data[31:0];
    midi_store #(.BLOCK_EVENTS(BLOCK_EVENTS),.BLOCKS(BLOCKS),.MAX_EVENTS(MAX_EVENTS),
        .VIEWS(VIEWS),.RETIREMENTS(RETIREMENTS),.TABLE_ENTRIES(TABLE_ENTRIES)) content (
        .clk(clk),.reset(reset),.arena_base(arena_base),.arena_bytes(),
        .command_valid(phase==STORE_REQUEST),.command_ready(store_ready),.command_op(store_op),
        .command_view(store_view),.command_arg0(store_arg0),.command_arg1(store_arg1),
        .command_source(store_source),.command_count(store_count),.command_packet(store_packet),.command_length(store_length),
        .response_valid(store_valid),.response_ready(phase==STORE_WAIT),.response_error(store_error),
        .response_handle(store_handle),.response_frame(store_frame),.response_order(store_order),
        .response_packet(store_response_packet),.response_length(store_response_length),
        .response_events(store_events),.response_status(store_status),.response_work(),
        .response_blocks(),.response_owner(),.response_references(store_refs),
        .free_block_slots(),.free_view_slots(),.free_retirement_slots(free_retirements),
        .memory_valid(store_memory_valid),.memory_ready(memory_ready && store_active),.memory_write(store_memory_write),
        .memory_address(store_memory_address),.memory_write_data(store_memory_data),
        .memory_response_valid(memory_response_valid && store_active),.memory_response_ready(store_memory_response_ready),
        .memory_response_error(memory_response_error),.memory_read_data(memory_read_data)
    );
    task automatic fail(input [3:0] code);
        begin response_error<=code;phase<=REPLY;if(code==7) prepared<=0;end
    endtask
    task automatic store_command(input [3:0] op,input [63:0] view_id,input [63:0] arg0,input [63:0] arg1,input phase_t next);
        begin store_op<=op;store_view<=view_id;store_arg0<=arg0;store_arg1<=arg1;after_store<=next;phase<=STORE_REQUEST;end
    endtask
    task automatic load_record(input [63:0] address,input [1:0] last_word,input phase_t next);
        begin record_address<=address;record_word<=0;record_last<=last_word;after_record<=next;phase<=LOAD_WORD;end
    endtask
    task automatic read_word(input [63:0] address,input phase_t next);
        begin own_address<=address;after_memory<=next;phase<=READ_REQUEST;end
    endtask
    task automatic write_word(input [63:0] address,input [31:0] value,input phase_t next);
        begin own_address<=address;own_write_data<=value;after_memory<=next;phase<=WRITE_REQUEST;end
    endtask
    task automatic load_sequence(input [31:0] index,input phase_t next);
        begin
            if(index>=32'(LOOPS)) fail(4);
            else begin sequence_index<=index;after_sequence<=next;phase<=SEQUENCE_REQUEST;end
        end
    endtask
    task automatic load_buffer(input [31:0] index,input phase_t next);
        begin
            if(index>=32'(BUFFERS) || !buffers_ready[BUFFER_BITS'(index)]) fail(4);
            else begin buffer_index<=index;after_buffer<=next;phase<=BUFFER_REQUEST;end
        end
    endtask
    task automatic publish(input [31:0] start,input [31:0] count,input phase_t next);
        begin buffer_index<=destination;buffer_value<={start,count};after_write<=next;phase<=BUFFER_WRITE;end
    endtask
    task automatic append_scratch(input [127:0] value,input phase_t next);
        begin
            if(used>=32'(SCRATCH_EVENTS)) fail(3);
            else begin copy_data<=value;copy_word<=0;after_copy<=next;phase<=COPY_WORD;end
        end
    endtask
    function automatic earlier(input [127:0] left,input [127:0] right);
        earlier={left[31:0],left[63:32],left[95:64]}<={right[31:0],right[63:32],right[95:64]};
    endfunction
    always @(posedge clk) begin
        if(phase==SEQUENCE_REQUEST) sequence_q<=sequences[SEQ_BITS'(sequence_index)];
        if(phase==SEQUENCE_WRITE || phase==FORMAT_SEQUENCES)
            sequences[SEQ_BITS'(phase==FORMAT_SEQUENCES ? scan : sequence_index)]<=phase==FORMAT_SEQUENCES ? 64'b0 : sequence_value;
        if(phase==BUFFER_REQUEST) buffer_q<=buffers[BUFFER_BITS'(buffer_index)];
        if(phase==BUFFER_WRITE) buffers[BUFFER_BITS'(buffer_index)]<=buffer_value;
        if(phase==CURSOR_REQUEST) cursor_q<=cursors[CURSOR_BITS'(cursor_index)];
        if(phase==READ_PUBLISH) cursors[CURSOR_BITS'(cursor_index)]<=cursor;
    end
    always @(posedge clk) begin
        if(reset) begin
            phase<=IDLE;prepared<=0;publishes<=0;publication_committed<=0;reply_kind<=0;
            response_error<=0;response_handle<=0;response_frame<=0;response_count<=0;response_packet<=0;response_length<=0;
            job<=0;control_op<=0;a0<=0;a1<=0;a2<=0;input_address<=0;input_count<=0;input_index<=0;
            previous_offset<=0;run_quantum<=0;used<=0;list_start<=0;destination<=0;scan<=0;
            own_address<=0;own_write_data<=0;word_data<=0;record_data<=0;copy_data<=0;head_a<=0;head_b<=0;
            record_address<=0;record_word<=0;record_last<=0;copy_word<=0;
            sequence_index<=0;buffer_index<=0;sequence_value<=0;buffer_value<=0;handle<=0;span_a<=0;span_b<=0;
            buffers_ready<=0;cursor_valid<=0;cursor<='0;low<=0;high<=0;middle<=0;
            index_a<=0;index_b<=0;remaining_a<=0;remaining_b<=0;valid_a<=0;valid_b<=0;pick_a<=0;
            new_handle<=0;old_handle<=0;new_frames<=0;new_count<=0;packet_destination<=0;
            store_op<=0;store_view<=0;store_arg0<=0;store_arg1<=0;store_source<=0;store_count<=0;store_packet<=0;store_length<=0;
            result_handle<=0;result_frame<=0;result_events<=0;result_order<=0;result_status<=0;result_refs<=0;result_packet<=0;
            after_memory<=IDLE;after_store<=IDLE;after_record<=IDLE;after_copy<=IDLE;after_sequence<=IDLE;after_buffer<=IDLE;after_write<=IDLE;
        end else begin
            if(fault_request) prepared<=0;
            if(publication_valid && publication_commit) publication_committed<=1;
            case(phase)
                IDLE: begin
                    response_error<=0;publishes<=0;publication_committed<=0;
                    if(prepare_valid && prepare_ready) begin
                        reply_kind<=PREPARE_REPLY;run_quantum<=quantum_frames;input_address<=event_address;input_count<=event_count;
                        input_index<=0;previous_offset<=0;used<=0;buffers_ready<=0;
                        if(!prepared) fail(4);
                        else if(quantum_frames==0 || (quantum_frames & (quantum_frames-1))!=0 || event_address[3:0]!=0 ||
                            (event_count!=0 && (event_address<arena_base+arena_bytes || event_address+{28'b0,event_count,4'b0}<event_address))) fail(1);
                        else phase<=PREPARE_NEXT;
                    end else if(finish_valid && finish_ready) begin
                        reply_kind<=FINISH_REPLY;store_command(11,0,64'(MAINTENANCE),0,REPLY);
                    end else if(job_valid && job_ready) begin
                        reply_kind<=JOB_REPLY;job<=job_data;list_start<=used;phase<=DISPATCH;
                    end else if(command_valid && command_ready) begin
                        reply_kind<=CONTROL_REPLY;control_op<=command_op;a0<=command_arg0;a1<=command_arg1;a2<=command_arg2;
                        if(command_op!=0 && !prepared) fail(4);
                        else if((command_op==5 || command_op==6 || command_op==7 || command_op==11) && command_arg0>=64'(LOOPS)) fail(4);
                        else if(command_op==6 && command_arg1>=64'(LOOPS)) fail(4);
                        else case(command_op)
                            0: begin prepared<=0;store_command(0,0,0,0,FORMAT_STORE);end
                            5,7: begin new_handle<=command_op==7 ? 0 : command_arg1;load_sequence(command_arg0[31:0],REPLACE_SEQUENCE);end
                            6: load_sequence(command_arg0[31:0],CONTROL_DUPLICATE);
                            8: store_command(4,0,0,0,CONTROL_RESULT);
                            9: begin store_packet<=command_arg2[23:0];store_length<=command_arg2[25:24];
                                if(command_arg2[63:26]!=0) fail(1);else store_command(5,command_arg0,command_arg1,0,CONTROL_RESULT);end
                            10: store_command(6,command_arg0,command_arg1,0,CONTROL_RESULT);
                            11: load_sequence(command_arg0[31:0],EXPORT_SEQUENCE);
                            12: store_command(10,command_arg0,command_arg1,0,CONTROL_RESULT);
                            13: store_command(9,command_arg0,0,0,CONTROL_RESULT);
                            15: store_command(7,command_arg0,command_arg1,command_arg2,CONTROL_RESULT);
                            20: store_command(11,0,command_arg0>64 ? 64 : command_arg0,0,CONTROL_RESULT);
                            default: fail(2);
                        endcase
                    end
                end
                STORE_REQUEST: if(store_ready) phase<=STORE_WAIT;
                STORE_WAIT: if(store_valid) begin
                    result_handle<=store_handle;result_frame<=store_frame;result_events<=store_events;result_order<=store_order;
                    result_status<=store_status;result_refs<=store_refs;result_packet<={6'b0,store_response_length,store_response_packet};
                    if(store_error!=0) fail(store_error);else phase<=after_store;
                end
                READ_REQUEST: if(memory_ready) phase<=READ_WAIT;
                READ_WAIT: if(memory_response_valid) begin
                    if(memory_response_error) fail(7);else begin word_data<=memory_read_data;phase<=after_memory;end
                end
                WRITE_REQUEST: if(memory_ready) phase<=WRITE_WAIT;
                WRITE_WAIT: if(memory_response_valid) begin if(memory_response_error) fail(7);else phase<=after_memory;end
                LOAD_WORD: read_word(record_address+{60'b0,record_word,2'b0},LOAD_CAPTURE);
                LOAD_CAPTURE: begin
                    record_data[32*record_word+:32]<=word_data;
                    if(record_word==record_last) phase<=after_record;else begin record_word<=record_word+1;phase<=LOAD_WORD;end
                end
                COPY_WORD: write_word(arena_base+CONTENT_BYTES+64'(used)*16+{60'b0,copy_word,2'b0},copy_data[32*copy_word+:32],COPY_NEXT);
                COPY_NEXT: if(copy_word==3) begin used<=used+1;phase<=after_copy;end else begin copy_word<=copy_word+1;phase<=COPY_WORD;end
                SEQUENCE_REQUEST: phase<=SEQUENCE_LATCH;
                SEQUENCE_LATCH: begin handle<=sequence_q;phase<=after_sequence;end
                SEQUENCE_WRITE: phase<=after_write;
                BUFFER_REQUEST: phase<=BUFFER_LATCH;
                BUFFER_LATCH: begin buffer_value<=buffer_q;phase<=after_buffer;end
                BUFFER_WRITE: begin buffers_ready[BUFFER_BITS'(buffer_index)]<=1;phase<=after_write;end
                FORMAT_STORE: begin scan<=0;buffers_ready<=0;cursor_valid<=0;used<=0;phase<=FORMAT_SEQUENCES;end
                FORMAT_SEQUENCES: if(scan==32'(LOOPS)-1) begin prepared<=1;phase<=REPLY;end else scan<=scan+1;
                PREPARE_NEXT: if(input_index==input_count) phase<=REPLY;
                    else load_record(input_address+64'(input_index)*16,2,PREPARE_CHECK);
                PREPARE_CHECK: begin
                    if(record_data[31:0]>=32'(INPUTS) || record_data[63:32]>=run_quantum ||
                        (input_index!=0 && record_data[63:32]<previous_offset) || record_data[95:90]!=0 ||
                        packet_error(record_data[87:64],record_data[89:88])!=0) fail(1);
                    else begin previous_offset<=record_data[63:32];input_index<=input_index+1;phase<=PREPARE_NEXT;end
                end
                DISPATCH: begin
                    if(!prepared) fail(4);
                    else case(kind)
                        `SHOOP_SP_MIDI_INPUT: begin
                            destination<={16'b0,id1};input_index<=0;
                            if(32'(id0)>=INPUTS || 32'(id1)>=BUFFERS || buffers_ready[BUFFER_BITS'(id1)]) fail(4);else phase<=INPUT_NEXT;
                        end
                        `SHOOP_SP_MIDI_EMPTY: begin
                            destination<={16'b0,id0};
                            if(32'(id0)>=BUFFERS || buffers_ready[BUFFER_BITS'(id0)]) fail(4);else begin buffer_index<={16'b0,id0};buffer_value<={used,32'b0};after_write<=REPLY;phase<=BUFFER_WRITE;end
                        end
                        `SHOOP_SP_MIDI_BEGIN_CAPTURE: load_sequence({16'b0,id0},BEGIN_CAPTURE);
                        `SHOOP_SP_MIDI_FINISH_CAPTURE: load_sequence({16'b0,id0},FINISH_CAPTURE);
                        `SHOOP_SP_MIDI_APPEND: load_sequence({16'b0,id0},APPEND_SEQUENCE);
                        `SHOOP_SP_MIDI_READ: begin
                            destination<={16'b0,id1};
                            if(32'(id1)>=BUFFERS || buffers_ready[BUFFER_BITS'(id1)] || {24'b0,cursor_index}>=CURSORS || p1[31:0]!=run_quantum || p0+{32'b0,p1[31:0]}<p0) fail(4);
                            else load_sequence({16'b0,id0},CURSOR_REQUEST);
                        end
                        `SHOOP_SP_MIDI_MERGE: begin
                            destination<=p0[31:0];
                            if(p0>=64'(BUFFERS) || buffers_ready[BUFFER_BITS'(p0)] || p0=={48'b0,id0} || p0=={48'b0,id1}) fail(4);
                            else load_buffer({16'b0,id0},MERGE_LEFT);
                        end
                        `SHOOP_SP_MIDI_OUTPUT: begin packet_destination<={16'b0,id1};if(32'(id1)>=DESTINATIONS) fail(4);else load_buffer({16'b0,id0},OUTPUT_BUFFER);end
                        default: fail(2);
                    endcase
                end
                INPUT_NEXT: if(input_index==input_count) publish(list_start,used-list_start,REPLY);
                    else load_record(input_address+64'(input_index)*16,2,INPUT_CHECK);
                INPUT_CHECK: if(record_data[31:0]=={16'b0,id0})
                    append_scratch({record_data[95:64],input_index,16'h0001,id0,record_data[63:32]},INPUT_ADVANCE);
                    else phase<=INPUT_ADVANCE;
                INPUT_ADVANCE: begin input_index<=input_index+1;phase<=INPUT_NEXT;end
                BEGIN_CAPTURE: begin
                    old_handle<=handle;
                    if(handle!=0) store_command(12,handle,0,0,BEGIN_OLD_DESCRIBED);else store_command(4,0,0,0,BEGIN_CREATED);
                end
                BEGIN_OLD_DESCRIBED: if(result_refs==1 && free_retirements==0) fail(3);else store_command(4,0,0,0,BEGIN_CREATED);
                BEGIN_CREATED: begin
                    new_handle<=result_handle;
                    if(old_handle!=0) store_command(9,old_handle,0,0,BEGIN_RELEASED);else begin sequence_value<=result_handle;after_write<=REPLY;phase<=SEQUENCE_WRITE;end
                end
                BEGIN_RELEASED: begin sequence_value<=new_handle;after_write<=REPLY;phase<=SEQUENCE_WRITE;end
                FINISH_CAPTURE: if(handle==0) fail(4);else store_command(12,handle,0,0,FINISH_DESCRIBED);
                FINISH_DESCRIBED: store_command(6,handle,result_frame,0,REPLY);
                APPEND_SEQUENCE: if(handle==0) fail(4);else load_buffer({16'b0,id1},APPEND_BUFFER);
                APPEND_BUFFER: begin
                    store_source<=arena_base+CONTENT_BYTES+64'(buffer_value[63:32])*16;store_count<=buffer_value[31:0];
                    if(p1[63:32]!=0 || p1[31:0]!=run_quantum || p0+p1<p0) fail(6);else store_command(15,handle,p0,p0+p1,REPLY);
                end
                CURSOR_REQUEST: phase<=CURSOR_LATCH;
                CURSOR_LATCH: begin cursor<=cursor_q;phase<=READ_BEGIN;end
                READ_BEGIN: begin
                    if(handle==0) fail(4);
                    else if(!cursor_valid[CURSOR_BITS'(cursor_index)] || cursor.view_handle!=handle || cursor.owner!=p2 || p1[56]) begin
                        cursor<='0;cursor.view_handle<=handle;cursor.owner<=p2;
                        store_command(12,handle,0,0,READ_DESCRIBED);
                    end else phase<=READ_NEXT;
                end
                READ_DESCRIBED: if(result_status!=2) fail(8);
                    else if(p0!=0) begin low<=0;high<=result_events;phase<=SEARCH;end else phase<=READ_NEXT;
                SEARCH: if(low==high) begin cursor.index<=low;phase<=READ_NEXT;end
                    else begin middle<=low+((high-low)>>1);store_command(10,handle,{32'b0,low+((high-low)>>1)},0,SEARCH_COMPARE);end
                SEARCH_COMPARE: begin if(result_frame<p0) low<=middle+1;else high<=middle;phase<=SEARCH;end
                READ_NEXT: if(cursor.ended) phase<=READ_PUBLISH;
                    else if(cursor.has_head) phase<=READ_HEAD;
                    else store_command(10,handle,{32'b0,cursor.index},0,READ_FETCHED);
                READ_FETCHED: if(result_packet[25:24]==0) begin cursor.ended<=1;phase<=READ_PUBLISH;end
                    else begin cursor.frame<=result_frame;cursor.order<=result_order;cursor.packet<=result_packet;cursor.has_head<=1;phase<=READ_HEAD;end
                READ_HEAD: if(cursor.frame>=p0+{32'b0,run_quantum}) phase<=READ_PUBLISH;
                    else if(cursor.frame>=p0 && (cursor.packet[7:0]>=8'hf0 || p1[6'd40+{2'b0,cursor.packet[3:0]}]))
                        append_scratch({cursor.packet,cursor.order,24'b0,cursor_index,32'(cursor.frame-p0)},READ_CONSUME);
                    else phase<=READ_CONSUME;
                READ_CONSUME: if(cursor.index==32'hffffffff) fail(6);
                    else begin cursor.index<=cursor.index+1;cursor.has_head<=0;phase<=READ_NEXT;end
                READ_PUBLISH: begin cursor_valid[CURSOR_BITS'(cursor_index)]<=1;publish(list_start,used-list_start,REPLY);end
                MERGE_LEFT: begin span_a<=buffer_value;load_buffer({16'b0,id1},MERGE_RIGHT);end
                MERGE_RIGHT: begin
                    span_b<=buffer_value;
                    if(span_a[31:0]==0 || buffer_value[31:0]==0) begin
                        if(span_a[31:0]==0) publish(buffer_value[63:32],buffer_value[31:0],REPLY);else publish(span_a[63:32],span_a[31:0],REPLY);
                    end else if({1'b0,span_a[31:0]}+{1'b0,buffer_value[31:0]}>33'(SCRATCH_EVENTS)-{1'b0,used}) fail(3);
                    else begin index_a<=span_a[63:32];index_b<=buffer_value[63:32];remaining_a<=span_a[31:0];remaining_b<=buffer_value[31:0];valid_a<=0;valid_b<=0;phase<=MERGE_NEXT;end
                end
                MERGE_NEXT: if(!valid_a && remaining_a!=0) load_record(arena_base+CONTENT_BYTES+64'(index_a)*16,3,MERGE_A);
                    else if(!valid_b && remaining_b!=0) load_record(arena_base+CONTENT_BYTES+64'(index_b)*16,3,MERGE_B);
                    else if(!valid_a && !valid_b) publish(list_start,used-list_start,REPLY);
                    else phase<=MERGE_CHOOSE;
                MERGE_A: begin head_a<=record_data;valid_a<=1;phase<=MERGE_NEXT;end
                MERGE_B: begin head_b<=record_data;valid_b<=1;phase<=MERGE_NEXT;end
                MERGE_CHOOSE: begin
                    pick_a<=valid_a && (!valid_b || earlier(head_a,head_b));
                    append_scratch(valid_a && (!valid_b || earlier(head_a,head_b)) ? head_a : head_b,MERGE_ADVANCE);
                end
                MERGE_ADVANCE: begin
                    if(pick_a) begin valid_a<=0;remaining_a<=remaining_a-1;index_a<=index_a+1;end
                    else begin valid_b<=0;remaining_b<=remaining_b-1;index_b<=index_b+1;end
                    phase<=MERGE_NEXT;
                end
                OUTPUT_BUFFER: begin index_a<=buffer_value[63:32];remaining_a<=buffer_value[31:0];phase<=OUTPUT_NEXT;end
                OUTPUT_NEXT: if(remaining_a==0) phase<=REPLY;else load_record(arena_base+CONTENT_BYTES+64'(index_a)*16,3,OUTPUT_SEND);
                OUTPUT_SEND: if(packet_ready) begin
                    if(packet_refused) fail(9);else begin remaining_a<=remaining_a-1;index_a<=index_a+1;phase<=OUTPUT_NEXT;end
                end
                CONTROL_DUPLICATE: begin new_handle<=handle;load_sequence(a1[31:0],REPLACE_SEQUENCE);end
                REPLACE_SEQUENCE: begin
                    old_handle<=handle;
                    if(new_handle!=0) store_command(12,new_handle,0,0,REPLACE_DESCRIBED);
                    else begin new_frames<=0;new_count<=0;phase<=REPLACE_CHECK_OLD;end
                end
                REPLACE_DESCRIBED: begin
                    if(result_status!=2) fail(8);
                    else if((result_frame & (64'(quantum_frames)-1))!=0 || result_refs==32'hffffffff) fail(4);
                    else begin new_frames<=result_frame;new_count<=result_events;phase<=REPLACE_CHECK_OLD;end
                end
                REPLACE_CHECK_OLD: if(old_handle!=0 && old_handle!=new_handle) store_command(12,old_handle,0,0,REPLACE_OLD_DESCRIBED);else phase<=REPLACE_RETAIN;
                REPLACE_OLD_DESCRIBED: if(result_refs==1 && free_retirements==0) fail(3);else phase<=REPLACE_RETAIN;
                REPLACE_RETAIN: if(new_handle!=0 && new_handle!=old_handle) store_command(8,new_handle,0,0,REPLACE_RELEASE);else phase<=REPLACE_RELEASE;
                REPLACE_RELEASE: if(old_handle!=0 && old_handle!=new_handle) store_command(9,old_handle,0,0,REPLACE_WRITE);else phase<=REPLACE_WRITE;
                REPLACE_WRITE: begin sequence_value<=new_handle;after_write<=REPLACE_PUBLISH;phase<=SEQUENCE_WRITE;end
                REPLACE_PUBLISH: begin response_handle<=new_handle;response_frame<=new_frames;response_count<=new_count;publishes<=1;phase<=REPLY;end
                EXPORT_SEQUENCE: if(handle==0) fail(4);else store_command(12,handle,0,0,EXPORT_DESCRIBED);
                EXPORT_DESCRIBED: begin new_frames<=result_frame;new_count<=result_events;store_command(8,handle,0,0,EXPORT_RETAINED);end
                EXPORT_RETAINED: begin response_handle<=handle;response_frame<=new_frames;response_count<=new_count;phase<=REPLY;end
                CONTROL_RESULT: begin response_handle<=result_handle;response_frame<=result_frame;response_count<=control_op==12 ? result_order : result_events;
                    response_packet<=result_packet[23:0];response_length<=result_packet[25:24];phase<=REPLY;end
                REPLY: if(reply_kind!=CONTROL_REPLY || (response_valid && response_ready)) phase<=IDLE;
                default: fail(4);
            endcase
        end
    end
endmodule
