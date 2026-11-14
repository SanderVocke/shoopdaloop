module audio_actions #(
    parameter integer SLOTS=128, QUANTUM=32, MAX_WORK=8000, ACTIVATION_WORK=7200,
    parameter integer WATCHDOG=16384, NATIVE_ACTIONS=0
) (
    input wire clk, input wire reset, input wire [63:0] current_frame,
    input wire cut_start, input wire [1:0] cut_mode, output reg cut_done, output wire busy, output reg fault,
    input wire request_valid, output wire request_ready, input wire [7:0] request_opcode,
    input wire request_state, input wire [15:0] request_address, input wire [639:0] request_data,
    output reg response_valid, output reg [3:0] response_error, output reg [831:0] response_data,
    output wire cfg_valid, input wire cfg_ready, output wire [7:0] cfg_opcode,
    output wire cfg_state, output wire [15:0] cfg_address, output wire [639:0] cfg_data,
    input wire cfg_response_valid, input wire [3:0] cfg_response_error, input wire [128:0] cfg_response_data,
    output wire resident_valid, input wire resident_ready, output wire [7:0] resident_opcode,
    output wire [511:0] resident_data, output wire [63:0] resident_sequence, output wire [63:0] resident_frame,
    output wire [47:0] resident_reference, output wire [15:0] resident_slot, output wire [31:0] resident_generation,
    output wire [1:0] resident_mode,
    input wire resident_done, input wire resident_pending, input wire [3:0] resident_error, input wire [511:0] resident_result,
    input wire lookup_valid, output wire lookup_ready, input wire [15:0] lookup_slot,
    output reg lookup_done, output reg [3:0] lookup_error, output wire [767:0] lookup_data,
    input wire update_valid, output wire update_ready, input wire [15:0] update_slot,
    input wire [31:0] update_generation, input wire [767:0] update_data,
    output reg update_done, output reg [3:0] update_error,
    output wire record_request_valid, input wire record_request_ready, output wire record_request_ack,
    output wire [15:0] record_request_slot, output wire [31:0] record_request_generation,
    output wire [31:0] record_request_epoch, output wire [4:0] record_request_word,
    input wire record_response_valid, input wire [3:0] record_response_error, input wire [63:0] record_response_data
);
    localparam integer A=SLOTS > 1 ? $clog2(SLOTS) : 1;
    localparam integer B=$clog2(2*SLOTS);
    localparam integer W=$clog2(MAX_WORK+ACTIVATION_WORK+129);
    localparam [3:0] CAPACITY=2, INVALID_ID=3, INVALID_STATE=5, EXHAUSTED=8, UNSUPPORTED=10;
    typedef enum reg [4:0] {CLEAR, IDLE, HOST_READ, HOST_EVAL, SEND, REPLY,
        NODE_READ, NODE_WAIT, ROW_READ, ROW_WAIT, VISIT, COMMIT_DONE, CHECK_ORDER, PUBLISH_ROW, EXECUTE_ROW, RECORD_SEND, RECORD_WAIT} phase_t;
    phase_t phase;
    reg [31:0] cursor, active_count, stage_count, written, elapsed;
    reg active_bank, staging, committing, applying, executing;
    reg [63:0] version, nonce, cut_frame, previous_frame, previous_sequence, selected_effective;
    reg previous_valid;
    reg [W-1:0] work;
    reg [7:0] host_opcode, native_opcode;
    reg host_state, native_state;
    reg [15:0] native_address;
    reg [639:0] host_data, native_data;
    reg [A-1:0] selected_slot;
    reg [31:0] selected_generation;
    reg [47:0] selected_reference;
    reg [1:0] mode;
    reg lookup_guard, update_pending;
    reg [15:0] guarded_slot, lookup_address;
    reg [31:0] guarded_generation;
    reg [767:0] selected_row;
    reg row_write;
    reg [A-1:0] row_write_address;
    reg [767:0] row_write_data;
    wire [767:0] row_data;
    wire [95:0] node_data;
    wire [B-1:0] node_base=B'(committing ? !active_bank : active_bank)*B'(SLOTS);
    wire [B-1:0] node_read_address=node_base+B'(cursor);
    wire [B-1:0] node_write_address=B'(!active_bank)*B'(SLOTS)+B'(request_address);
    wire accept=request_valid && request_ready;
    wire node_write=accept && request_opcode == 17 && staging
        && request_data[127:64] == nonce && 32'(request_address) == written && written < stage_count
        && {16'b0,request_data[15:0]} < 32'(SLOTS) && request_data[47:16] != 0;
    wire [63:0] effective=selected_row[96:33] < cut_frame ? cut_frame : selected_row[96:33];
    wire [W-1:0] cost=selected_row[168:161] == 5 || selected_row[168:161] == 17 || selected_row[168:161] == 18 ? W'(ACTIVATION_WORK)
        : selected_row[168:161] == 6 ? W'(128) : W'(16);
    wire unused=&{1'b0,host_data[639:576],host_state,node_data[15:0]};
    assign busy=phase != IDLE;
    assign request_ready=phase == IDLE && !cut_start && !fault && !reset;
    assign cfg_valid=phase == SEND && native_opcode < 9;
    assign cfg_opcode=native_opcode;
    assign cfg_state=native_state;
    assign cfg_address=native_address;
    assign cfg_data=native_data;
    assign resident_valid=phase == SEND && native_opcode >= 9;
    assign resident_opcode=native_opcode;
    assign resident_data=native_data[511:0];
    assign resident_sequence=selected_row[160:97];
    assign resident_frame=cut_frame;
    assign resident_reference=selected_reference;
    assign resident_slot=16'(selected_slot);
    assign resident_generation=selected_row[32:1];
    assign resident_mode=mode;
    assign record_request_valid=phase == RECORD_SEND && !fault;
    assign record_request_ack=host_opcode == 20;
    assign record_request_slot=16'(selected_slot);
    assign record_request_generation=host_opcode == 20 ? selected_row[32:1] : host_data[31:0];
    assign record_request_epoch=host_opcode == 20 ? selected_row[334:303] : host_data[63:32];
    assign record_request_word=host_data[68:64];
    assign lookup_ready=NATIVE_ACTIONS != 0 && phase == REPLY && native_opcode >= 9
        && !lookup_done && !update_valid && !update_pending && !row_write;
    wire lookup_accept=lookup_valid && lookup_ready;
    wire lookup_in_range=32'(lookup_slot) < 32'(SLOTS);
    assign lookup_data=lookup_error == 0 ? row_data : 768'b0;
    assign update_ready=NATIVE_ACTIONS != 0 && phase == REPLY && native_opcode >= 9
        && lookup_guard && !lookup_done && !update_pending && !row_write;
    wire update_accept=update_valid && update_ready;
    sp_ram #(.WIDTH(768),.DEPTH(SLOTS),.ADDR_BITS(A)) rows (
        .clk(clk),.read_enable(phase == HOST_READ || phase == ROW_READ || (lookup_accept && lookup_in_range)),
        .read_address(lookup_accept ? A'(lookup_slot) : selected_slot),.read_data(row_data),
        .write_enable(row_write),.write_address(row_write_address),.write_data(row_write_data)
    );
    sp_ram #(.WIDTH(96),.DEPTH(2*SLOTS),.ADDR_BITS(B)) order_memory (
        .clk(clk),.read_enable(phase == NODE_READ),.read_address(node_read_address),.read_data(node_data),
        .write_enable(node_write),.write_address(node_write_address),
        .write_data({request_data[175:128],request_data[47:0]})
    );
    task automatic reply_empty(input [3:0] error);
        begin response_valid<=1; response_error<=error; phase<=IDLE; end
    endtask
    task automatic reply(input [3:0] error, input [831:0] data);
        begin reply_empty(error); if (error == 0) response_data<=data; end
    endtask
    task automatic write_row(input [A-1:0] slot, input [767:0] data);
        begin row_write<=1; row_write_address<=slot; row_write_data<=data; end
    endtask
    task automatic next_node;
        begin
            if (cursor+1 == (committing ? stage_count : active_count)) begin
                if (committing) phase<=COMMIT_DONE;
                else begin cut_done<=1; phase<=IDLE; end
            end else begin cursor<=cursor+1'b1; phase<=NODE_READ; end
        end
    endtask
    task automatic terminal(input [3:0] error, input [511:0] data);
        reg [767:0] result_row;
        begin
            result_row=selected_row; result_row[186]=1; result_row[190:187]=error;
            if (selected_row[168:161] >= 12) result_row[169]=1;
            result_row[254:191]=mode == 1 && (selected_row[168:161] == 9 || selected_row[168:161] == 10)
                && data[511:504] == 0 && error == 0 ? data[460:397] : cut_frame;
            result_row[766:255]=data;
            write_row(selected_slot,result_row); next_node();
        end
    endtask
    initial if (SLOTS < 1 || SLOTS > 65536 || QUANTUM < 1 || (QUANTUM & (QUANTUM-1)) != 0
        || MAX_WORK < 1 || WATCHDOG < MAX_WORK+7*SLOTS+32)
        $fatal(1,"Invalid resident configuration profile");
    always @(posedge clk) begin : controller
        reg [767:0] updated;
        reg [63:0] rounded;
        row_write<=0; response_valid<=0; cut_done<=0; lookup_done<=0; update_done<=update_pending; update_pending<=0;
        if (reset) begin
            phase<=CLEAR; cursor<=0; active_count<=0; stage_count<=0; written<=0;
            active_bank<=0; staging<=0; committing<=0; applying<=0; executing<=0;
            version<=0; nonce<=0; cut_frame<=0; previous_frame<=0; previous_sequence<=0;
            previous_valid<=0; selected_effective<=0; work<=0; elapsed<=0; fault<=0;
            host_opcode<=0; host_state<=0; host_data<=0;
            native_opcode<=0; native_state<=0; native_address<=0; native_data<=0;
            selected_slot<=0; selected_generation<=0; selected_reference<=0; selected_row<=0;
            mode<=0; lookup_guard<=0; update_pending<=0; guarded_slot<=0; guarded_generation<=0; lookup_address<=0;
            lookup_done<=0; lookup_error<=0; update_done<=0; update_error<=0;
            row_write<=0; row_write_address<=0; row_write_data<=0;
            response_valid<=0; response_error<=0; response_data<=0; cut_done<=0;
        end else if (!fault) begin
            lookup_done<=lookup_accept;
            if (phase != REPLY || lookup_accept) lookup_guard<=0;
            if (lookup_accept) begin lookup_error<=lookup_in_range ? 4'b0 : INVALID_ID; lookup_address<=lookup_slot; end
            if (lookup_done && lookup_error == 0) begin
                lookup_guard<=row_data[0] && row_data[767]; guarded_slot<=lookup_address; guarded_generation<=row_data[32:1];
            end
            if (update_accept) begin
                lookup_guard<=0; update_pending<=1;
                if (update_slot != guarded_slot || update_generation != guarded_generation
                    || !update_data[0] || !update_data[767] || update_data[32:1] != guarded_generation) update_error<=INVALID_ID;
                else begin update_error<=0; write_row(A'(guarded_slot),update_data); end
            end
            if (phase != IDLE && phase != CLEAR) begin
                if (elapsed == 32'(WATCHDOG-1)) fault<=1;
                else elapsed<=elapsed+1'b1;
            end
            case (phase)
                CLEAR: begin
                    updated=0; updated[32:1]=1; write_row(A'(cursor),updated);
                    if (cursor+1 == 32'(SLOTS)) begin cursor<=0; phase<=IDLE; end
                    else cursor<=cursor+1'b1;
                end
                IDLE: begin
                    elapsed<=0;
                    if (cut_start) begin
                        cut_frame<=current_frame; cursor<=0; work<=0; committing<=0; executing<=1;
                        mode<=cut_mode;
                        if (cut_mode == 3) fault<=1;
                        else if (active_count == 0) cut_done<=1; else phase<=NODE_READ;
                    end else if (accept) begin
                        response_data<=0;
                        executing<=0; host_opcode<=request_opcode; host_state<=request_state;
                        host_data<=request_data;
                        if (request_opcode <= 8) begin
                            native_opcode<=request_opcode; native_state<=request_state;
                            native_address<=request_address; native_data<=request_data; phase<=SEND;
                        end else if (request_opcode == 16) begin
                            if (request_data[63:0] != version) reply_empty(INVALID_ID);
                            else if (request_data[95:64] > 32'(SLOTS)) reply_empty(CAPACITY);
                            else if (&nonce) reply_empty(EXHAUSTED);
                            else begin
                                nonce<=nonce+1'b1; staging<=1; stage_count<=request_data[95:64]; written<=0;
                                reply(0,{768'b0,nonce+64'd1});
                            end
                        end else if (request_opcode == 17) begin
                            if (!staging || request_data[127:64] != nonce) reply_empty(INVALID_ID);
                            else if (!node_write) reply_empty(INVALID_STATE);
                            else begin written<=written+1'b1; reply_empty(0); end
                        end else if (request_opcode == 18) begin
                            if (!staging || request_data[63:0] != nonce) reply_empty(INVALID_ID);
                            else if (written != stage_count) reply_empty(INVALID_STATE);
                            else if (&version) reply_empty(EXHAUSTED);
                            else begin
                                committing<=1; applying<=0; previous_valid<=0; cursor<=0; cut_frame<=current_frame;
                                phase<=stage_count == 0 ? COMMIT_DONE : NODE_READ;
                            end
                        end else if (request_opcode == 21) reply(0,{575'b0,staging,current_frame,nonce,version,64'(active_count)});
                        else if (request_opcode == 48) begin
                            if (NATIVE_ACTIONS == 0) reply_empty(UNSUPPORTED);
                            else if (32'(request_address) >= 32'(SLOTS) || request_data[71:64] >= 30) reply_empty(INVALID_ID);
                            else begin selected_slot<=A'(request_address); phase<=RECORD_SEND; end
                        end
                        else if ((request_opcode >= 19 && request_opcode <= 24 || request_opcode[7])
                            && 32'(request_address) < 32'(SLOTS)) begin
                            selected_slot<=A'(request_address); phase<=HOST_READ;
                        end else reply_empty(UNSUPPORTED);
                    end
                end
                HOST_READ: phase<=HOST_EVAL;
                HOST_EVAL: begin
                    if (host_opcode[7]) begin
                        rounded=(host_data[447:384]+64'(QUANTUM-1)) & ~64'(QUANTUM-1);
                        if (!(host_opcode[6:0] == 0 || host_opcode[6:0] == 5 || host_opcode[6:0] == 6 || host_opcode[6:0] == 8
                            || (NATIVE_ACTIONS != 0 && host_opcode[6:0] >= 9 && host_opcode[6:0] <= 20))) reply_empty(UNSUPPORTED);
                        else if (row_data[32:1] == 0 || host_data[543:512] != row_data[32:1]) reply_empty(INVALID_ID);
                        else if (row_data[0] && row_data[767]) reply_empty(INVALID_STATE);
                        else if (rounded < host_data[447:384]) reply_empty(EXHAUSTED);
                        else begin
                            updated=0; updated[0]=1; updated[32:1]=row_data[32:1]; updated[96:33]=rounded;
                            updated[160:97]=host_data[511:448]; updated[168:161]={1'b0,host_opcode[6:0]};
                            updated[169]=host_opcode[6:0] >= 9 ? 1'b0 : host_data[560]; updated[185:170]=host_data[559:544];
                            updated[766:255]=host_opcode[6:0] >= 9 ? 512'b0 : {288'b0,host_data[223:0]};
                            write_row(selected_slot,updated); reply(0,{64'b0,updated});
                        end
                    end else if (!row_data[0] || row_data[32:1] != host_data[31:0]) reply_empty(INVALID_ID);
                    else if (host_opcode == 24) begin
                        if (row_data[767] || row_data[168:161] < 9 || NATIVE_ACTIONS == 0) reply_empty(INVALID_STATE);
                        else begin updated=row_data; updated[169]=1; updated[766:255]=host_data[575:64];
                            write_row(selected_slot,updated); reply(0,{64'b0,updated}); end
                    end else if (host_opcode == 19) reply(0,{64'b0,row_data});
                    else if (host_opcode == 23) begin
                        if (row_data[168:161] >= 9 && (row_data[172] || row_data[171:170] == 0)) reply_empty(INVALID_ID);
                        else if (row_data[168:161] >= 9 && (row_data[186] || !row_data[169])) reply_empty(INVALID_STATE);
                        else if (row_data[186]) reply(0,{64'b0,row_data});
                        else begin
                            updated=row_data; updated[186]=1; updated[190:187]=11; updated[254:191]=current_frame;
                            updated[766:255]=0; write_row(selected_slot,updated); reply(0,{64'b0,updated});
                        end
                    end else if (host_opcode == 22 ? row_data[767] : !row_data[186]) reply_empty(INVALID_STATE);
                    else if (host_opcode == 20 && row_data[168:161] >= 9 && row_data[766:759] == 5) begin
                        if (row_data[270:255] != 16'(selected_slot) || row_data[302:271] != row_data[32:1]) begin fault<=1;reply_empty(INVALID_STATE);end
                        else begin selected_row<=row_data;phase<=RECORD_SEND;end
                    end
                    else begin
                        updated=0; updated[32:1]=&row_data[32:1] ? 32'b0 : row_data[32:1]+1'b1;
                        write_row(selected_slot,updated); reply(0,{64'b0,row_data});
                    end
                end
                RECORD_SEND: if (record_request_ready) phase<=RECORD_WAIT;
                RECORD_WAIT: if (record_response_valid) begin
                    if (record_response_error != 0) begin
                        if (host_opcode == 20) fault<=1;
                        reply_empty(record_response_error);
                    end else if (host_opcode == 20) begin
                        updated=0; updated[32:1]=&selected_row[32:1] ? 32'b0 : selected_row[32:1]+1'b1;
                        write_row(selected_slot,updated); reply(0,{64'b0,selected_row});
                    end else reply(0,{672'b0,host_data[63:32],host_data[31:0],16'(selected_slot),8'b0,host_data[71:64],record_response_data});
                end
                SEND: if (native_opcode < 9 ? cfg_ready : resident_ready) phase<=REPLY;
                REPLY: if (!update_accept && (native_opcode < 9 ? cfg_response_valid : resident_done)) begin
                    if (native_opcode >= 9) begin
                        if (resident_error != 0) fault<=1;
                        else if (mode == 2) begin
                            if (resident_error != 0) fault<=1;
                            else begin updated=selected_row; updated[169]=1; write_row(selected_slot,updated); next_node(); end
                        end else if (resident_pending) begin
                            updated=selected_row; updated[169]=0; updated[766:255]=resident_result;
                            write_row(selected_slot,updated); next_node();
                        end else terminal(resident_error,resident_error == 0 ? resident_result : 512'b0);
                    end else if (executing) terminal(cfg_response_error,cfg_response_error == 0 ? {383'b0,cfg_response_data} : 512'b0);
                    else reply(cfg_response_error,cfg_response_error == 0 ? {703'b0,cfg_response_data} : 832'b0);
                end
                NODE_READ: phase<=NODE_WAIT;
                NODE_WAIT: begin
                    selected_slot<=A'(node_data[15:0]); selected_generation<=node_data[47:16]; selected_reference<=node_data[95:48]; phase<=ROW_READ;
                end
                ROW_READ: phase<=ROW_WAIT;
                ROW_WAIT: begin selected_row<=row_data; phase<=VISIT; end
                VISIT: begin
                    if (committing) begin
                        if (!selected_row[0] || selected_row[32:1] != selected_generation || selected_row[186]) next_node();
                        else if (!applying && !selected_row[767] && selected_row[168:161] >= 9 && !selected_row[169]) reply_empty(INVALID_STATE);
                        else if (!applying || !selected_row[767]) begin
                            selected_effective<=effective;
                            phase<=applying ? PUBLISH_ROW : CHECK_ORDER;
                        end else next_node();
                    end else if (selected_row[0] && selected_row[767] && selected_row[32:1] == selected_generation) begin
                        if (mode == 0 && !selected_row[186] && selected_row[96:33] <= cut_frame
                            && !(selected_row[168:161] >= 9 && selected_row[168:161] <= 10 && !selected_row[169])) phase<=EXECUTE_ROW;
                        else if (mode == 1 && !selected_row[186] && !selected_row[169]
                            && (selected_row[168:161] == 9 || selected_row[168:161] == 10
                                || selected_row[168:161] == 12 || selected_row[168:161] == 13)) phase<=EXECUTE_ROW;
                        else if (mode == 2 && selected_row[186] && !selected_row[169]
                            && (selected_row[168:161] == 9 || selected_row[168:161] == 10)) phase<=EXECUTE_ROW;
                        else next_node();
                    end else next_node();
                end
                CHECK_ORDER: begin
                    if (previous_valid && (selected_effective < previous_frame
                        || (selected_effective == previous_frame && selected_row[160:97] < previous_sequence))) reply_empty(INVALID_STATE);
                    else begin previous_valid<=1; previous_frame<=selected_effective; previous_sequence<=selected_row[160:97]; next_node(); end
                end
                PUBLISH_ROW: begin
                    updated=selected_row; updated[767]=1; updated[96:33]=selected_effective;
                    write_row(selected_slot,updated); next_node();
                end
                EXECUTE_ROW: begin
                    if (work+cost > W'(MAX_WORK)) terminal(CAPACITY,0);
                    else begin
                        work<=work+cost; native_opcode<=selected_row[168:161]; native_state<=selected_row[169];
                        native_address<=selected_row[185:170]; native_data<={128'b0,selected_row[766:255]}; phase<=SEND;
                    end
                end
                COMMIT_DONE: begin
                    if (!applying && stage_count != 0) begin applying<=1; cursor<=0; phase<=NODE_READ; end
                    else begin active_bank<=!active_bank; active_count<=stage_count; staging<=0; version<=version+1'b1; reply(0,{704'b0,current_frame,version+64'd1}); end
                end
                default: fault<=1;
            endcase
        end
    end
endmodule
