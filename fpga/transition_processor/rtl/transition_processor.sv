module transition_processor #(
    parameter integer LOOP_DEPTH = 64,
    parameter integer REQUEST_DEPTH = 64,
    parameter integer EVENT_DEPTH = 192,
    parameter integer INTEGRATED = 0,
    parameter integer LA = (LOOP_DEPTH > 1) ? $clog2(LOOP_DEPTH) : 1,
    parameter integer RA = (REQUEST_DEPTH > 1) ? $clog2(REQUEST_DEPTH) : 1,
    parameter integer EA = (EVENT_DEPTH > 1) ? $clog2(EVENT_DEPTH) : 1,
    parameter integer SA = (LOOP_DEPTH > 1) ? $clog2(LOOP_DEPTH) : 1
) (
    input wire clk,
    input wire reset,
    input wire command_valid,
    output wire command_ready,
    input wire [7:0] command_opcode,
    input wire [31:0] command_index,
    input wire [639:0] command_data,
    output reg response_valid,
    input wire response_ready,
    output reg [3:0] response_error,
    output reg [831:0] response_data,
    output wire busy,
    input wire loop_read_valid,
    output wire loop_read_ready,
    input wire [31:0] loop_read_slot,
    output reg loop_response_valid,
    output reg loop_response_error,
    output wire [163:0] loop_response_data,
    output wire [31:0] configured_quantum
);
    localparam [3:0] OK=0, INVALID_CONFIG=1, CAPACITY=2, INVALID_ID=3,
        DUPLICATE_ID=4, INVALID_STATE=5, INVALID_FRAME=6, MISSING_SYNC=7,
        EXHAUSTED=8, EMPTY_CONTENT=9, UNSUPPORTED=10;
    localparam [1:0] IDLE_PHASE=0, EXECUTING=1, SETTLED=2, FAULTED=3;
    localparam [7:0] STATUS=0, WRITE_SPEC=1, WRITE_ORDER=2, PREPARE=3,
        INSTALL=4, CANCEL=5, COLLECT=6, PROCESS=9, FINISH=11, READ_STATE=12,
        READ_REQUEST=13, READ_EVENT=15, SET_REPEAT=17;
    localparam integer VALID=0, GENERATION=1, MODE=33, REPEAT=35, LAST_VALID=36,
        LENGTH=64, POSITION=128, CAPTURE=192, CYCLES=256,
        CONTENT_REV=320, CAPTURE_REV=384, WATERMARK=448, LAST_PULSE=512,
        INTENT_VALID=576, INTENT_SLOT=577, INTENT_SEQ=609,
        PROTECTED_VALID=673, PROTECTED_SEQ=674, ORIGIN_VALID=738,
        ORIGIN_SLOT=739, ORIGIN_GEN=771, DISCONT=803;
    localparam integer R_VALID=0, R_GEN=1, R_TICKS=33, R_OP=34,
        R_RETRIGGER=36, R_OUTCOME=37, R_REASON=40, R_CLEAR=44,
        R_TARGET=64, R_SEQUENCE=128, R_AT=192, R_SOURCE=256,
        R_SKIP=320, R_POSITION=384, R_EFFECTIVE=448, R_REMAINING=512,
        R_FRAME=576;
    localparam [1:0] STOPPED=0, PLAYING=1, RECORDING=2;
    localparam [1:0] OP_STOP=0, OP_PLAY=1, OP_RECORD=2, OP_SEEK=3;
    localparam [2:0] APPLIED=1, SUPERSEDED=2, CANCELLED=3, FAILED=4;
    localparam integer SPEC_SOURCE_VALID=33, SPEC_SOURCE=34, SPEC_REPEAT=98,
        SPEC_KIND=99, SPEC_TIMELINE=101, SPEC_LENGTH=128;
    localparam integer PA=(REQUEST_DEPTH > 1) ? $clog2(REQUEST_DEPTH) : 1;
    localparam integer H=PA+1, RW=640+2*H;
    localparam [H-1:0] NO_REQUEST={H{1'b1}};
    localparam integer HEAD_AT=37, HEAD_SYNC=804, R_NEXT=640, R_PREV=640+H;

    typedef enum reg [7:0] {
        S_IDLE, S_DISPATCH,
        S_L_WAIT, S_L_GET, S_R_WAIT, S_R_GET, S_S_WAIT, S_S_GET,
        S_M_WAIT, S_M_GET, S_E_WAIT, S_E_GET,
        S_PREP_LOOP, S_PREP_SPEC, S_PREP_SOURCE, S_PREP_WRITE,
        S_PREP_ORDER, S_PREP_ORDER_CHECK, S_PREP_ORDER_SOURCE,
        S_PREP_REQUEST, S_PREP_COMPLETE,
        S_INSTALL_TARGET, S_INSTALL_SOURCE, S_INSTALL_CHECK,
        S_INSTALL_ROUND, S_INSTALL_ALIGN,
        S_CANCEL_COLLECT, S_CANCEL_REPLY,
        S_INSERT_HEAD, S_INSERT_FIND, S_INSERT_READ, S_INSERT_PLACE,
        S_INSERT_PREV, S_INSERT_NEXT, S_INSERT_NEXT_WRITE, S_INSERT_DONE,
        S_UNLINK_BEGIN, S_UNLINK_HEAD, S_UNLINK_PREV, S_UNLINK_NEXT, S_UNLINK_NEXT_WRITE, S_UNLINK_DONE,
        S_APPLY_UNLINK_DONE,
        S_CLEAR,
        S_DIRECT_SCAN, S_DIRECT_CHECK,
        S_OFFER_LOOP, S_OFFER_CHECK, S_OFFER_LOSER,
        S_ORDER, S_ORDER_SLOT, S_ORDER_APPLY, S_ORDER_NATURAL, S_ORDER_INHERIT,
        S_ORDER_PULSE, S_PULSE_EMIT, S_ORDER_NEXT,
        S_REACT, S_REACT_CHECK, S_REACT_OFFER_DONE, S_REACT_APPLY, S_REACT_NATURAL,
        S_APPLY, S_APPLY_PLAN, S_APPLY_CHECK, S_CANCEL_OLDER, S_CANCEL_OLDER_CHECK, S_CANCEL_OLDER_NEXT,
        S_NATURAL, S_NATURAL_SOURCE, S_NATURAL_WRAP,
        S_FINAL, S_FINISH_LOOP,
        S_READ_LOOP_REPLY, S_READ_REQUEST_REPLY, S_READ_EVENT_REPLY, S_SET_REPEAT
    } state_t;
    state_t state, read_return, offer_return, apply_return, natural_return, react_return, list_return;
    reg configured;
    reg [3:0] fault_error;
    reg [1:0] phase;
    reg [63:0] frame;
    reg [31:0] loop_slots, request_slots, event_limit, order_count, quantum;
    reg [31:0] event_count;
    reg [LOOP_DEPTH-1:0] spec_written, order_written, valid_loops, seen_order;
    reg [LOOP_DEPTH-1:0] delivered;
    reg pulse_ready, pulse_masked;
    reg [REQUEST_DEPTH-1:0] absolute, absolute_to_visit;
    wire absolute_valid;
    wire [PA-1:0] absolute_index;
    tp_priority #(.SLOTS(REQUEST_DEPTH), .INDEX_BITS(PA)) request_priority (
        absolute_to_visit, absolute_valid, absolute_index);
    reg [31:0] request_checks, follower_visits, pulse_deliveries;
    reg [7:0] opcode;
    reg [31:0] index;
    reg [639:0] payload;
    reg [831:0] loop_row;
    reg [RW-1:0] request_row;
    reg [191:0] spec_row;
    reg [SA-1:0] order_slot;
    reg [255:0] event_row;
    reg [1:0] source_mode;
    reg loop_source_read, loop_with_spec;
    reg [31:0] cursor, request_cursor;
    reg [31:0] order_cursor;
    reg [63:0] submitted_effective;
    reg [6:0] quantum_mask;
    reg [63:0] submit_frame;
    reg [64:0] submit_sum;
    reg [32:0] prepare_events;
    reg [H-1:0] list_previous, list_next, walk_next;
    reg [31:0] list_slot, list_target;
    reg list_ticks, stopping, cancel_sync;
    reg [31:0] target_slot, react_source, apply_slot;
    reg [31:0] offer_slot, offer_target, loser_slot;
    reg [63:0] offer_sequence, stop_sequence;
    reg [63:0] target_source;
    reg target_source_valid;
    reg [31:0] install_slot, install_generation;

    reg lr_en, lw_en, rr_en, rw_en, sr_en, sw_en, mr_en, mw_en, er_en, ew_en;
    reg [LA-1:0] lr_address, lw_address;
    reg [RA-1:0] rr_address, rw_address;
    reg [SA-1:0] sr_address, sw_address, mr_address, mw_address;
    reg [EA-1:0] er_address, ew_address;
    reg [831:0] lw_data;
    reg [RW-1:0] rw_data;
    reg [191:0] sw_data;
    reg [SA-1:0] mw_data;
    reg [255:0] ew_data;
    wire [831:0] lr_data;
    wire [RW-1:0] rr_data;
    wire [191:0] sr_data;
    wire [SA-1:0] mr_data;
    wire [255:0] er_data;
    reg loop_stream_valid;
    reg finish_valid, finish_plays, finish_records;
    reg [831:0] finish_row;
    reg [64:0] finish_position, finish_capture;
    reg [LA-1:0] finish_address;
    wire loop_stream = state == S_CLEAR || state == S_FINISH_LOOP;
    wire loop_stream_read = cursor < loop_slots && !reset;
    wire [LA-1:0] stream_loop_address = loop_address(cursor);
    wire loop_accept = loop_read_valid && loop_read_ready;
    wire loop_in_range = loop_read_slot < loop_slots;
    wire [LA-1:0] shared_address = LA'(loop_read_slot);
    tp_ram #(.WIDTH(832), .DEPTH(LOOP_DEPTH), .ADDR_BITS(LA)) loops (
        clk, loop_stream ? loop_stream_read : lr_en || (loop_accept && loop_in_range),
        loop_stream ? stream_loop_address : loop_accept ? shared_address : lr_address,
        lr_data, lw_en, lw_address, lw_data);
    tp_ram #(.WIDTH(RW), .DEPTH(REQUEST_DEPTH), .ADDR_BITS(RA)) requests (
        clk, rr_en, rr_address, rr_data, rw_en, rw_address, rw_data);
    tp_ram #(.WIDTH(192), .DEPTH(LOOP_DEPTH), .ADDR_BITS(SA)) specs (
        clk, sr_en, sr_address, sr_data, sw_en, sw_address, sw_data);
    tp_ram #(.WIDTH(SA), .DEPTH(LOOP_DEPTH), .ADDR_BITS(SA)) dependency_order (
        clk, mr_en, mr_address, mr_data, mw_en, mw_address, mw_data);
    tp_ram #(.WIDTH(256), .DEPTH(EVENT_DEPTH), .ADDR_BITS(EA)) events (
        clk, er_en, er_address, er_data, ew_en, ew_address, ew_data);

    assign busy = state != S_IDLE;
    assign command_ready = !reset && state == S_IDLE && !response_valid && !loop_response_valid;
    assign loop_read_ready = !reset && state == S_IDLE && phase == SETTLED
        && !response_valid && !loop_response_valid && !command_valid;
    assign loop_response_data = loop_response_error ? 164'b0 :
        {(lr_data[CAPTURE+:64] == 0), lr_data[POSITION+:64], lr_data[LENGTH+:64], lr_data[MODE+:2], lr_data[GENERATION+:32], lr_data[VALID]};
    assign configured_quantum = configured ? quantum : 0;
    always @(posedge clk) begin
        if (reset) begin
            loop_response_valid <= 0;
            loop_response_error <= 0;
        end else begin
            loop_response_valid <= loop_accept;
            if (loop_accept) loop_response_error <= !loop_in_range;
        end
    end

    function automatic [LA-1:0] loop_address(input [31:0] unused_slot);
        loop_address = LA'(unused_slot);
    endfunction
    function automatic [RA-1:0] request_address(input [31:0] unused_slot);
        request_address = RA'(unused_slot);
    endfunction
    function automatic [EA-1:0] event_address(input [31:0] unused_slot);
        event_address = EA'(unused_slot);
    endfunction
    function automatic plays(input [831:0] unused_row);
        plays = unused_row[MODE+:2] == PLAYING && unused_row[LENGTH+:64] != 0
            && unused_row[POSITION+:64] < unused_row[LENGTH+:64];
    endfunction
    function automatic at_end(input [831:0] unused_row);
        at_end = unused_row[MODE+:2] == PLAYING && unused_row[LENGTH+:64] != 0
            && unused_row[POSITION+:64] == unused_row[LENGTH+:64];
    endfunction
    function automatic [RW-1:0] terminal(input [RW-1:0] row, input [2:0] result,
                                       input [3:0] reason, input [63:0] now_frame);
        reg [RW-1:0] value;
        begin
            value = row;
            value[R_OUTCOME+:3] = result;
            value[R_REASON+:4] = reason;
            value[R_FRAME+:64] = now_frame;
            terminal = value;
        end
    endfunction
    function automatic [836:0] operation(input [831:0] before_row, input [639:0] req);
        reg [831:0] value;
        reg [3:0] error;
        reg pulse, preserve;
        begin
            value = before_row;
            error = OK;
            pulse = 0;
            preserve = before_row[MODE+:2] == PLAYING && !req[R_RETRIGGER];
            if (req[R_CLEAR]) begin
                if (before_row[MODE+:2] == RECORDING) error = INVALID_STATE;
                else begin
                    value[MODE+:2] = STOPPED; value[LENGTH+:64] = 0; value[POSITION+:64] = 0;
                    value[CONTENT_REV+:64] = req[R_POSITION+:64];
                    if (req[R_POSITION+:64]>before_row[WATERMARK+:64]) value[WATERMARK+:64] = req[R_POSITION+:64];
                end
            end else case (req[R_OP+:2])
                OP_STOP: begin
                    if (before_row[MODE+:2] == RECORDING) begin
                        value[LENGTH+:64] = before_row[CAPTURE+:64];
                        value[CONTENT_REV+:64] = before_row[CAPTURE_REV+:64];
                    end
                    value[MODE+:2] = STOPPED;
                    value[POSITION+:64] = 0;
                end
                OP_RECORD: begin
                    if (&before_row[WATERMARK+:64]) error = EXHAUSTED;
                    value[CAPTURE_REV+:64] = before_row[WATERMARK+:64] + 1'b1;
                    value[WATERMARK+:64] = value[CAPTURE_REV+:64];
                    value[MODE+:2] = RECORDING;
                    value[CAPTURE+:64] = 0;
                    pulse = 1;
                end
                OP_PLAY: begin
                    if (before_row[MODE+:2] == RECORDING) begin
                        value[LENGTH+:64] = before_row[CAPTURE+:64];
                        value[CONTENT_REV+:64] = before_row[CAPTURE_REV+:64];
                    end
                    if (value[LENGTH+:64] == 0) error = EMPTY_CONTENT;
                    else if (!preserve && req[R_POSITION+:64] >= value[LENGTH+:64]) error = INVALID_FRAME;
                    value[MODE+:2] = PLAYING;
                    if (!preserve) begin
                        value[POSITION+:64] = req[R_POSITION+:64];
                        value[CYCLES+:64] = 0;
                    end
                    pulse = !preserve;
                end
                OP_SEEK: begin
                    if (before_row[MODE+:2] != PLAYING) error = INVALID_STATE;
                    else if (req[R_POSITION+:64] >= before_row[LENGTH+:64]) error = INVALID_FRAME;
                    value[POSITION+:64] = req[R_POSITION+:64];
                    if (req[R_RETRIGGER]) value[CYCLES+:64] = 0;
                    pulse = req[R_RETRIGGER];
                end
            endcase
            operation = {error, pulse, value};
        end
    endfunction

    reg [836:0] op_result;
    reg [831:0] apply_loop;
    reg [RW-1:0] apply_request;
    reg apply_event, apply_stop, apply_pulse;
    reg planned_event, planned_stop, planned_pulse;
    always @* begin
        apply_loop = loop_row;
        apply_request = request_row;
        apply_event = 0;
        apply_stop = 0;
        apply_pulse = 0;
        if (loop_row[PROTECTED_VALID] && request_row[R_SEQUENCE+:64] <= loop_row[PROTECTED_SEQ+:64]) begin
            apply_request = terminal(request_row, SUPERSEDED, OK, frame);
        end else begin
            apply_loop[PROTECTED_VALID] = 1;
            apply_loop[PROTECTED_SEQ+:64] = request_row[R_SEQUENCE+:64];
            if (op_result[836:833] != OK) begin
                apply_request = terminal(request_row, FAILED,
                    op_result[836:833], frame);
            end else begin
                apply_loop[575:0] = op_result[575:0];
                apply_loop[ORIGIN_VALID] = 1;
                apply_loop[ORIGIN_SLOT+:32] = apply_slot;
                apply_loop[ORIGIN_GEN+:32] = request_row[R_GEN+:32];
                apply_loop[DISCONT] = loop_row[DISCONT] || (plays(op_result[831:0])
                    && (!plays(loop_row) || request_row[R_OP+:2] == OP_SEEK
                        || (request_row[R_OP+:2] == OP_PLAY && request_row[R_RETRIGGER])));
                apply_request = terminal(request_row, APPLIED, OK, frame);
                apply_event = loop_row[575:0] != op_result[575:0];
                apply_stop = request_row[R_OP+:2] == OP_STOP;
                apply_pulse = op_result[832];
            end
        end
    end

    task automatic reply(input [3:0] error, input [831:0] data);
        begin
            response_valid <= 1;
            response_error <= error;
            response_data <= data;
            state <= S_IDLE;
            if (error != OK && (phase == EXECUTING || (phase == SETTLED && opcode == FINISH))) begin
                phase <= FAULTED; fault_error <= error;
            end
        end
    endtask
    task automatic read_loop(input [31:0] slot, input source_only,
                             input with_spec, input state_t destination);
        begin
            lr_en <= 1;
            lr_address <= loop_address(slot);
            loop_source_read <= source_only;
            loop_with_spec <= with_spec;
            if (with_spec) begin sr_en <= 1; sr_address <= SA'(slot); end
            read_return <= destination;
            state <= S_L_WAIT;
        end
    endtask
    task automatic write_loop(input [31:0] slot, input [831:0] data);
        begin lw_en <= 1; lw_address <= loop_address(slot); lw_data <= data; end
    endtask
    task automatic read_request(input [31:0] slot, input state_t destination);
        begin
            rr_en <= 1; rr_address <= request_address(slot);
            read_return <= destination; state <= S_R_WAIT;
        end
    endtask
    task automatic write_request(input [31:0] slot, input [RW-1:0] data);
        begin rw_en <= 1; rw_address <= request_address(slot); rw_data <= data; end
    endtask
    task automatic read_spec(input [31:0] unused_slot, input state_t destination);
        begin sr_en <= 1; sr_address <= SA'(unused_slot); read_return <= destination; state <= S_S_WAIT; end
    endtask
    task automatic read_order(input [31:0] unused_slot, input state_t destination);
        begin mr_en <= 1; mr_address <= SA'(unused_slot); read_return <= destination; state <= S_M_WAIT; end
    endtask
    task automatic emit_event(input [1:0] kind, input [1:0] op, input retrigger,
                              input [63:0] position_value);
        begin
            ew_en <= 1;
            ew_address <= event_address(event_count);
            ew_data <= {61'b0, retrigger, op, position_value,
                        loop_row[GENERATION+:32], target_slot, frame};
            ew_data[255:254] <= kind;
            ew_data[195] <= kind==0 && request_row[R_CLEAR];
            event_count <= event_count + 1'b1;
        end
    endtask
    task automatic offer(input [31:0] slot, input state_t destination);
        begin
            offer_slot <= slot;
            offer_target <= request_row[R_TARGET+:32];
            offer_sequence <= request_row[R_SEQUENCE+:64];
            offer_return <= destination;
            read_loop(request_row[R_TARGET+:32], 0, 0, S_OFFER_CHECK);
        end
    endtask

    task automatic unlink(input [31:0] slot, input [RW-1:0] row,
                          input state_t destination);
        begin
            list_slot <= slot; list_target <= row[R_TARGET+:32];
            list_previous <= row[R_PREV+:H]; list_next <= row[R_NEXT+:H];
            list_ticks <= row[R_TICKS]; list_return <= destination;
            if (!row[R_TICKS]) begin
                absolute[PA'(slot)] <= 0;
                absolute_to_visit[PA'(slot)] <= 0;
            end
            state <= S_UNLINK_BEGIN;
        end
    endtask

    always @(posedge clk) begin : control
        reg [831:0] temporary_loop;
        reg [RW-1:0] temporary_request;
        lr_en <= 0; lw_en <= 0; rr_en <= 0; rw_en <= 0;
        sr_en <= 0; sw_en <= 0; mr_en <= 0; mw_en <= 0;
        er_en <= 0; ew_en <= 0;
        loop_stream_valid <= 0;
        finish_valid <= 0;
        if (response_valid && response_ready) response_valid <= 0;
        if (reset) begin
            state <= S_IDLE;
            configured <= 0; fault_error <= 0;
            phase <= IDLE_PHASE; frame <= 0;
            loop_slots <= 0; request_slots <= 0; event_limit <= 0; order_count <= 0; quantum <= 0;
            quantum_mask <= 0; submit_frame <= 0; submit_sum <= 0;
            prepare_events <= 0;
            op_result <= 0;
            planned_event <= 0; planned_stop <= 0; planned_pulse <= 0;
            finish_plays <= 0; finish_records <= 0; finish_row <= 0;
            finish_position <= 0; finish_capture <= 0; finish_address <= 0;
            event_count <= 0;
            spec_written <= 0; order_written <= 0; valid_loops <= 0; seen_order <= 0;
            pulse_ready <= 0; delivered <= 0; pulse_masked <= 0;
            absolute <= 0; absolute_to_visit <= 0;
            request_checks <= 0; follower_visits <= 0; pulse_deliveries <= 0;
            response_valid <= 0; response_error <= OK; response_data <= 0;
        end else begin
            case (state)
                S_IDLE: if (command_valid && command_ready) begin
                    opcode <= command_opcode; index <= command_index; payload <= command_data;
                    prepare_events <= ({1'b0,command_data[31:0]} << 1)+{1'b0,command_data[63:32]};
                    state <= S_DISPATCH;
                end
                S_DISPATCH: begin
                    if (opcode == STATUS) begin
                        temporary_loop = 0;
                        temporary_loop[0+:64] = frame;
                        temporary_loop[64+:4] = fault_error;
                        temporary_loop[128+:2] = phase;
                        temporary_loop[136] = configured;
                        temporary_loop[192+:32] = event_count;
                        temporary_loop[256+:32] = loop_slots;
                        temporary_loop[288+:32] = request_slots;
                        temporary_loop[320+:32] = event_limit;
                        temporary_loop[352+:32] = order_count;
                        temporary_loop[384+:32] = quantum;
                        temporary_loop[416+:32] = request_checks;
                        temporary_loop[448+:32] = follower_visits;
                        temporary_loop[480+:32] = pulse_deliveries;
                        reply(OK, temporary_loop);
                    end else if (opcode == WRITE_SPEC || opcode == WRITE_ORDER) begin
                        if (configured) reply(INVALID_STATE, 0);
                        else if (index >= LOOP_DEPTH) reply(CAPACITY, 0);
                        else if (opcode == WRITE_SPEC) begin
                            sw_en <= 1; sw_address <= SA'(index); sw_data <= payload[191:0];
                            spec_written[index[SA-1:0]] <= 1;
                            reply(OK, 0);
                        end else if (payload[31:0] >= LOOP_DEPTH) reply(INVALID_ID, 0);
                        else if (payload[63:32] != 0) reply(INVALID_CONFIG, 0);
                        else begin
                            mw_en <= 1; mw_address <= SA'(index); mw_data <= SA'(payload[31:0]);
                            order_written[index[SA-1:0]] <= 1;
                            reply(OK, 0);
                        end
                    end else if (opcode == PREPARE) begin
                        if (configured) reply(INVALID_STATE, 0);
                        else if (payload[31:0] == 0 || payload[31:0] > LOOP_DEPTH
                            || payload[63:32] == 0 || payload[63:32] > REQUEST_DEPTH
                            || payload[95:64] > EVENT_DEPTH
                            || {1'b0,payload[95:64]} < prepare_events
                            || payload[127:96] > payload[31:0]
                            || !(payload[159:128] == 1 || payload[159:128] == 16
                                || payload[159:128] == 32 || payload[159:128] == 64 || payload[159:128] == 128))
                            reply(INVALID_CONFIG, 0);
                        else begin
                            loop_slots <= payload[31:0]; request_slots <= payload[63:32];
                            event_limit <= payload[95:64]; order_count <= payload[127:96]; quantum <= payload[159:128];
                            quantum_mask <= 7'(payload[159:128]-1'b1);
                            cursor <= 0; valid_loops <= 0; seen_order <= 0;
                            frame <= 0; fault_error <= 0;
                            event_count <= 0;
                            absolute <= 0;
                            state <= S_PREP_LOOP;
                        end
                    end else if (!configured) reply(INVALID_STATE, 0);
                    else case (opcode)
                        INSTALL: begin
                            if (phase != IDLE_PHASE) reply(INVALID_STATE, 0);
                            else if (index >= request_slots || payload[R_TARGET+:32] >= loop_slots) reply(INVALID_ID, 0);
                            else read_spec(payload[R_TARGET+:32], S_INSTALL_TARGET);
                        end
                        CANCEL, COLLECT: begin
                            if (phase != IDLE_PHASE) reply(INVALID_STATE, 0);
                            else if (index >= request_slots) reply(INVALID_ID, 0);
                            else read_request(index, S_CANCEL_COLLECT);
                        end
                        PROCESS: begin
                            if (phase == FAULTED) reply(fault_error, 0);
                            else if (phase != IDLE_PHASE) reply(INVALID_STATE, 0);
                            else begin
                                if (({1'b0,frame}+{33'b0,quantum}) > 65'h0ffffffffffffffff) begin
                                    phase <= FAULTED; fault_error <= EXHAUSTED; reply(EXHAUSTED, 0);
                                end
                                else begin
                                    phase <= EXECUTING; pulse_ready <= 0; delivered <= 0; pulse_masked <= 0;
                                    absolute_to_visit <= absolute; event_count <= 0;
                                    request_checks <= 0; follower_visits <= 0; pulse_deliveries <= 0;
                                    cursor <= 0; state <= S_CLEAR;
                                end
                            end
                        end
                        FINISH: begin
                            if (INTEGRATED == 0 || phase != SETTLED) reply(INVALID_STATE, 0);
                            else begin cursor <= 0; state <= S_FINISH_LOOP; end
                        end
                        SET_REPEAT: begin
                            if (phase != IDLE_PHASE) reply(INVALID_STATE,0);
                            else if (index>=loop_slots) reply(INVALID_ID,0);
                            else read_loop(index,0,0,S_SET_REPEAT);
                        end
                        READ_STATE: begin
                            if (index >= loop_slots) reply(INVALID_ID, 0);
                            else read_loop(index, 0, 0, S_READ_LOOP_REPLY);
                        end
                        READ_REQUEST: begin
                            if (index >= request_slots) reply(INVALID_ID, 0);
                            else read_request(index, S_READ_REQUEST_REPLY);
                        end
                        READ_EVENT: begin
                            if (index >= event_count) reply(INVALID_ID, 0);
                            else begin
                                er_en <= 1; er_address <= event_address(index);
                                read_return <= S_READ_EVENT_REPLY; state <= S_E_WAIT;
                            end
                        end
                        default: reply(UNSUPPORTED, 0);
                    endcase
                end
                S_L_WAIT: state <= S_L_GET;
                S_L_GET: begin
                    if (loop_source_read) source_mode <= lr_data[MODE+:2];
                    else loop_row <= lr_data;
                    if (loop_with_spec) spec_row <= sr_data;
                    state <= read_return;
                end
                S_R_WAIT: state <= S_R_GET;
                S_R_GET: begin
                    request_row <= rr_data;
                    op_result <= operation(loop_row, rr_data[639:0]);
                    state <= read_return;
                end
                S_S_WAIT: state <= S_S_GET;
                S_S_GET: begin spec_row <= sr_data; state <= read_return; end
                S_M_WAIT: state <= S_M_GET;
                S_M_GET: begin order_slot <= mr_data; state <= read_return; end
                S_E_WAIT: state <= S_E_GET;
                S_E_GET: begin event_row <= er_data; state <= read_return; end
                S_PREP_LOOP: begin
                    if (cursor == loop_slots) begin cursor <= 0; state <= S_PREP_ORDER; end
                    else if (!spec_written[cursor[SA-1:0]]) reply(INVALID_CONFIG, 0);
                    else read_spec(cursor, S_PREP_SPEC);
                end
                S_PREP_SPEC: begin
                    if (spec_row[SPEC_KIND+:2] != 0 || spec_row[SPEC_TIMELINE]) reply(UNSUPPORTED, 0);
                    else if (spec_row[VALID] && spec_row[GENERATION+:32] == 0) reply(INVALID_ID, 0);
                    else if (spec_row[VALID] && (spec_row[SPEC_LENGTH+:64] & {57'b0,quantum_mask}) != 0)
                        reply(INVALID_FRAME, 0);
                    else begin
                        temporary_loop = 0;
                        temporary_loop[HEAD_AT+:H] = NO_REQUEST;
                        temporary_loop[HEAD_SYNC+:H] = NO_REQUEST;
                        if (spec_row[VALID]) begin
                            temporary_loop[VALID] = 1;
                            temporary_loop[GENERATION+:32] = spec_row[GENERATION+:32];
                            temporary_loop[REPEAT] = spec_row[SPEC_REPEAT];
                            temporary_loop[LENGTH+:64] = spec_row[SPEC_LENGTH+:64];
                            valid_loops[cursor[SA-1:0]] <= 1;
                        end
                        write_loop(cursor, temporary_loop);
                        if (spec_row[VALID] && spec_row[SPEC_SOURCE_VALID]) begin
                            target_source <= spec_row[SPEC_SOURCE+:64];
                            if (spec_row[SPEC_SOURCE+:32] >= loop_slots
                                || !spec_written[spec_row[SPEC_SOURCE+:SA]]) reply(INVALID_ID, 0);
                            else read_spec(spec_row[SPEC_SOURCE+:32], S_PREP_SOURCE);
                        end else state <= S_PREP_WRITE;
                    end
                end
                S_PREP_SOURCE: begin
                    if (!spec_row[VALID] || spec_row[GENERATION+:32] != target_source[63:32]) reply(INVALID_ID, 0);
                    else state <= S_PREP_WRITE;
                end
                S_PREP_WRITE: begin cursor <= cursor+1'b1; state <= S_PREP_LOOP; end
                S_PREP_ORDER: begin
                    if (cursor == order_count) begin
                        if (seen_order != valid_loops) reply(INVALID_CONFIG, 0);
                        else begin cursor <= 0; state <= S_PREP_REQUEST; end
                    end else if (!order_written[cursor[SA-1:0]]) reply(INVALID_CONFIG, 0);
                    else read_order(cursor, S_PREP_ORDER_CHECK);
                end
                S_PREP_ORDER_CHECK: begin
                    if (32'(order_slot) >= loop_slots) reply(INVALID_ID, 0);
                    else if (seen_order[order_slot]) reply(DUPLICATE_ID, 0);
                    else read_spec(32'(order_slot), S_PREP_ORDER_SOURCE);
                end
                S_PREP_ORDER_SOURCE: begin
                    if (!spec_row[VALID]) reply(INVALID_ID, 0);
                    else if (spec_row[SPEC_SOURCE_VALID] && spec_row[SPEC_SOURCE+:SA] != order_slot
                        && !seen_order[spec_row[SPEC_SOURCE+:SA]]) reply(INVALID_CONFIG, 0);
                    else begin
                        seen_order[order_slot] <= 1;
                        cursor <= cursor+1'b1; state <= S_PREP_ORDER;
                    end
                end
                S_PREP_REQUEST: begin
                    temporary_request = 0; temporary_request[R_GEN+:32] = 1;
                    temporary_request[R_NEXT+:H] = NO_REQUEST; temporary_request[R_PREV+:H] = NO_REQUEST;
                    write_request(cursor, temporary_request);
                    if (cursor+1'b1 == request_slots) state <= S_PREP_COMPLETE;
                    else cursor <= cursor+1'b1;
                end
                S_PREP_COMPLETE: begin configured <= 1; phase <= IDLE_PHASE; reply(OK, 0); end
                S_SET_REPEAT: begin
                    if (!loop_row[VALID] || loop_row[GENERATION+:32]!=payload[31:0]) reply(INVALID_ID,0);
                    else begin temporary_loop=loop_row;temporary_loop[REPEAT]=payload[32];write_loop(index,temporary_loop);reply(OK,0);end
                end
                S_INSTALL_TARGET: begin
                    if (!spec_row[VALID] || spec_row[GENERATION+:32] != payload[R_TARGET+32+:32]) reply(INVALID_ID, 0);
                    else if (!payload[R_CLEAR] && (payload[R_OP+:2] == OP_PLAY || payload[R_OP+:2] == OP_SEEK)
                        && (payload[R_POSITION+:64] & {57'b0,quantum_mask}) != 0) reply(INVALID_FRAME, 0);
                    else begin
                        target_source <= spec_row[SPEC_SOURCE+:64]; target_source_valid <= spec_row[SPEC_SOURCE_VALID];
                        if (payload[R_TICKS]) begin
                            if (payload[R_SOURCE+:32] >= loop_slots) reply(INVALID_ID, 0);
                            else read_spec(payload[R_SOURCE+:32], S_INSTALL_SOURCE);
                        end else read_request(index, S_INSTALL_CHECK);
                    end
                end
                S_INSTALL_SOURCE: begin
                    if (!spec_row[VALID] || spec_row[GENERATION+:32] != payload[R_SOURCE+32+:32]) reply(INVALID_ID, 0);
                    else if (!target_source_valid || target_source != payload[R_SOURCE+:64]) reply(MISSING_SYNC, 0);
                    else read_request(index, S_INSTALL_CHECK);
                end
                S_INSTALL_CHECK: begin
                    if (payload[R_GEN+:32] == 0 || request_row[R_GEN+:32] != payload[R_GEN+:32]) reply(INVALID_ID, 0);
                    else if (request_row[R_VALID]) reply(INVALID_STATE, 0);
                    else begin
                        install_slot <= index; install_generation <= payload[R_GEN+:32];
                        submit_frame <= payload[R_AT+:64] > frame ? payload[R_AT+:64] : frame;
                        state <= S_INSTALL_ROUND;
                    end
                end
                S_INSTALL_ROUND: begin
                    submit_sum <= {1'b0,submit_frame}+{58'b0,quantum_mask};
                    state <= S_INSTALL_ALIGN;
                end
                S_INSTALL_ALIGN: begin
                    if (submit_sum[64]) reply(INVALID_FRAME, 0);
                    else begin
                        submitted_effective <= submit_sum[63:0] & ~{57'b0,quantum_mask};
                        list_slot <= install_slot;
                        list_target <= payload[R_TARGET+:32]; list_ticks <= payload[R_TICKS];
                        read_loop(payload[R_TARGET+:32], 0, 0, S_INSERT_HEAD);
                    end
                end
                S_CANCEL_COLLECT: begin
                    if (!request_row[R_VALID] || request_row[R_GEN+:32] != payload[31:0]) reply(INVALID_ID, 0);
                    else if (opcode == CANCEL) begin
                        if (request_row[R_OUTCOME+:3] != 0) reply(OK, 0);
                        else begin
                            write_request(index, terminal(request_row, CANCELLED, OK, frame));
                            unlink(index, request_row, S_CANCEL_REPLY);
                        end
                    end else if (request_row[R_OUTCOME+:3] == 0) reply(OK, 0);
                    else begin
                        temporary_request = 0;
                        temporary_request[R_GEN+:32] = request_row[R_GEN+:32]+1'b1;
                        write_request(index, temporary_request);
                        reply(OK, {192'b0,request_row[639:0]});
                    end
                end
                S_CANCEL_REPLY: reply(OK, 832'd1);
                S_INSERT_HEAD: begin
                    list_previous <= NO_REQUEST;
                    list_next <= list_ticks ? loop_row[HEAD_SYNC+:H] : loop_row[HEAD_AT+:H];
                    state <= S_INSERT_FIND;
                end
                S_INSERT_FIND: begin
                    if (list_next == NO_REQUEST || 32'(list_next) > list_slot) state <= S_INSERT_PLACE;
                    else read_request(32'(list_next), S_INSERT_READ);
                end
                S_INSERT_READ: begin
                    list_previous <= list_next; list_next <= request_row[R_NEXT+:H]; state <= S_INSERT_FIND;
                end
                S_INSERT_PLACE: begin
                        temporary_request = {{(RW-640){1'b0}},payload};
                        temporary_request[63:0] = 0;
                        temporary_request[R_VALID] = 1;
                        temporary_request[R_GEN+:32] = install_generation;
                        temporary_request[R_TICKS] = payload[R_TICKS];
                        temporary_request[R_OP+:2] = payload[R_OP+:2];
                        temporary_request[R_RETRIGGER] = payload[R_RETRIGGER];
                        temporary_request[R_CLEAR] = payload[R_CLEAR];
                        temporary_request[R_EFFECTIVE+:64] = submitted_effective;
                        temporary_request[R_REMAINING+:64] = payload[R_TICKS] ? payload[R_SKIP+:64] : 0;
                        temporary_request[R_FRAME+:64] = 0;
                        temporary_request[R_NEXT+:H] = NO_REQUEST; temporary_request[R_PREV+:H] = NO_REQUEST;
                    temporary_request[R_PREV+:H] = list_previous; temporary_request[R_NEXT+:H] = list_next;
                    write_request(list_slot, temporary_request);
                    if (list_previous == NO_REQUEST) begin
                        temporary_loop = loop_row;
                        if (list_ticks) temporary_loop[HEAD_SYNC+:H] = H'(list_slot);
                        else temporary_loop[HEAD_AT+:H] = H'(list_slot);
                        write_loop(list_target, temporary_loop);
                        state <= S_INSERT_NEXT;
                    end else read_request(32'(list_previous), S_INSERT_PREV);
                end
                S_INSERT_PREV: begin
                    temporary_request = request_row; temporary_request[R_NEXT+:H] = H'(list_slot);
                    write_request(32'(list_previous), temporary_request); state <= S_INSERT_NEXT;
                end
                S_INSERT_NEXT: begin
                    if (list_next == NO_REQUEST) state <= S_INSERT_DONE;
                    else read_request(32'(list_next), S_INSERT_NEXT_WRITE);
                end
                S_INSERT_NEXT_WRITE: begin
                    temporary_request = request_row; temporary_request[R_PREV+:H] = H'(list_slot);
                    write_request(32'(list_next), temporary_request); state <= S_INSERT_DONE;
                end
                S_INSERT_DONE: begin
                    if (!list_ticks) begin
                        absolute[PA'(list_slot)] <= 1;
                    end
                    reply(OK, {768'b0, install_generation, install_slot});
                end
                S_UNLINK_BEGIN: begin
                    if (list_previous == NO_REQUEST) read_loop(list_target, 0, 0, S_UNLINK_HEAD);
                    else read_request(32'(list_previous), S_UNLINK_PREV);
                end
                S_UNLINK_HEAD: begin
                    temporary_loop = loop_row;
                    if (list_ticks) temporary_loop[HEAD_SYNC+:H] = list_next;
                    else temporary_loop[HEAD_AT+:H] = list_next;
                    loop_row <= temporary_loop; write_loop(list_target, temporary_loop);
                    state <= S_UNLINK_NEXT;
                end
                S_UNLINK_PREV: begin
                    temporary_request = request_row; temporary_request[R_NEXT+:H] = list_next;
                    write_request(32'(list_previous), temporary_request); state <= S_UNLINK_NEXT;
                end
                S_UNLINK_NEXT: begin
                    if (list_next == NO_REQUEST) state <= S_UNLINK_DONE;
                    else read_request(32'(list_next), S_UNLINK_NEXT_WRITE);
                end
                S_UNLINK_NEXT_WRITE: begin
                    temporary_request = request_row; temporary_request[R_PREV+:H] = list_previous;
                    write_request(32'(list_next), temporary_request); state <= S_UNLINK_DONE;
                end
                S_UNLINK_DONE: state <= list_return;
                S_CLEAR: begin
                    loop_stream_valid <= loop_stream_read;
                    if (loop_stream_read) cursor <= cursor+1'b1;
                    if (loop_stream_valid) begin
                        temporary_loop = lr_data; temporary_loop[576+:228] = 0;
                        write_loop(cursor-1'b1, temporary_loop);
                    end
                    if (!loop_stream_read && !loop_stream_valid) state <= S_DIRECT_SCAN;
                end
                S_DIRECT_SCAN: begin
                    if (!absolute_valid) begin order_cursor <= 0; state <= S_ORDER; end
                    else begin
                        request_cursor <= 32'(absolute_index); absolute_to_visit[absolute_index] <= 0;
                        read_request(32'(absolute_index), S_DIRECT_CHECK);
                    end
                end
                S_DIRECT_CHECK: begin
                    request_checks <= request_checks+1'b1;
                    if (request_row[R_VALID] && request_row[R_OUTCOME+:3] == 0
                        && request_row[R_EFFECTIVE+:64] <= frame && !request_row[R_TICKS])
                        offer(request_cursor, S_OFFER_LOOP);
                    else state <= S_DIRECT_SCAN;
                end
                S_OFFER_LOOP: state <= S_DIRECT_SCAN;
                S_OFFER_CHECK: begin
                    temporary_loop = loop_row;
                    if (!loop_row[INTENT_VALID]) begin
                        temporary_loop[INTENT_VALID] = 1;
                        temporary_loop[INTENT_SLOT+:32] = offer_slot;
                        temporary_loop[INTENT_SEQ+:64] = offer_sequence;
                        write_loop(offer_target, temporary_loop);
                        loop_row <= temporary_loop; state <= offer_return;
                    end else begin
                        if (offer_sequence > loop_row[INTENT_SEQ+:64]) begin
                            loser_slot <= loop_row[INTENT_SLOT+:32];
                            temporary_loop[INTENT_SLOT+:32] = offer_slot;
                            temporary_loop[INTENT_SEQ+:64] = offer_sequence;
                            write_loop(offer_target, temporary_loop);
                            loop_row <= temporary_loop;
                            read_request(loop_row[INTENT_SLOT+:32], S_OFFER_LOSER);
                        end else begin
                            loser_slot <= offer_slot;
                            read_request(offer_slot, S_OFFER_LOSER);
                        end
                    end
                end
                S_OFFER_LOSER: begin
                    write_request(loser_slot, terminal(request_row, SUPERSEDED, OK, frame));
                    unlink(loser_slot, request_row, offer_return);
                end
                S_ORDER: begin
                    if (order_cursor == order_count) state <= S_FINAL;
                    else read_order(order_cursor, S_ORDER_SLOT);
                end
                S_ORDER_SLOT: begin
                    target_slot <= 32'(order_slot);
                    pulse_ready <= 0; pulse_masked <= 0;
                    read_loop(32'(order_slot), 0, 1, S_ORDER_APPLY);
                end
                S_ORDER_APPLY: begin
                    if (!loop_row[INTENT_VALID]) state <= S_ORDER_NATURAL;
                    else begin apply_return <= S_ORDER_NATURAL; state <= S_APPLY; end
                end
                S_ORDER_NATURAL: begin natural_return <= S_ORDER_INHERIT; state <= S_NATURAL; end
                S_ORDER_INHERIT: begin
                    if (spec_row[SPEC_SOURCE_VALID] && spec_row[SPEC_SOURCE+:32] != target_slot
                        && delivered[spec_row[SPEC_SOURCE+:SA]]) begin
                        if (!pulse_masked) pulse_ready <= 1;
                        follower_visits <= follower_visits+1'b1;
                        react_source <= spec_row[SPEC_SOURCE+:32];
                        if (loop_row[HEAD_SYNC+:H] == NO_REQUEST) state <= S_ORDER_PULSE;
                        else begin
                            request_cursor <= 32'(loop_row[HEAD_SYNC+:H]);
                            react_return <= S_ORDER_PULSE; state <= S_REACT;
                        end
                    end else state <= S_ORDER_PULSE;
                end
                S_ORDER_PULSE: begin
                    if (!pulse_ready) state <= S_ORDER_NEXT;
                    else if (event_count == event_limit) reply(CAPACITY, 0);
                    else begin
                        delivered[target_slot[SA-1:0]] <= 1;
                        pulse_deliveries <= pulse_deliveries+1'b1;
                        temporary_loop = loop_row;
                        temporary_loop[LAST_VALID] = 1; temporary_loop[LAST_PULSE+:64] = frame;
                        loop_row <= temporary_loop; write_loop(target_slot, temporary_loop);
                        emit_event(2, OP_STOP, 0, 0);
                        state <= S_PULSE_EMIT;
                    end
                end
                S_PULSE_EMIT: begin
                    if (spec_row[SPEC_SOURCE_VALID] && spec_row[SPEC_SOURCE+:32] == target_slot) begin
                        follower_visits <= follower_visits+1'b1;
                        react_source <= target_slot;
                        if (loop_row[HEAD_SYNC+:H] == NO_REQUEST) state <= S_ORDER_NEXT;
                        else begin
                            request_cursor <= 32'(loop_row[HEAD_SYNC+:H]);
                            react_return <= S_ORDER_NEXT; state <= S_REACT;
                        end
                    end else state <= S_ORDER_NEXT;
                end
                S_ORDER_NEXT: begin order_cursor <= order_cursor+1'b1; state <= S_ORDER; end
                S_REACT: begin
                    if (request_cursor == 32'(NO_REQUEST)) state <= S_REACT_APPLY;
                    else read_request(request_cursor, S_REACT_CHECK);
                end
                S_REACT_CHECK: begin
                    request_checks <= request_checks+1'b1;
                    walk_next <= request_row[R_NEXT+:H];
                    if (request_row[R_VALID] && request_row[R_OUTCOME+:3] == 0
                        && request_row[R_EFFECTIVE+:64] <= frame
                        && request_row[R_TARGET+:32] == target_slot
                        && request_row[R_TICKS]
                        && request_row[R_SOURCE+:32] == react_source) begin
                        if (request_row[R_REMAINING+:64] != 0) begin
                            temporary_request = request_row;
                            temporary_request[R_REMAINING+:64] = request_row[R_REMAINING+:64]-1'b1;
                            write_request(request_cursor, temporary_request);
                            request_cursor <= 32'(request_row[R_NEXT+:H]); state <= S_REACT;
                        end else offer(request_cursor, S_REACT_OFFER_DONE);
                    end else begin request_cursor <= 32'(request_row[R_NEXT+:H]); state <= S_REACT; end
                end
                S_REACT_OFFER_DONE: begin request_cursor <= 32'(walk_next); state <= S_REACT; end
                S_REACT_APPLY: begin apply_return <= S_REACT_NATURAL; state <= S_APPLY; end
                S_REACT_NATURAL: begin natural_return <= react_return; state <= S_NATURAL; end
                S_APPLY: begin
                    if (!loop_row[INTENT_VALID]) state <= apply_return;
                    else begin
                        apply_slot <= loop_row[INTENT_SLOT+:32]; loop_row[INTENT_VALID] <= 0;
                        read_request(loop_row[INTENT_SLOT+:32], S_APPLY_PLAN);
                    end
                end
                S_APPLY_PLAN: begin
                    planned_event <= apply_event; planned_stop <= apply_stop; planned_pulse <= apply_pulse;
                    state <= S_APPLY_CHECK;
                end
                S_APPLY_CHECK: begin
                    if (planned_event && event_count == event_limit) reply(CAPACITY, 0);
                    else begin
                        loop_row <= apply_loop;
                        write_loop(target_slot, apply_loop);
                        write_request(apply_slot, apply_request);
                        if (planned_event) emit_event(0, request_row[R_OP+:2], request_row[R_RETRIGGER], request_row[R_POSITION+:64]);
                        stopping <= planned_stop;
                        if (planned_stop) begin
                            pulse_masked <= 1; pulse_ready <= 0;
                            stop_sequence <= request_row[R_SEQUENCE+:64];
                        end else if (planned_pulse) begin
                            pulse_masked <= 0;
                            if (!delivered[target_slot[SA-1:0]]) pulse_ready <= 1;
                        end
                        unlink(apply_slot, request_row, S_APPLY_UNLINK_DONE);
                    end
                end
                S_APPLY_UNLINK_DONE: begin
                    if (!stopping) state <= apply_return;
                    else begin
                        cancel_sync <= 0; request_cursor <= 32'(loop_row[HEAD_AT+:H]); state <= S_CANCEL_OLDER;
                    end
                end
                S_CANCEL_OLDER: begin
                    if (request_cursor == 32'(NO_REQUEST)) begin
                        if (cancel_sync) state <= apply_return;
                        else begin cancel_sync <= 1; request_cursor <= 32'(loop_row[HEAD_SYNC+:H]); end
                    end else read_request(request_cursor, S_CANCEL_OLDER_CHECK);
                end
                S_CANCEL_OLDER_CHECK: begin
                    request_checks <= request_checks+1'b1;
                    walk_next <= request_row[R_NEXT+:H];
                    if (request_row[R_SEQUENCE+:64] < stop_sequence) begin
                        write_request(request_cursor, terminal(request_row, CANCELLED, OK, frame));
                        unlink(request_cursor, request_row, S_CANCEL_OLDER_NEXT);
                    end else state <= S_CANCEL_OLDER_NEXT;
                end
                S_CANCEL_OLDER_NEXT: begin request_cursor <= 32'(walk_next); state <= S_CANCEL_OLDER; end
                S_NATURAL: begin
                    if (!at_end(loop_row)) state <= natural_return;
                    else if (loop_row[REPEAT] && spec_row[SPEC_SOURCE_VALID] && spec_row[SPEC_SOURCE+:32] != target_slot)
                        read_loop(spec_row[SPEC_SOURCE+:32], 1, 0, S_NATURAL_SOURCE);
                    else state <= S_NATURAL_WRAP;
                end
                S_NATURAL_SOURCE: begin
                    if (source_mode == PLAYING && !delivered[spec_row[SPEC_SOURCE+:SA]]) state <= natural_return;
                    else state <= S_NATURAL_WRAP;
                end
                S_NATURAL_WRAP: begin
                    if (&loop_row[CYCLES+:64]) reply(EXHAUSTED, 0);
                    else if (event_count == event_limit) reply(CAPACITY, 0);
                    else begin
                        temporary_loop = loop_row;
                        temporary_loop[POSITION+:64] = 0; temporary_loop[CYCLES+:64] = loop_row[CYCLES+:64]+1'b1;
                        temporary_loop[DISCONT] = 1;
                        if (!delivered[target_slot[SA-1:0]]) pulse_ready <= 1;
                        loop_row <= temporary_loop; write_loop(target_slot, temporary_loop);
                        emit_event(1, OP_STOP, 0, 0); state <= natural_return;
                    end
                end
                S_FINAL: begin
                    if (INTEGRATED != 0) begin phase <= SETTLED; reply(OK, 0); end
                    else begin cursor <= 0; state <= S_FINISH_LOOP; end
                end
                S_FINISH_LOOP: begin
                    loop_stream_valid <= loop_stream_read;
                    finish_valid <= loop_stream_valid;
                    if (loop_stream_read) cursor <= cursor+1'b1;
                    if (loop_stream_valid) begin
                        finish_row <= lr_data;
                        finish_address <= loop_address(cursor-1'b1);
                        finish_position <= {1'b0,lr_data[POSITION+:64]}+{33'b0,quantum};
                        finish_capture <= {1'b0,lr_data[CAPTURE+:64]}+{33'b0,quantum};
                        finish_plays <= lr_data[VALID] && plays(lr_data);
                        finish_records <= lr_data[VALID] && lr_data[MODE+:2] == RECORDING;
                    end
                    if (finish_valid) begin
                        temporary_loop = finish_row;
                        if (finish_plays && (finish_position[64] || finish_position[63:0] > finish_row[LENGTH+:64]))
                            reply(INVALID_FRAME, 0);
                        else begin
                            if (finish_plays) temporary_loop[POSITION+:64] = finish_position[63:0];
                            if (finish_records && finish_capture[64]) reply(EXHAUSTED, 0);
                            else begin
                                if (finish_records) temporary_loop[CAPTURE+:64] = finish_capture[63:0];
                                write_loop(32'(finish_address), temporary_loop);
                            end
                        end
                    end else if (!loop_stream_valid && !loop_stream_read) begin
                        frame <= frame+64'(quantum); phase <= IDLE_PHASE; reply(OK, 0);
                    end
                end
                S_READ_LOOP_REPLY: reply(OK, loop_row);
                S_READ_REQUEST_REPLY: reply(OK, {192'b0,request_row[639:0]});
                S_READ_EVENT_REPLY: reply(OK, {576'b0,event_row});
                default: reply(INVALID_STATE, 0);
            endcase
        end
    end

    wire unused_reserved_bits = &{1'b0, payload, order_slot, spec_row, loop_row, request_row};
endmodule
