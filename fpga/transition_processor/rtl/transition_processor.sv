module transition_processor #(
    parameter integer LOOP_DEPTH = 64,
    parameter integer REQUEST_DEPTH = 64,
    parameter integer EVENT_DEPTH = 192,
    parameter integer LA = $clog2(2 * LOOP_DEPTH),
    parameter integer RA = $clog2(2 * REQUEST_DEPTH),
    parameter integer EA = $clog2(2 * EVENT_DEPTH),
    parameter integer SA = (LOOP_DEPTH > 1) ? $clog2(LOOP_DEPTH) : 1,
    parameter integer OA = $clog2(LOOP_DEPTH + REQUEST_DEPTH + 1)
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
    output wire busy
);
    localparam [3:0] OK=0, INVALID_CONFIG=1, CAPACITY=2, INVALID_ID=3,
        DUPLICATE_ID=4, INVALID_STATE=5, INVALID_FRAME=6, MISSING_SYNC=7,
        EXHAUSTED=8, EMPTY_CONTENT=9, UNSUPPORTED=10;
    localparam [1:0] IDLE_PHASE=0, PREPARING=1, PREVIEW=2, COMMITTED=3;
    localparam [7:0] STATUS=0, WRITE_SPEC=1, WRITE_MEMBER=2, PREPARE=3,
        SUBMIT=4, CANCEL=5, COLLECT=6, OBS_CLEAR=7, OBS_ADD=8,
        MAKE_PREVIEW=9, COMMIT=10, FINISH=11, READ_STATE=12,
        READ_REQUEST=13, READ_DECISION=14, READ_EVENT=15;
    localparam integer VALID=0, GENERATION=1, MODE=33, REPEAT=35, LAST_VALID=36,
        LENGTH=64, POSITION=128, CAPTURE=192, CYCLES=256,
        CONTENT_REV=320, CAPTURE_REV=384, WATERMARK=448, LAST_PULSE=512,
        INTENT_VALID=576, INTENT_SLOT=577, INTENT_SEQ=609,
        PROTECTED_VALID=673, PROTECTED_SEQ=674, ORIGIN_VALID=738,
        ORIGIN_SLOT=739, ORIGIN_GEN=771, DISCONT=803;
    localparam integer R_VALID=0, R_GEN=1, R_TICKS=33, R_OP=34,
        R_RETRIGGER=36, R_OUTCOME=37, R_REASON=40,
        R_TARGET=64, R_SEQUENCE=128, R_AT=192, R_SOURCE=256,
        R_SKIP=320, R_POSITION=384, R_EFFECTIVE=448, R_REMAINING=512,
        R_FRAME=576;
    localparam [1:0] STOPPED=0, PLAYING=1, RECORDING=2;
    localparam [1:0] OP_STOP=0, OP_PLAY=1, OP_RECORD=2, OP_SEEK=3;
    localparam [2:0] APPLIED=1, SUPERSEDED=2, CANCELLED=3, FAILED=4;
    localparam integer SPEC_SOURCE_VALID=33, SPEC_SOURCE=34, SPEC_REPEAT=98,
        SPEC_KIND=99, SPEC_TIMELINE=101, SPEC_LENGTH=128;
    localparam integer PA=(REQUEST_DEPTH > 1) ? $clog2(REQUEST_DEPTH) : 1;
    localparam integer OBS_DEPTH=LOOP_DEPTH+REQUEST_DEPTH+1;

    typedef enum reg [7:0] {
        S_IDLE, S_DISPATCH,
        S_L_WAIT, S_L_GET, S_R_WAIT, S_R_GET, S_S_WAIT, S_S_GET,
        S_M_WAIT, S_M_GET, S_O_WAIT, S_O_GET, S_E_WAIT, S_E_GET,
        S_PREP_LOOP, S_PREP_SPEC, S_PREP_SOURCE, S_PREP_WRITE,
        S_PREP_MEMBER, S_PREP_MEMBER_CHECK, S_PREP_MEMBER_LOOP,
        S_PREP_REQUEST, S_PREP_COMPLETE,
        S_SUBMIT_TARGET, S_SUBMIT_SOURCE, S_SUBMIT_SCAN, S_SUBMIT_CHECK, S_SUBMIT_WRITE,
        S_CANCEL_COLLECT,
        S_PREVIEW_OBS, S_OBS_CHECK, S_OBS_TARGET, S_OBS_REQUEST,
        S_OBS_DUP, S_OBS_DUP_CHECK, S_OBS_NEXT,
        S_COPY_LOOP, S_COPY_LOOP_WRITE, S_COPY_REQUEST, S_COPY_REQUEST_WRITE,
        S_DIRECT_SCAN, S_DIRECT_CHECK,
        S_OFFER_LOOP, S_OFFER_CHECK, S_OFFER_LOSER,
        S_COMPONENT, S_COMPONENT_END, S_COMPONENT_END_CHECK,
        S_FIRST, S_FIRST_MEMBER, S_FIRST_APPLY, S_FIRST_NATURAL, S_FIRST_INHERIT,
        S_SECOND, S_SECOND_MEMBER, S_SECOND_REACT, S_SECOND_NEXT,
        S_FIND_PULSE, S_FIND_PULSE_CHECK, S_PULSE_LOAD, S_PULSE_EMIT,
        S_FOLLOW, S_FOLLOW_MEMBER, S_FOLLOW_REACT, S_FOLLOW_NEXT,
        S_REACT, S_REACT_CHECK, S_REACT_OFFER_DONE, S_REACT_APPLY, S_REACT_NATURAL,
        S_APPLY, S_APPLY_CHECK, S_CANCEL_OLDER, S_CANCEL_OLDER_CHECK,
        S_NATURAL, S_NATURAL_SOURCE, S_NATURAL_WRAP,
        S_FINAL, S_FINAL_CHECK, S_FINISH_LOOP, S_FINISH_WRITE,
        S_READ_LOOP_REPLY, S_READ_REQUEST_REPLY, S_READ_EVENT_REPLY
    } state_t;
    state_t state, read_return, offer_return, apply_return, natural_return, react_return;
    reg configured, active_bank, candidate_valid;
    reg [1:0] phase;
    reg [63:0] frame, serial;
    reg [31:0] loop_slots, request_slots, event_limit, member_count, quantum;
    reg [31:0] event_count_0, event_count_1;
    wire [31:0] active_events = active_bank ? event_count_1 : event_count_0;
    wire [31:0] scratch_events = active_bank ? event_count_0 : event_count_1;
    reg [LOOP_DEPTH-1:0] spec_written, member_written, valid_loops, seen_members;
    reg [LOOP_DEPTH-1:0] pulses, delivered, masked, capture_refused;
    reg [REQUEST_DEPTH-1:0] eligible, request_refused;
    reg [3:0] capture_reason [0:LOOP_DEPTH-1];
    reg [3:0] request_reason [0:REQUEST_DEPTH-1];
    reg [31:0] observation_count;
    reg observation_overflow;
    reg [7:0] opcode;
    reg [31:0] index;
    reg [639:0] payload;
    reg [831:0] loop_row;
    reg [639:0] request_row;
    reg [191:0] spec_row;
    reg [63:0] member_row;
    reg [127:0] observation_row;
    reg [255:0] event_row;
    reg [1:0] source_mode;
    reg loop_source_read, loop_with_spec;
    reg [31:0] cursor, observation_cursor, duplicate_cursor, request_cursor;
    reg [64:0] observation_key;
    reg [3:0] observation_reason;
    reg [31:0] component_start, component_end, member_cursor, pulse_cursor, follow_cursor;
    reg [31:0] target_slot, pulse_slot, react_source, apply_slot;
    reg [31:0] offer_slot, offer_target, loser_slot;
    reg [63:0] offer_sequence, stop_sequence;
    reg [63:0] target_source;
    reg target_source_valid;
    reg free_found;
    reg [31:0] free_slot, free_generation;

    reg lr_en, lw_en, rr_en, rw_en, sr_en, sw_en, mr_en, mw_en, or_en, ow_en, er_en, ew_en;
    reg [LA-1:0] lr_address, lw_address;
    reg [RA-1:0] rr_address, rw_address;
    reg [SA-1:0] sr_address, sw_address, mr_address, mw_address;
    reg [OA-1:0] or_address, ow_address;
    reg [EA-1:0] er_address, ew_address;
    reg [831:0] lw_data;
    reg [639:0] rw_data;
    reg [191:0] sw_data;
    reg [63:0] mw_data;
    reg [127:0] ow_data;
    reg [255:0] ew_data;
    wire [831:0] lr_data;
    wire [639:0] rr_data;
    wire [191:0] sr_data;
    wire [63:0] mr_data;
    wire [127:0] or_data;
    wire [255:0] er_data;
    tp_ram #(.WIDTH(832), .DEPTH(2*LOOP_DEPTH), .ADDR_BITS(LA)) loops (
        clk, lr_en, lr_address, lr_data, lw_en, lw_address, lw_data);
    tp_ram #(.WIDTH(640), .DEPTH(2*REQUEST_DEPTH), .ADDR_BITS(RA)) requests (
        clk, rr_en, rr_address, rr_data, rw_en, rw_address, rw_data);
    tp_ram #(.WIDTH(192), .DEPTH(LOOP_DEPTH), .ADDR_BITS(SA)) specs (
        clk, sr_en, sr_address, sr_data, sw_en, sw_address, sw_data);
    tp_ram #(.WIDTH(64), .DEPTH(LOOP_DEPTH), .ADDR_BITS(SA)) members (
        clk, mr_en, mr_address, mr_data, mw_en, mw_address, mw_data);
    tp_ram #(.WIDTH(128), .DEPTH(OBS_DEPTH), .ADDR_BITS(OA)) observations (
        clk, or_en, or_address, or_data, ow_en, ow_address, ow_data);
    tp_ram #(.WIDTH(256), .DEPTH(2*EVENT_DEPTH), .ADDR_BITS(EA)) events (
        clk, er_en, er_address, er_data, ew_en, ew_address, ew_data);

    assign busy = state != S_IDLE;
    assign command_ready = !reset && state == S_IDLE && !response_valid;

    function automatic [LA-1:0] loop_address(input [31:0] slot, input bank);
        loop_address = LA'(slot + (bank ? LOOP_DEPTH : 0));
    endfunction
    function automatic [RA-1:0] request_address(input [31:0] slot, input bank);
        request_address = RA'(slot + (bank ? REQUEST_DEPTH : 0));
    endfunction
    function automatic [EA-1:0] event_address(input [31:0] slot, input bank);
        event_address = EA'(slot + (bank ? EVENT_DEPTH : 0));
    endfunction
    function automatic plays(input [831:0] unused_row);
        plays = unused_row[MODE+:2] == PLAYING && unused_row[LENGTH+:64] != 0
            && unused_row[POSITION+:64] < unused_row[LENGTH+:64];
    endfunction
    function automatic at_end(input [831:0] unused_row);
        at_end = unused_row[MODE+:2] == PLAYING && unused_row[LENGTH+:64] != 0
            && unused_row[POSITION+:64] == unused_row[LENGTH+:64];
    endfunction
    function automatic [639:0] terminal(input [639:0] row, input [2:0] result,
                                       input [3:0] reason, input [63:0] now_frame);
        reg [639:0] value;
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
            case (req[R_OP+:2])
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

    wire [836:0] op_result = operation(loop_row, request_row);
    reg [831:0] apply_loop;
    reg [639:0] apply_request;
    reg apply_event, apply_stop, apply_pulse;
    reg [3:0] refusal;
    always @* begin
        apply_loop = loop_row;
        apply_request = request_row;
        apply_event = 0;
        apply_stop = 0;
        apply_pulse = 0;
        refusal = OK;
        if (request_refused[apply_slot[PA-1:0]])
            refusal = request_reason[apply_slot[PA-1:0]];
        else if (request_row[R_OP+:2] == OP_RECORD && capture_refused[target_slot[SA-1:0]])
            refusal = capture_reason[target_slot[SA-1:0]];
        if (loop_row[PROTECTED_VALID] && request_row[R_SEQUENCE+:64] <= loop_row[PROTECTED_SEQ+:64]) begin
            apply_request = terminal(request_row, SUPERSEDED, OK, frame);
        end else begin
            apply_loop[PROTECTED_VALID] = 1;
            apply_loop[PROTECTED_SEQ+:64] = request_row[R_SEQUENCE+:64];
            if (refusal != OK || op_result[836:833] != OK) begin
                apply_request = terminal(request_row, FAILED,
                    refusal != OK ? refusal : op_result[836:833], frame);
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
        end
    endtask
    task automatic read_loop(input [31:0] slot, input bank, input source_only,
                             input with_spec, input state_t destination);
        begin
            lr_en <= 1;
            lr_address <= loop_address(slot, bank);
            loop_source_read <= source_only;
            loop_with_spec <= with_spec;
            if (with_spec) begin sr_en <= 1; sr_address <= SA'(slot); end
            read_return <= destination;
            state <= S_L_WAIT;
        end
    endtask
    task automatic write_loop(input [31:0] slot, input bank, input [831:0] data);
        begin lw_en <= 1; lw_address <= loop_address(slot, bank); lw_data <= data; end
    endtask
    task automatic read_request(input [31:0] slot, input bank, input state_t destination);
        begin
            rr_en <= 1; rr_address <= request_address(slot, bank);
            read_return <= destination; state <= S_R_WAIT;
        end
    endtask
    task automatic write_request(input [31:0] slot, input bank, input [639:0] data);
        begin rw_en <= 1; rw_address <= request_address(slot, bank); rw_data <= data; end
    endtask
    task automatic read_spec(input [31:0] unused_slot, input state_t destination);
        begin sr_en <= 1; sr_address <= SA'(unused_slot); read_return <= destination; state <= S_S_WAIT; end
    endtask
    task automatic read_member(input [31:0] unused_slot, input state_t destination);
        begin mr_en <= 1; mr_address <= SA'(unused_slot); read_return <= destination; state <= S_M_WAIT; end
    endtask
    task automatic read_observation(input [31:0] unused_slot, input state_t destination);
        begin or_en <= 1; or_address <= OA'(unused_slot); read_return <= destination; state <= S_O_WAIT; end
    endtask
    task automatic emit_event(input [1:0] kind, input [1:0] op, input retrigger,
                              input [63:0] position_value);
        begin
            ew_en <= 1;
            ew_address <= event_address(scratch_events, !active_bank);
            ew_data <= {61'b0, retrigger, op, position_value,
                        loop_row[GENERATION+:32], target_slot, frame};
            ew_data[255:254] <= kind;
            if (active_bank) event_count_0 <= scratch_events + 1'b1;
            else event_count_1 <= scratch_events + 1'b1;
        end
    endtask
    task automatic offer(input [31:0] slot, input state_t destination);
        begin
            offer_slot <= slot;
            offer_target <= request_row[R_TARGET+:32];
            offer_sequence <= request_row[R_SEQUENCE+:64];
            offer_return <= destination;
            read_loop(request_row[R_TARGET+:32], !active_bank, 0, 0, S_OFFER_CHECK);
        end
    endtask

    always @(posedge clk) begin : control
        reg [831:0] temporary_loop;
        reg [639:0] temporary_request;
        reg [64:0] addition;
        reg [63:0] rounded_frame;
        lr_en <= 0; lw_en <= 0; rr_en <= 0; rw_en <= 0;
        sr_en <= 0; sw_en <= 0; mr_en <= 0; mw_en <= 0;
        or_en <= 0; ow_en <= 0; er_en <= 0; ew_en <= 0;
        if (response_valid && response_ready) response_valid <= 0;
        if (reset) begin
            state <= S_IDLE;
            configured <= 0; active_bank <= 0; candidate_valid <= 0;
            phase <= IDLE_PHASE; frame <= 0; serial <= 0;
            loop_slots <= 0; request_slots <= 0; event_limit <= 0; member_count <= 0; quantum <= 0;
            event_count_0 <= 0; event_count_1 <= 0;
            spec_written <= 0; member_written <= 0; valid_loops <= 0; seen_members <= 0;
            pulses <= 0; delivered <= 0; masked <= 0; eligible <= 0;
            capture_refused <= 0; request_refused <= 0;
            observation_count <= 0; observation_overflow <= 0;
            response_valid <= 0; response_error <= OK; response_data <= 0;
        end else begin
            case (state)
                S_IDLE: if (command_valid && command_ready) begin
                    opcode <= command_opcode; index <= command_index; payload <= command_data;
                    state <= S_DISPATCH;
                end
                S_DISPATCH: begin
                    if (opcode == STATUS) begin
                        temporary_loop = 0;
                        temporary_loop[0+:64] = frame;
                        temporary_loop[64+:64] = serial;
                        temporary_loop[128+:2] = phase;
                        temporary_loop[136] = configured;
                        temporary_loop[137] = active_bank;
                        temporary_loop[192+:32] = active_events;
                        temporary_loop[224+:32] = phase == COMMITTED ? active_events : scratch_events;
                        temporary_loop[256+:32] = loop_slots;
                        temporary_loop[288+:32] = request_slots;
                        temporary_loop[320+:32] = event_limit;
                        temporary_loop[352+:32] = member_count;
                        temporary_loop[384+:32] = quantum;
                        reply(OK, temporary_loop);
                    end else if (opcode == WRITE_SPEC || opcode == WRITE_MEMBER) begin
                        if (configured) reply(INVALID_STATE, 0);
                        else if (index >= LOOP_DEPTH) reply(CAPACITY, 0);
                        else if (opcode == WRITE_SPEC) begin
                            sw_en <= 1; sw_address <= SA'(index); sw_data <= payload[191:0];
                            spec_written[index[SA-1:0]] <= 1;
                            reply(OK, 0);
                        end else begin
                            mw_en <= 1; mw_address <= SA'(index); mw_data <= payload[63:0];
                            member_written[index[SA-1:0]] <= 1;
                            reply(OK, 0);
                        end
                    end else if (opcode == PREPARE) begin
                        if (configured) reply(INVALID_STATE, 0);
                        else if (payload[31:0] == 0 || payload[31:0] > LOOP_DEPTH
                            || payload[63:32] == 0 || payload[63:32] > REQUEST_DEPTH
                            || payload[95:64] > EVENT_DEPTH
                            || {1'b0,payload[95:64]} < 2*{1'b0,payload[31:0]}+{1'b0,payload[63:32]}
                            || payload[127:96] > payload[31:0]
                            || !(payload[159:128] == 1 || payload[159:128] == 16
                                || payload[159:128] == 32 || payload[159:128] == 64 || payload[159:128] == 128))
                            reply(INVALID_CONFIG, 0);
                        else begin
                            loop_slots <= payload[31:0]; request_slots <= payload[63:32];
                            event_limit <= payload[95:64]; member_count <= payload[127:96]; quantum <= payload[159:128];
                            cursor <= 0; valid_loops <= 0; seen_members <= 0;
                            active_bank <= 0; frame <= 0; serial <= 0;
                            event_count_0 <= 0; event_count_1 <= 0;
                            state <= S_PREP_LOOP;
                        end
                    end else if (!configured) reply(INVALID_STATE, 0);
                    else case (opcode)
                        SUBMIT: begin
                            if (phase != IDLE_PHASE) reply(INVALID_STATE, 0);
                            else if (payload[R_TARGET+:32] >= loop_slots) reply(INVALID_ID, 0);
                            else read_spec(payload[R_TARGET+:32], S_SUBMIT_TARGET);
                        end
                        CANCEL, COLLECT: begin
                            if (phase != IDLE_PHASE) reply(INVALID_STATE, 0);
                            else if (index >= request_slots) reply(INVALID_ID, 0);
                            else read_request(index, active_bank, S_CANCEL_COLLECT);
                        end
                        OBS_CLEAR: begin
                            if (phase == COMMITTED) reply(INVALID_STATE, 0);
                            else begin observation_count <= 0; observation_overflow <= 0; reply(OK, 0); end
                        end
                        OBS_ADD: begin
                            if (phase == COMMITTED) reply(INVALID_STATE, 0);
                            else begin
                                if (observation_count < OBS_DEPTH) begin
                                    ow_en <= 1; ow_address <= OA'(observation_count); ow_data <= payload[127:0];
                                    observation_count <= observation_count + 1'b1;
                                end else observation_overflow <= 1;
                                reply(OK, 0);
                            end
                        end
                        MAKE_PREVIEW: begin
                            if (phase == COMMITTED) reply(INVALID_STATE, 0);
                            else begin
                                phase <= PREPARING; candidate_valid <= 0;
                                pulses <= 0; delivered <= 0; masked <= 0; eligible <= 0;
                                capture_refused <= 0; request_refused <= 0;
                                if (active_bank) event_count_0 <= 0; else event_count_1 <= 0;
                                addition = {1'b0, frame}+{33'b0, quantum};
                                if (&serial) reply(EXHAUSTED, 0);
                                else begin
                                    serial <= serial+1'b1;
                                    if (addition[64]) reply(EXHAUSTED, 0);
                                    else if (observation_overflow || observation_count > loop_slots+request_slots)
                                        reply(CAPACITY, 0);
                                    else begin observation_cursor <= 0; state <= S_PREVIEW_OBS; end
                                end
                            end
                        end
                        COMMIT: begin
                            if (phase != PREVIEW || !candidate_valid || payload[63:0] != frame || payload[127:64] != serial)
                                reply(INVALID_STATE, 0);
                            else begin active_bank <= !active_bank; phase <= COMMITTED; reply(OK, 0); end
                        end
                        FINISH: begin
                            if (phase != COMMITTED || payload[63:0] != frame || payload[127:64] != serial)
                                reply(INVALID_STATE, 0);
                            else begin cursor <= 0; state <= S_FINISH_LOOP; end
                        end
                        READ_STATE: begin
                            if (index >= loop_slots) reply(INVALID_ID, 0);
                            else read_loop(index, active_bank, 0, 0, S_READ_LOOP_REPLY);
                        end
                        READ_REQUEST: begin
                            if (index >= request_slots) reply(INVALID_ID, 0);
                            else read_request(index, active_bank, S_READ_REQUEST_REPLY);
                        end
                        READ_DECISION: begin
                            if ((phase != PREVIEW && phase != COMMITTED) || !candidate_valid
                                || payload[63:0] != frame || payload[127:64] != serial) reply(INVALID_STATE, 0);
                            else if (index >= loop_slots) reply(INVALID_ID, 0);
                            else read_loop(index, phase == COMMITTED ? active_bank : !active_bank, 0, 0, S_READ_LOOP_REPLY);
                        end
                        READ_EVENT: begin
                            if (payload[0] && (phase != PREVIEW || !candidate_valid)) reply(INVALID_STATE, 0);
                            else if (index >= (payload[0] ? scratch_events : active_events)) reply(INVALID_ID, 0);
                            else begin
                                er_en <= 1; er_address <= event_address(index, payload[0] ? !active_bank : active_bank);
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
                S_R_GET: begin request_row <= rr_data; state <= read_return; end
                S_S_WAIT: state <= S_S_GET;
                S_S_GET: begin spec_row <= sr_data; state <= read_return; end
                S_M_WAIT: state <= S_M_GET;
                S_M_GET: begin member_row <= mr_data; state <= read_return; end
                S_O_WAIT: state <= S_O_GET;
                S_O_GET: begin observation_row <= or_data; state <= read_return; end
                S_E_WAIT: state <= S_E_GET;
                S_E_GET: begin event_row <= er_data; state <= read_return; end
                S_PREP_LOOP: begin
                    if (cursor == loop_slots) begin cursor <= 0; state <= S_PREP_MEMBER; end
                    else if (!spec_written[cursor[SA-1:0]]) reply(INVALID_CONFIG, 0);
                    else read_spec(cursor, S_PREP_SPEC);
                end
                S_PREP_SPEC: begin
                    if (spec_row[SPEC_KIND+:2] != 0 || spec_row[SPEC_TIMELINE]) reply(UNSUPPORTED, 0);
                    else if (spec_row[VALID] && spec_row[GENERATION+:32] == 0) reply(INVALID_ID, 0);
                    else if (spec_row[VALID] && (spec_row[SPEC_LENGTH+:64] & (64'(quantum)-1'b1)) != 0)
                        reply(INVALID_FRAME, 0);
                    else begin
                        temporary_loop = 0;
                        if (spec_row[VALID]) begin
                            temporary_loop[VALID] = 1;
                            temporary_loop[GENERATION+:32] = spec_row[GENERATION+:32];
                            temporary_loop[REPEAT] = spec_row[SPEC_REPEAT];
                            temporary_loop[LENGTH+:64] = spec_row[SPEC_LENGTH+:64];
                            valid_loops[cursor[SA-1:0]] <= 1;
                        end
                        write_loop(cursor, 0, temporary_loop);
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
                S_PREP_MEMBER: begin
                    if (cursor == member_count) begin
                        if (seen_members != valid_loops) reply(INVALID_CONFIG, 0);
                        else begin cursor <= 0; state <= S_PREP_REQUEST; end
                    end else if (!member_written[cursor[SA-1:0]]) reply(INVALID_CONFIG, 0);
                    else read_member(cursor, S_PREP_MEMBER_CHECK);
                end
                S_PREP_MEMBER_CHECK: begin
                    if (member_row[31:0] >= loop_slots) reply(INVALID_ID, 0);
                    else if (seen_members[member_row[SA-1:0]]) reply(DUPLICATE_ID, 0);
                    else if (cursor+1'b1 == member_count && !member_row[32]) reply(INVALID_CONFIG, 0);
                    else read_loop(member_row[31:0], 0, 0, 0, S_PREP_MEMBER_LOOP);
                end
                S_PREP_MEMBER_LOOP: begin
                    if (!loop_row[VALID]) reply(INVALID_ID, 0);
                    else begin seen_members[member_row[SA-1:0]] <= 1; cursor <= cursor+1'b1; state <= S_PREP_MEMBER; end
                end
                S_PREP_REQUEST: begin
                    temporary_request = 0; temporary_request[R_GEN+:32] = 1;
                    write_request(cursor, 0, temporary_request);
                    if (cursor+1'b1 == request_slots) state <= S_PREP_COMPLETE;
                    else cursor <= cursor+1'b1;
                end
                S_PREP_COMPLETE: begin configured <= 1; phase <= IDLE_PHASE; reply(OK, 0); end
                S_SUBMIT_TARGET: begin
                    if (!spec_row[VALID] || spec_row[GENERATION+:32] != payload[R_TARGET+32+:32]) reply(INVALID_ID, 0);
                    else if ((payload[R_OP+:2] == OP_PLAY || payload[R_OP+:2] == OP_SEEK)
                        && (payload[R_POSITION+:64] & (64'(quantum)-1'b1)) != 0) reply(INVALID_FRAME, 0);
                    else begin
                        target_source <= spec_row[SPEC_SOURCE+:64]; target_source_valid <= spec_row[SPEC_SOURCE_VALID];
                        if (payload[R_TICKS]) begin
                            if (payload[R_SOURCE+:32] >= loop_slots) reply(INVALID_ID, 0);
                            else read_spec(payload[R_SOURCE+:32], S_SUBMIT_SOURCE);
                        end else begin request_cursor <= 0; free_found <= 0; state <= S_SUBMIT_SCAN; end
                    end
                end
                S_SUBMIT_SOURCE: begin
                    if (!spec_row[VALID] || spec_row[GENERATION+:32] != payload[R_SOURCE+32+:32]) reply(INVALID_ID, 0);
                    else if (!target_source_valid || target_source != payload[R_SOURCE+:64]) reply(MISSING_SYNC, 0);
                    else begin request_cursor <= 0; free_found <= 0; state <= S_SUBMIT_SCAN; end
                end
                S_SUBMIT_SCAN: begin
                    if (request_cursor == request_slots) state <= S_SUBMIT_WRITE;
                    else read_request(request_cursor, active_bank, S_SUBMIT_CHECK);
                end
                S_SUBMIT_CHECK: begin
                    if (request_row[R_VALID] && request_row[R_SEQUENCE+:64] == payload[R_SEQUENCE+:64]) reply(DUPLICATE_ID, 0);
                    else begin
                        if (!free_found && !request_row[R_VALID] && request_row[R_GEN+:32] != 0) begin
                            free_found <= 1; free_slot <= request_cursor; free_generation <= request_row[R_GEN+:32];
                        end
                        request_cursor <= request_cursor+1'b1; state <= S_SUBMIT_SCAN;
                    end
                end
                S_SUBMIT_WRITE: begin
                    rounded_frame = payload[R_AT+:64] > frame ? payload[R_AT+:64] : frame;
                    addition = {1'b0,rounded_frame}+{33'b0,quantum}-1'b1;
                    if ((rounded_frame & (64'(quantum)-1'b1)) == 0) addition = {1'b0,rounded_frame};
                    if (!free_found) reply(CAPACITY, 0);
                    else if (addition[64]) reply(INVALID_FRAME, 0);
                    else begin
                        temporary_request = payload;
                        temporary_request[63:0] = 0;
                        temporary_request[R_VALID] = 1;
                        temporary_request[R_GEN+:32] = free_generation;
                        temporary_request[R_TICKS] = payload[R_TICKS];
                        temporary_request[R_OP+:2] = payload[R_OP+:2];
                        temporary_request[R_RETRIGGER] = payload[R_RETRIGGER];
                        temporary_request[R_EFFECTIVE+:64] = addition[63:0] & ~(64'(quantum)-1'b1);
                        temporary_request[R_REMAINING+:64] = payload[R_TICKS] ? payload[R_SKIP+:64] : 0;
                        temporary_request[R_FRAME+:64] = 0;
                        write_request(free_slot, active_bank, temporary_request);
                        reply(OK, {768'b0, free_generation, free_slot});
                    end
                end
                S_CANCEL_COLLECT: begin
                    if (!request_row[R_VALID] || request_row[R_GEN+:32] != payload[31:0]) reply(INVALID_ID, 0);
                    else if (opcode == CANCEL) begin
                        if (request_row[R_OUTCOME+:3] != 0) reply(OK, 0);
                        else begin
                            write_request(index, active_bank, terminal(request_row, CANCELLED, OK, frame));
                            reply(OK, 832'd1);
                        end
                    end else if (request_row[R_OUTCOME+:3] == 0) reply(OK, 0);
                    else begin
                        temporary_request = 0;
                        temporary_request[R_GEN+:32] = request_row[R_GEN+:32]+1'b1;
                        write_request(index, active_bank, temporary_request);
                        reply(OK, {192'b0,request_row});
                    end
                end
                S_PREVIEW_OBS: begin
                    if (observation_cursor == observation_count) begin cursor <= 0; state <= S_COPY_LOOP; end
                    else read_observation(observation_cursor, S_OBS_CHECK);
                end
                S_OBS_CHECK: begin
                    observation_key <= observation_row[64:0];
                    observation_reason <= observation_row[65+:4];
                    duplicate_cursor <= 0;
                    if (observation_row[65+:4] == 0 || observation_row[65+:4] > UNSUPPORTED) reply(UNSUPPORTED, 0);
                    else if (observation_row[0]) begin
                        if (observation_row[1+:32] >= request_slots) reply(INVALID_ID, 0);
                        else read_request(observation_row[1+:32], active_bank, S_OBS_REQUEST);
                    end else begin
                        if (observation_row[1+:32] >= loop_slots) reply(INVALID_ID, 0);
                        else read_spec(observation_row[1+:32], S_OBS_TARGET);
                    end
                end
                S_OBS_TARGET: begin
                    if (!spec_row[VALID] || spec_row[GENERATION+:32] != observation_key[64:33]) reply(INVALID_ID, 0);
                    else state <= S_OBS_DUP;
                end
                S_OBS_REQUEST: begin
                    if (!request_row[R_VALID] || request_row[R_GEN+:32] != observation_key[64:33]) reply(INVALID_ID, 0);
                    else if (request_row[R_OUTCOME+:3] != 0) reply(INVALID_STATE, 0);
                    else state <= S_OBS_DUP;
                end
                S_OBS_DUP: begin
                    if (duplicate_cursor == observation_cursor) state <= S_OBS_NEXT;
                    else read_observation(duplicate_cursor, S_OBS_DUP_CHECK);
                end
                S_OBS_DUP_CHECK: begin
                    if (observation_row[64:0] == observation_key) reply(DUPLICATE_ID, 0);
                    else begin duplicate_cursor <= duplicate_cursor+1'b1; state <= S_OBS_DUP; end
                end
                S_OBS_NEXT: begin
                    if (observation_key[0]) begin
                        request_refused[observation_key[1+:PA]] <= 1;
                        request_reason[observation_key[1+:PA]] <= observation_reason;
                    end else begin
                        capture_refused[observation_key[1+:SA]] <= 1;
                        capture_reason[observation_key[1+:SA]] <= observation_reason;
                    end
                    observation_cursor <= observation_cursor+1'b1; state <= S_PREVIEW_OBS;
                end
                S_COPY_LOOP: begin
                    if (cursor == loop_slots) begin cursor <= 0; state <= S_COPY_REQUEST; end
                    else read_loop(cursor, active_bank, 0, 0, S_COPY_LOOP_WRITE);
                end
                S_COPY_LOOP_WRITE: begin
                    write_loop(cursor, !active_bank, {256'b0,loop_row[575:0]});
                    cursor <= cursor+1'b1; state <= S_COPY_LOOP;
                end
                S_COPY_REQUEST: begin
                    if (cursor == request_slots) begin request_cursor <= 0; state <= S_DIRECT_SCAN; end
                    else read_request(cursor, active_bank, S_COPY_REQUEST_WRITE);
                end
                S_COPY_REQUEST_WRITE: begin
                    write_request(cursor, !active_bank, request_row);
                    cursor <= cursor+1'b1; state <= S_COPY_REQUEST;
                end
                S_DIRECT_SCAN: begin
                    if (request_cursor == request_slots) begin component_start <= 0; state <= S_COMPONENT; end
                    else read_request(request_cursor, !active_bank, S_DIRECT_CHECK);
                end
                S_DIRECT_CHECK: begin
                    if (request_row[R_VALID] && request_row[R_OUTCOME+:3] == 0
                        && request_row[R_EFFECTIVE+:64] <= frame && !request_row[R_TICKS])
                        offer(request_cursor, S_OFFER_LOOP);
                    else begin request_cursor <= request_cursor+1'b1; state <= S_DIRECT_SCAN; end
                end
                S_OFFER_LOOP: begin request_cursor <= request_cursor+1'b1; state <= S_DIRECT_SCAN; end
                S_OFFER_CHECK: begin
                    temporary_loop = loop_row;
                    if (!loop_row[INTENT_VALID]) begin
                        temporary_loop[INTENT_VALID] = 1;
                        temporary_loop[INTENT_SLOT+:32] = offer_slot;
                        temporary_loop[INTENT_SEQ+:64] = offer_sequence;
                        write_loop(offer_target, !active_bank, temporary_loop);
                        state <= offer_return;
                    end else begin
                        if (offer_sequence > loop_row[INTENT_SEQ+:64]) begin
                            loser_slot <= loop_row[INTENT_SLOT+:32];
                            temporary_loop[INTENT_SLOT+:32] = offer_slot;
                            temporary_loop[INTENT_SEQ+:64] = offer_sequence;
                            write_loop(offer_target, !active_bank, temporary_loop);
                            read_request(loop_row[INTENT_SLOT+:32], !active_bank, S_OFFER_LOSER);
                        end else begin
                            loser_slot <= offer_slot;
                            read_request(offer_slot, !active_bank, S_OFFER_LOSER);
                        end
                    end
                end
                S_OFFER_LOSER: begin
                    write_request(loser_slot, !active_bank, terminal(request_row, SUPERSEDED, OK, frame));
                    state <= offer_return;
                end
                S_COMPONENT: begin
                    if (component_start == member_count) begin cursor <= 0; state <= S_FINAL; end
                    else begin member_cursor <= component_start; state <= S_COMPONENT_END; end
                end
                S_COMPONENT_END: read_member(member_cursor, S_COMPONENT_END_CHECK);
                S_COMPONENT_END_CHECK: begin
                    if (member_row[32]) begin
                        component_end <= member_cursor+1'b1;
                        member_cursor <= component_start; state <= S_FIRST;
                    end else begin member_cursor <= member_cursor+1'b1; state <= S_COMPONENT_END; end
                end
                S_FIRST: begin
                    if (member_cursor == component_end) begin member_cursor <= component_start; state <= S_SECOND; end
                    else read_member(member_cursor, S_FIRST_MEMBER);
                end
                S_FIRST_MEMBER: begin
                    target_slot <= member_row[31:0];
                    read_loop(member_row[31:0], !active_bank, 0, 1, S_FIRST_APPLY);
                end
                S_FIRST_APPLY: begin apply_return <= S_FIRST_NATURAL; state <= S_APPLY; end
                S_FIRST_NATURAL: begin natural_return <= S_FIRST_INHERIT; state <= S_NATURAL; end
                S_FIRST_INHERIT: begin
                    if (spec_row[SPEC_SOURCE_VALID] && delivered[spec_row[SPEC_SOURCE+:SA]] && !masked[target_slot[SA-1:0]])
                        pulses[target_slot[SA-1:0]] <= 1;
                    member_cursor <= member_cursor+1'b1; state <= S_FIRST;
                end
                S_SECOND: begin
                    if (member_cursor == component_end) begin pulse_cursor <= component_start; state <= S_FIND_PULSE; end
                    else read_member(member_cursor, S_SECOND_MEMBER);
                end
                S_SECOND_MEMBER: begin
                    target_slot <= member_row[31:0];
                    read_loop(member_row[31:0], !active_bank, 0, 1, S_SECOND_REACT);
                end
                S_SECOND_REACT: begin
                    if (spec_row[SPEC_SOURCE_VALID] && delivered[spec_row[SPEC_SOURCE+:SA]]) begin
                        react_source <= spec_row[SPEC_SOURCE+:32];
                        react_return <= S_SECOND_NEXT; request_cursor <= 0; state <= S_REACT;
                    end else state <= S_SECOND_NEXT;
                end
                S_SECOND_NEXT: begin member_cursor <= member_cursor+1'b1; state <= S_SECOND; end
                S_FIND_PULSE: begin
                    if (pulse_cursor == component_end) begin component_start <= component_end; state <= S_COMPONENT; end
                    else read_member(pulse_cursor, S_FIND_PULSE_CHECK);
                end
                S_FIND_PULSE_CHECK: begin
                    if (pulses[member_row[SA-1:0]] && !delivered[member_row[SA-1:0]]) begin
                        target_slot <= member_row[31:0]; pulse_slot <= member_row[31:0];
                        delivered[member_row[SA-1:0]] <= 1;
                        read_loop(member_row[31:0], !active_bank, 0, 0, S_PULSE_EMIT);
                    end else begin pulse_cursor <= pulse_cursor+1'b1; state <= S_FIND_PULSE; end
                end
                S_PULSE_EMIT: begin
                    if (scratch_events == event_limit) reply(CAPACITY, 0);
                    else begin
                        temporary_loop = loop_row;
                        temporary_loop[LAST_VALID] = 1; temporary_loop[LAST_PULSE+:64] = frame;
                        write_loop(target_slot, !active_bank, temporary_loop);
                        emit_event(2, OP_STOP, 0, 0);
                        follow_cursor <= component_start; state <= S_FOLLOW;
                    end
                end
                S_FOLLOW: begin
                    if (follow_cursor == component_end) begin pulse_cursor <= component_start; state <= S_FIND_PULSE; end
                    else read_member(follow_cursor, S_FOLLOW_MEMBER);
                end
                S_FOLLOW_MEMBER: begin
                    target_slot <= member_row[31:0];
                    read_loop(member_row[31:0], !active_bank, 0, 1, S_FOLLOW_REACT);
                end
                S_FOLLOW_REACT: begin
                    if (spec_row[SPEC_SOURCE_VALID] && spec_row[SPEC_SOURCE+:32] == pulse_slot) begin
                        if (!masked[target_slot[SA-1:0]]) pulses[target_slot[SA-1:0]] <= 1;
                        react_source <= pulse_slot;
                        react_return <= S_FOLLOW_NEXT; request_cursor <= 0; state <= S_REACT;
                    end else state <= S_FOLLOW_NEXT;
                end
                S_FOLLOW_NEXT: begin follow_cursor <= follow_cursor+1'b1; state <= S_FOLLOW; end
                S_REACT: begin
                    if (request_cursor == request_slots) read_loop(target_slot, !active_bank, 0, 1, S_REACT_APPLY);
                    else read_request(request_cursor, !active_bank, S_REACT_CHECK);
                end
                S_REACT_CHECK: begin
                    if (request_row[R_VALID] && request_row[R_OUTCOME+:3] == 0
                        && request_row[R_EFFECTIVE+:64] <= frame
                        && request_row[R_TARGET+:32] == target_slot
                        && !eligible[request_cursor[PA-1:0]] && request_row[R_TICKS]
                        && request_row[R_SOURCE+:32] == react_source) begin
                        eligible[request_cursor[PA-1:0]] <= 1;
                        if (request_row[R_REMAINING+:64] != 0) begin
                            temporary_request = request_row;
                            temporary_request[R_REMAINING+:64] = request_row[R_REMAINING+:64]-1'b1;
                            write_request(request_cursor, !active_bank, temporary_request);
                            request_cursor <= request_cursor+1'b1; state <= S_REACT;
                        end else offer(request_cursor, S_REACT_OFFER_DONE);
                    end else begin request_cursor <= request_cursor+1'b1; state <= S_REACT; end
                end
                S_REACT_OFFER_DONE: begin request_cursor <= request_cursor+1'b1; state <= S_REACT; end
                S_REACT_APPLY: begin apply_return <= S_REACT_NATURAL; state <= S_APPLY; end
                S_REACT_NATURAL: begin natural_return <= react_return; state <= S_NATURAL; end
                S_APPLY: begin
                    if (!loop_row[INTENT_VALID]) state <= apply_return;
                    else begin
                        apply_slot <= loop_row[INTENT_SLOT+:32]; loop_row[INTENT_VALID] <= 0;
                        read_request(loop_row[INTENT_SLOT+:32], !active_bank, S_APPLY_CHECK);
                    end
                end
                S_APPLY_CHECK: begin
                    if (apply_event && scratch_events == event_limit) reply(CAPACITY, 0);
                    else begin
                        loop_row <= apply_loop;
                        write_loop(target_slot, !active_bank, apply_loop);
                        write_request(apply_slot, !active_bank, apply_request);
                        if (apply_event) emit_event(0, request_row[R_OP+:2], request_row[R_RETRIGGER], request_row[R_POSITION+:64]);
                        if (apply_stop) begin
                            masked[target_slot[SA-1:0]] <= 1; pulses[target_slot[SA-1:0]] <= 0;
                            stop_sequence <= request_row[R_SEQUENCE+:64]; request_cursor <= 0; state <= S_CANCEL_OLDER;
                        end else begin
                            if (apply_pulse) begin masked[target_slot[SA-1:0]] <= 0; pulses[target_slot[SA-1:0]] <= 1; end
                            state <= apply_return;
                        end
                    end
                end
                S_CANCEL_OLDER: begin
                    if (request_cursor == request_slots) state <= apply_return;
                    else read_request(request_cursor, !active_bank, S_CANCEL_OLDER_CHECK);
                end
                S_CANCEL_OLDER_CHECK: begin
                    if (request_row[R_VALID] && request_row[R_OUTCOME+:3] == 0
                        && request_row[R_TARGET+:32] == target_slot && request_row[R_SEQUENCE+:64] < stop_sequence)
                        write_request(request_cursor, !active_bank, terminal(request_row, CANCELLED, OK, frame));
                    request_cursor <= request_cursor+1'b1; state <= S_CANCEL_OLDER;
                end
                S_NATURAL: begin
                    if (!at_end(loop_row)) state <= natural_return;
                    else if (loop_row[REPEAT] && spec_row[SPEC_SOURCE_VALID] && spec_row[SPEC_SOURCE+:32] != target_slot)
                        read_loop(spec_row[SPEC_SOURCE+:32], !active_bank, 1, 0, S_NATURAL_SOURCE);
                    else state <= S_NATURAL_WRAP;
                end
                S_NATURAL_SOURCE: begin
                    if (source_mode == PLAYING && !delivered[spec_row[SPEC_SOURCE+:SA]]) state <= natural_return;
                    else state <= S_NATURAL_WRAP;
                end
                S_NATURAL_WRAP: begin
                    if (&loop_row[CYCLES+:64]) reply(EXHAUSTED, 0);
                    else if (scratch_events == event_limit) reply(CAPACITY, 0);
                    else begin
                        temporary_loop = loop_row;
                        temporary_loop[POSITION+:64] = 0; temporary_loop[CYCLES+:64] = loop_row[CYCLES+:64]+1'b1;
                        temporary_loop[DISCONT] = 1; pulses[target_slot[SA-1:0]] <= 1;
                        loop_row <= temporary_loop; write_loop(target_slot, !active_bank, temporary_loop);
                        emit_event(1, OP_STOP, 0, 0); state <= natural_return;
                    end
                end
                S_FINAL: begin
                    if (cursor == loop_slots) begin
                        candidate_valid <= 1; phase <= PREVIEW; reply(OK, {704'b0,serial,frame});
                    end else read_loop(cursor, !active_bank, 0, 0, S_FINAL_CHECK);
                end
                S_FINAL_CHECK: begin
                    addition = {1'b0,loop_row[POSITION+:64]}+{33'b0,quantum};
                    if (loop_row[VALID] && plays(loop_row) && (addition[64] || addition[63:0] > loop_row[LENGTH+:64]))
                        reply(INVALID_FRAME, 0);
                    else begin
                        addition = {1'b0,loop_row[CAPTURE+:64]}+{33'b0,quantum};
                        if (loop_row[VALID] && loop_row[MODE+:2] == RECORDING && addition[64]) reply(EXHAUSTED, 0);
                        else begin cursor <= cursor+1'b1; state <= S_FINAL; end
                    end
                end
                S_FINISH_LOOP: begin
                    if (cursor == loop_slots) begin frame <= frame+64'(quantum); phase <= IDLE_PHASE; candidate_valid <= 0; reply(OK, 0); end
                    else read_loop(cursor, active_bank, 0, 0, S_FINISH_WRITE);
                end
                S_FINISH_WRITE: begin
                    temporary_loop = loop_row;
                    if (loop_row[VALID] && plays(loop_row)) temporary_loop[POSITION+:64] = loop_row[POSITION+:64]+64'(quantum);
                    if (loop_row[VALID] && loop_row[MODE+:2] == RECORDING) temporary_loop[CAPTURE+:64] = loop_row[CAPTURE+:64]+64'(quantum);
                    write_loop(cursor, active_bank, temporary_loop); cursor <= cursor+1'b1; state <= S_FINISH_LOOP;
                end
                S_READ_LOOP_REPLY: reply(OK, loop_row);
                S_READ_REQUEST_REPLY: reply(OK, {192'b0,request_row});
                S_READ_EVENT_REPLY: reply(OK, {576'b0,event_row});
                default: reply(INVALID_STATE, 0);
            endcase
        end
    end

    wire unused_reserved_bits = &{1'b0, payload, observation_row, member_row, spec_row,
        loop_row, request_row};
endmodule
