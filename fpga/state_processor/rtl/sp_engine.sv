`include "state_processor_defs.svh"
`include "sp_loop_defs.svh"

module sp_engine #(
    parameter integer PROGRAM_DEPTH = 256,
    parameter integer STATE_DEPTH = 256,
    parameter integer LOOP_DEPTH = STATE_DEPTH,
    parameter integer LOOP_MEMORY_BITS = $clog2(LOOP_DEPTH*16),
    parameter integer PROGRAM_ADDR_BITS = (PROGRAM_DEPTH > 1) ? $clog2(PROGRAM_DEPTH) : 1,
    parameter integer STATE_ADDR_BITS = (STATE_DEPTH > 1) ? $clog2(STATE_DEPTH) : 1,
    parameter integer PC_BITS = $clog2(PROGRAM_DEPTH + 1),
    parameter integer STREAM_JOBS = 0
) (
    input  wire                                clk,
    input  wire                                reset,
    input  wire                                start,
    input  wire [31:0]                         instruction_count,
    input wire [31:0] sync_count,
    input  wire [31:0]                         state_count,
    input  wire [31:0]                         quantum,
    input wire [31:0] chunk_frames,
    input  wire [31:0]                         queued_jobs,
    input  wire                                abort_run,
    output reg                                 busy,
    output reg                                 done,
    output reg  [7:0]                          error_code,
    output wire [31:0]                         instruction_pc,
    output wire                                program_read_enable,
    output wire [PROGRAM_ADDR_BITS-1:0]         program_read_address,
    input  wire                                program_available,
    input  wire                                program_error,
    input  wire [`SHOOP_SP_INSTRUCTION_BITS-1:0] program_read_data,
    output wire                                state_read_enable,
    output wire [STATE_ADDR_BITS-1:0]           state_read_address,
    input  wire [`SHOOP_SP_STATE_BITS-1:0]       state_read_data,
    output wire                                state_write_enable,
    output wire [STATE_ADDR_BITS-1:0]           state_write_address,
    output wire [`SHOOP_SP_STATE_BITS-1:0]       state_write_data,
    output wire                                job_valid,
    input  wire                                job_ready,
    output reg  [`SHOOP_SP_JOB_BITS-1:0]         job_data,
    output wire loop_read_valid,
    input wire loop_read_ready,
    output wire [31:0] loop_read_slot,
    input wire loop_response_valid,
    input wire loop_response_error,
    input wire [163:0] loop_response_data,
    output wire loop_ram_read_enable,
    output wire [LOOP_MEMORY_BITS-1:0] loop_ram_read_address,
    input wire [63:0] loop_ram_read_data,
    output wire loop_ram_write_enable,
    output wire [LOOP_MEMORY_BITS-1:0] loop_ram_write_address,
    output wire [63:0] loop_ram_write_data,
    output wire sync_event_valid,
    input wire sync_event_ready,
    output wire [255:0] sync_event_data,
    output reg [63:0] sync_frame,
    output wire faulted
);
    typedef enum logic [6:0] {
        IDLE, ADMIT, ADD_LOW, ADD_HIGH, FRAME_FINISH, FETCH, DECODE, DISPATCH, READ_REQUEST, READ_CAPTURE, READ_RESULT, EMIT, NEXT,
        LOOP_REQUEST, LOOP_WAIT, LOOP_RESULT,
        SYNC_BEGIN, SYNC_LOAD, SYNC_CHECK, SYNC_SOURCE_FLAGS, SYNC_SOURCE_CHECK,
        SYNC_SCAN_BEGIN, SYNC_SCAN, SYNC_EFFECTIVE, SYNC_REMAINING, SYNC_CANDIDATE, SYNC_SCAN_NEXT,
        SYNC_APPLY, SYNC_OP_BEGIN, SYNC_OP_CHECK, SYNC_RECORD_REV, SYNC_NEW_REV,
        SYNC_RESET_BEGIN, SYNC_RESET_CYCLES, SYNC_SUCCESS, SYNC_AFTER_APPLY, SYNC_EFFECTS,
        SYNC_CANCEL, SYNC_CANCEL_NEXT, SYNC_GROUP_DONE, SYNC_NATURAL, SYNC_WRAP, SYNC_WRAP_EVENT,
        SYNC_SOURCE, SYNC_DELIVER, SYNC_SELF, SYNC_RENDER, SYNC_STAMP,
        SYNC_ADVANCE, SYNC_ADV_POSITION, SYNC_ADV_COMMIT, SYNC_ADV_CAPTURE, SYNC_STORE, SYNC_FINISH,
        SYNC_WRITE, SYNC_EVENT, OWN_FLAGS, OWN_STAMP, OWN_RENDER, OWN_LENGTH, MIDI_DISPATCH, MIDI_CAPTURE_FLAGS, MIDI_CAPTURE_BEGIN, MIDI_CAPTURE_APPEND, MIDI_CAPTURE_POSITION
    } phase_t;
    typedef enum logic [2:0] {
        ENABLE, POSITION, LENGTH, VIEW, RAMP, SMOOTHING, LOOP_MEMORY
    } operand_t;

    reg [5:0] chunk_shift;
    function automatic [5:0] shift_for(input [31:0] frames);
        begin
            shift_for = 0;
            for (integer bit_index=0; bit_index<32; bit_index=bit_index+1)
                if (frames[bit_index]) shift_for = 6'(bit_index);
        end
    endfunction
    phase_t phase;
    operand_t operand;
    reg [PC_BITS-1:0] pc;
    reg [PC_BITS-1:0] run_count;
    reg [31:0] run_state_count;
    reg [31:0] run_quantum;
    localparam integer SYNC_COUNT_BITS = $clog2(LOOP_DEPTH + 1);
    reg [SYNC_COUNT_BITS-1:0] sync_remaining;
    reg [63:0] run_frame;
    reg run_has_sync, last_instruction;
    reg begin_capture;
    reg [1:0] playback_flags;
    reg [255:0] capture_job;
    reg [7:0] opcode, instruction_reserved;
    wire [95:0] instruction={instruction_reserved,arg4,arg3,arg2,arg1,arg0,opcode};
    reg [15:0] arg0, arg1, arg2, arg3, arg4;
    reg [15:0] operand_address;
    reg midi_restart;
    phase_t emit_return;
    wire midi_loop_instruction = opcode==`SHOOP_SP_MIDI_PLAYBACK_INSTRUCTION || opcode==`SHOOP_SP_MIDI_CAPTURE_INSTRUCTION || opcode==`SHOOP_SP_MIDI_STATE_INSTRUCTION
        || opcode==`SHOOP_SP_MIDI_PREPARE_CAPTURE;
    reg [63:0] position;
    reg [63:0] length;
    reg writeback;
    reg [95:0] ramp_state;
    reg [32:0] ramp_elapsed;
    reg [`SHOOP_SP_STATE_BITS-1:0] operand_data;
    reg shape_bad, program_bad, state_bad, empty_program;
    reg loop_invalid, loop_at_start;
    reg [1:0] loop_mode;
    reg [1:0] operand_nonzero_parts;
    wire word_nonzero = |operand_nonzero_parts;
    wire [63:0] read_word = operand==LOOP_MEMORY ? loop_ram_read_data : state_read_data[63:0];
    wire [63:0] word_value = operand_data[63:0];
    wire is_ramp = operand_data[128];
    wire unused_reserved = &{1'b0, instruction_reserved};
    wire operand_in_range = {16'b0, operand_address} < run_state_count;

    function automatic [`SHOOP_SP_JOB_BITS-1:0] pack_job(
        input [7:0] kind,
        input [15:0] id0,
        input [15:0] id1,
        input [63:0] payload0,
        input [63:0] payload1,
        input [63:0] payload2
    );
        pack_job = {payload2, payload1, payload0, 24'b0, id1, id0, kind};
    endfunction

    assign instruction_pc = 32'(pc);
    assign program_read_enable = (phase == FETCH) && !reset;
    assign program_read_address = PROGRAM_ADDR_BITS'(pc);
    assign state_read_enable = (phase == READ_REQUEST) && operand != LOOP_MEMORY && operand_in_range && !reset;
    assign state_read_address = STATE_ADDR_BITS'(operand_address);
    assign job_valid = (phase == EMIT) && !abort_run && !reset;
    assign state_write_enable = job_valid && job_ready && writeback;
    assign state_write_address = STATE_ADDR_BITS'(arg0);
    assign state_write_data = {1'b1,
        ramp_elapsed >= {1'b0, ramp_state[95:64]} ? ramp_state[95:64] : ramp_elapsed[31:0],
        ramp_state};
    assign loop_read_valid = phase == LOOP_REQUEST && !abort_run && !reset;
    assign loop_read_slot = {16'b0, arg0};

    localparam integer V=`SHOOP_SP_L_VALID, G=`SHOOP_SP_L_GEN, M=`SHOOP_SP_L_MODE,
        REP=`SHOOP_SP_L_REPEAT, SV=`SHOOP_SP_L_SOURCE_VALID, SS=`SHOOP_SP_L_SOURCE,
        PULSE=`SHOOP_SP_L_PULSE, BC=`SHOOP_SP_L_BEGIN_CAPTURE,
        PV=`SHOOP_SP_L_PROCESSED_VALID, NEWER=`SHOOP_SP_L_NEWER;
    localparam [3:0] FLAGS=`SHOOP_SP_L_FLAGS, CONTROL=`SHOOP_SP_L_CONTROL,
        POS=`SHOOP_SP_L_POSITION, LEN=`SHOOP_SP_L_LENGTH, CAP=`SHOOP_SP_L_CAPTURE,
        CYC=`SHOOP_SP_L_CYCLES, CR=`SHOOP_SP_L_CONTENT_REV, WM=`SHOOP_SP_L_WATERMARK,
        RP=`SHOOP_SP_L_RENDER_POSITION, PF=`SHOOP_SP_L_PROCESSED_FRAME;
    localparam integer P0=`SHOOP_SP_L_PLAN0_FLAGS, P1=`SHOOP_SP_L_PLAN1_FLAGS,
        VALID=`SHOOP_SP_P_VALID, TICKS=`SHOOP_SP_P_TICKS,
        OP=`SHOOP_SP_P_OP, RET=`SHOOP_SP_P_RETRIGGER;
    localparam [7:0] INVALID_STATE=5, INVALID_FRAME=6, EXHAUSTED=8, EMPTY_CONTENT=9;

    phase_t memory_return, sync_event_return, sync_natural_return, operation_return;
    reg [63:0] flags, control, capture_frames;
    reg [LOOP_MEMORY_BITS-1:0] loop_address;
    reg [3:0] loop_word;
    reg [1:0] sync_pass;
    reg scan_index, winner, winner_valid, cancel_index;
    reg pulse_pending, delivered, masked, protected_valid, protected_newer;
    reg changed, applied_pulse, sync_fault;
    wire [31:0] target={16'b0,arg0};
    wire [31:0] generation={arg2,arg1};
    wire [63:0] source_flags=capture_job[63:0];
    reg inline_sync, source_valid, source_self;
    reg sync_flags_invalid, sync_control_matches;
    reg [31:0] source_slot;
    wire selected_source_valid=inline_sync ? arg3!=16'hffff : flags[SV];
    wire [31:0] selected_source_slot={16'b0,inline_sync ? arg3 : flags[SS+:16]};
    wire source_tick=source_valid && !source_self && source_flags[PULSE];
    wire source_playing=source_flags[M+:2]==1;
    wire [4:0] scan_flags=scan_index ? control[P1+:5] : control[P0+:5];
    wire [4:0] winner_flags=winner ? control[P1+:5] : control[P0+:5];
    wire [4:0] cancel_flags=cancel_index ? control[P1+:5] : control[P0+:5];
    wire winner_newer=winner==flags[NEWER];
    wire [1:0] operation_code=ramp_state[65:64];
    wire retrigger=ramp_state[66];
    wire [63:0] operation_position=ramp_state[63:0];
    wire preserve=flags[M+:2]==1 && !retrigger;
    wire [63:0] playable_length=flags[M+:2]==2 ? capture_frames : length;
    reg [63:0] operation_length;
    reg operation_empty, operation_in_range;
    reg position_in_range, word_at_frame, word_after_frame, advanced_invalid;
    reg [63:0] arithmetic_left, arithmetic_right;
    reg [32:0] arithmetic_low;
    reg [64:0] arithmetic_sum;
    phase_t arithmetic_return;
    wire [32:0] arithmetic_high={1'b0,arithmetic_left[63:32]}
        +{1'b0,arithmetic_right[63:32]}+{32'b0,arithmetic_low[32]};
    task automatic add_words(input [63:0] left, input [63:0] right, input phase_t destination);
        begin
            arithmetic_left<=left; arithmetic_right<=right;
            arithmetic_return<=destination; phase<=ADD_LOW;
        end
    endtask
    assign faulted=sync_fault;
    assign sync_event_valid=phase==SYNC_EVENT && !abort_run && !reset;
    assign sync_event_data=job_data;
    assign loop_ram_read_enable=phase==READ_REQUEST && operand==LOOP_MEMORY && !reset;
    assign loop_ram_read_address=loop_address;
    assign loop_ram_write_enable=phase==SYNC_WRITE && !abort_run && !reset;
    assign loop_ram_write_address=loop_address;
    assign loop_ram_write_data=word_value;

    function automatic [3:0] plan_word(input index, input [1:0] offset);
        plan_word=(index ? 4'd13 : 4'd10)+{2'b0,offset};
    endfunction
    task automatic loop_read(input [31:0] slot, input [3:0] word_number, input phase_t destination);
        begin
            loop_address<=LOOP_MEMORY_BITS'({slot,word_number}); memory_return<=destination;
            operand<=LOOP_MEMORY; phase<=READ_REQUEST;
        end
    endtask
    task automatic loop_write(input [3:0] word_number, input [63:0] value, input phase_t destination);
        begin
            loop_address<=LOOP_MEMORY_BITS'({target,word_number}); operand_data[63:0]<=value;
            memory_return<=destination; phase<=SYNC_WRITE;
        end
    endtask
    task automatic finish_error(input [7:0] code);
        begin
            error_code<=code; done<=1; busy<=0; phase<=IDLE;
            if (phase==IDLE ? sync_count!=0 : run_has_sync) sync_fault<=1;
        end
    endtask
    task automatic sync_fail;
        begin finish_error(`SHOOP_SP_INVALID_STATE); sync_fault<=1; end
    endtask
    task automatic sync_emit(input [7:0] kind, input [7:0] reason, input [7:0] outcome,
        input [63:0] payload, input [63:0] sequence_value, input phase_t destination);
        begin
            job_data<={run_frame,sequence_value,payload,target,8'b0,outcome,reason,kind};
            sync_event_return<=destination; phase<=SYNC_EVENT;
        end
    endtask
    task automatic sync_resolve(input index, input [7:0] outcome, input [7:0] reason, input phase_t destination);
        begin
            if (index) control[P1+VALID]<=0; else control[P0+VALID]<=0;
            sync_emit(3,reason,outcome,{63'b0,index},0,destination);
        end
    endtask

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            busy <= 0;
            operand <= ENABLE;
            pc <= 0;
            run_count <= 0;
            run_state_count <= 0;
            flags<=0; control<=0; capture_frames<=0; loop_address<=0; loop_word<=0;
            sync_remaining<=0; run_frame<=0; run_has_sync<=0; last_instruction<=0;
            memory_return<=IDLE; sync_event_return<=IDLE; sync_natural_return<=IDLE; operation_return<=IDLE; sync_pass<=0;
            scan_index<=0; winner<=0; winner_valid<=0; cancel_index<=0;
            pulse_pending<=0; delivered<=0; masked<=0; protected_valid<=0; protected_newer<=0;
            changed<=0; applied_pulse<=0; sync_fault<=0; sync_frame<=0; midi_restart<=0; emit_return<=NEXT;
            run_quantum <= 0; begin_capture <= 0; capture_job <= 0; chunk_shift <= 0;
            opcode <= 0;
            arg0 <= 0;
            arg1 <= 0;
            arg2 <= 0;
            arg3 <= 0;
            arg4 <= 0;
            operand_address <= 0;
            position <= 0;
            length <= 0;
            writeback <= 0;
            ramp_state <= 0;
            ramp_elapsed <= 0;
            operand_data <= 0; operand_nonzero_parts <= 0;
            shape_bad <= 0; program_bad <= 0; state_bad <= 0; empty_program <= 0;
            loop_invalid <= 0; loop_at_start <= 0; loop_mode <= 0;
            inline_sync<=0; source_valid<=0; source_self<=0; sync_flags_invalid<=0; sync_control_matches<=0; source_slot<=0;
            operation_length<=0; operation_empty<=0; operation_in_range<=0;
            position_in_range<=0; word_at_frame<=0; word_after_frame<=0; advanced_invalid<=0;
            job_data <= 0;
            done <= 0;
            error_code <= `SHOOP_SP_OK;
            arithmetic_left<=0; arithmetic_right<=0; arithmetic_low<=0; arithmetic_sum<=0; arithmetic_return<=IDLE;
        end else begin
            done <= 0;
            if (abort_run && busy) begin
                // Retire an accepted loop read before allowing another run.
                if (phase != LOOP_WAIT || loop_response_valid)
                    finish_error(`SHOOP_SP_INVALID_STATE);
            end else case (phase)
                IDLE: if (start) begin
                    busy <= 1;
                    pc <= 0; chunk_shift <= shift_for(chunk_frames);
                    error_code <= `SHOOP_SP_OK;
                    shape_bad <= chunk_frames == 0 || (chunk_frames & (chunk_frames-1)) != 0
                        || quantum == 0 || (quantum & (chunk_frames-1)) != 0;
                    program_bad <= instruction_count > 32'(PROGRAM_DEPTH) || sync_count>instruction_count || sync_count>32'(LOOP_DEPTH);
                    state_bad <= state_count > 32'(STATE_DEPTH);
                    empty_program <= instruction_count == 0;
                    run_count <= PC_BITS'(instruction_count);
                    run_state_count <= state_count;
                    run_quantum <= quantum; sync_remaining<=SYNC_COUNT_BITS'(sync_count); run_has_sync<=sync_count!=0;
                    run_frame <= sync_frame;
                    if (sync_count!=0 && !(quantum==1 || quantum==16 || quantum==32 || quantum==64 || quantum==128)) shape_bad<=1;
                    if (queued_jobs != 0) begin
                        finish_error(`SHOOP_SP_QUEUE_FULL);
                    end else add_words(sync_frame,{32'b0,quantum},ADMIT);
                end
                ADD_LOW: begin
                    arithmetic_low<={1'b0,arithmetic_left[31:0]}+{1'b0,arithmetic_right[31:0]};
                    phase<=ADD_HIGH;
                end
                ADD_HIGH: begin
                    arithmetic_sum<={arithmetic_high,arithmetic_low[31:0]};
                    phase<=arithmetic_return;
                end
                ADMIT: begin
                    if (shape_bad || arithmetic_sum[64]) begin
                        finish_error(`SHOOP_SP_INVALID_STATE);
                    end else if (program_bad) begin
                        finish_error(`SHOOP_SP_INVALID_PROGRAM);
                    end else if (state_bad) begin
                        finish_error(`SHOOP_SP_INVALID_STATE);
                    end else if (empty_program) begin
                        sync_frame <= arithmetic_sum[63:0];
                        done <= 1; busy <= 0; phase <= IDLE;
                    end else begin
                        if (!run_has_sync) sync_frame <= arithmetic_sum[63:0];
                        phase <= FETCH;
                    end
                end
                FETCH: if (program_error) begin
                    finish_error(`SHOOP_SP_PROGRAM_MEMORY_ERROR);
                end else if (program_available) begin
                    last_instruction <= (pc+1'b1)==run_count;
                    phase <= DECODE;
                end
                DECODE: begin
                    opcode <= program_read_data[7:0];
                    inline_sync <= program_read_data[7:0]==`SHOOP_SP_PROCESS_LOOP && program_read_data[72];
                    arg0 <= program_read_data[23:8];
                    arg1 <= program_read_data[39:24];
                    arg2 <= program_read_data[55:40];
                    arg3 <= program_read_data[71:56];
                    arg4 <= program_read_data[87:72];
                    instruction_reserved <= program_read_data[95:88];
                    phase <= DISPATCH;
                end
                DISPATCH: begin
                    writeback <= 0; emit_return<=NEXT;
                    if (last_instruction && sync_remaining != 0
                        && !(sync_remaining == 1 && opcode == `SHOOP_SP_PROCESS_LOOP)) begin
                        finish_error(`SHOOP_SP_INVALID_PROGRAM);
                    end else case (instruction[7:0])
                        `SHOOP_SP_PROCESS_LOOP: begin
                            if (sync_remaining == 0 || (instruction[95:56]!=0 && !(instruction[95:72]==24'd1))) begin finish_error(`SHOOP_SP_INVALID_PROGRAM); end
                            else phase<=SYNC_BEGIN;
                        end
                        `SHOOP_SP_MIDI_INPUT_INSTRUCTION: begin
                            job_data<=pack_job(`SHOOP_SP_MIDI_INPUT,arg0,arg1,0,0,0); phase<=EMIT;
                        end
                        `SHOOP_SP_MIDI_MERGE_INSTRUCTION: begin
                            job_data<=pack_job(`SHOOP_SP_MIDI_MERGE,arg0,arg1,{48'b0,arg2},0,0); phase<=EMIT;
                        end
                        `SHOOP_SP_MIDI_OUTPUT_INSTRUCTION: begin
                            job_data<=pack_job(`SHOOP_SP_MIDI_OUTPUT,arg0,arg1,0,0,0); phase<=EMIT;
                        end
                        `SHOOP_SP_MIDI_MERGE_OUTPUT_INSTRUCTION: begin
                            job_data<=pack_job(`SHOOP_SP_MIDI_MERGE_OUTPUT,arg0,{8'b0,instruction_reserved},{arg4,arg3,arg2,arg1},0,0);
                            phase<=EMIT;
                        end
                        `SHOOP_SP_MIDI_PLAYBACK_INSTRUCTION, `SHOOP_SP_MIDI_CAPTURE_INSTRUCTION,
                        `SHOOP_SP_MIDI_PREPARE_CAPTURE, `SHOOP_SP_MIDI_STATE_INSTRUCTION: begin
                            if (!run_has_sync) finish_error(`SHOOP_SP_INVALID_PROGRAM);
                            else phase<=SYNC_BEGIN;
                        end
                        `SHOOP_SP_LOGICAL_PLAYBACK, `SHOOP_SP_LOGICAL_CAPTURE, `SHOOP_SP_PROCESS_SMOOTHING, `SHOOP_SP_HISTORY_SMOOTHING: begin
                            if (run_has_sync) phase<=SYNC_BEGIN;
                            else phase<=LOOP_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_CONSTANT: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_CONSTANT,
                                instruction[55:40], 0, {32'b0,instruction[39:8]}, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_ARITHMETIC: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_ARITHMETIC,
                                instruction[23:8], instruction[39:24],
                                {32'b0,instruction[71:40]}, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_SILENCE: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_SILENCE,
                                instruction[23:8], 0, 0, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_INPUT: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_INPUT,
                                instruction[23:8], instruction[39:24], 0, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_PLAYBACK: begin
                            operand_address <= instruction[71:56];
                            operand <= ENABLE;
                            phase <= READ_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_GAIN: begin
                            operand_address <= instruction[23:8];
                            operand <= RAMP;
                            phase <= READ_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_MIX: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_MIX,
                                instruction[23:8], instruction[39:24],
                                {48'b0, instruction[55:40]}, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_CAPTURE: begin
                            operand_address <= instruction[39:24];
                            operand <= ENABLE;
                            phase <= READ_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_OUTPUT: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_OUTPUT,
                                instruction[23:8], instruction[39:24], 0, 0, 0);
                            phase <= EMIT;
                        end
                        default: begin
                            finish_error(`SHOOP_SP_INVALID_PROGRAM);
                        end
                    endcase
                end
                LOOP_REQUEST: if (loop_read_ready) phase <= LOOP_WAIT;
                LOOP_WAIT: if (loop_response_valid) begin
                    loop_invalid <= loop_response_error || !loop_response_data[0]
                        || loop_response_data[1+:32] != {arg2,arg1}
                        || loop_response_data[33+:2] == 3;
                    loop_mode <= loop_response_data[33+:2];
                    loop_at_start <= loop_response_data[99+:64] == 0;
                    begin_capture <= loop_response_data[163];
                    position <= loop_response_data[99+:64] >> chunk_shift;
                    length <= loop_response_data[35+:64] >> chunk_shift;
                    phase <= LOOP_RESULT;
                end
                LOOP_RESULT: begin
                    position_in_range <= position < length;
                    if (loop_invalid) begin
                        finish_error(`SHOOP_SP_INVALID_STATE);
                    end else if (midi_loop_instruction) phase<=MIDI_DISPATCH;
                    else if (opcode == `SHOOP_SP_PROCESS_SMOOTHING || opcode == `SHOOP_SP_HISTORY_SMOOTHING) begin
                        playback_flags <= {loop_at_start,loop_mode==1};
                        operand_address <= opcode==`SHOOP_SP_HISTORY_SMOOTHING ? arg1 : arg3; operand <= SMOOTHING; phase <= READ_REQUEST;
                    end else if (opcode == `SHOOP_SP_LOGICAL_PLAYBACK) begin
                        if (loop_mode == 1) begin
                            operand_address <= arg3; operand <= VIEW; phase <= READ_REQUEST;
                        end else begin
                            job_data <= pack_job(`SHOOP_SP_JOB_SILENCE, arg4, 0, 0, 0, 0);
                            phase <= EMIT;
                        end
                    end else if (loop_mode == 2) begin
                        operand_address <= arg3; operand <= VIEW; phase <= READ_REQUEST;
                    end else phase <= NEXT;
                end
                READ_REQUEST: begin
                    if (operand==LOOP_MEMORY) phase<=READ_CAPTURE;
                    else if ((opcode == `SHOOP_SP_PROCESS_PLAYBACK || opcode == `SHOOP_SP_LOGICAL_PLAYBACK) && operand == VIEW
                        && !position_in_range) begin
                        if (opcode == `SHOOP_SP_LOGICAL_PLAYBACK) begin
                            job_data <= pack_job(`SHOOP_SP_JOB_SILENCE, arg4, 0, 0, 0, 0);
                            phase <= EMIT;
                        end else begin
                            finish_error(`SHOOP_SP_INVALID_STATE);
                        end
                    end else if (!operand_in_range) begin
                        finish_error(`SHOOP_SP_INVALID_STATE);
                    end else begin
                        phase <= READ_CAPTURE;
                    end
                end
                READ_CAPTURE: begin
                    operand_data <= operand==LOOP_MEMORY ? {65'b0,loop_ram_read_data} : state_read_data;
                    operand_nonzero_parts <= {|read_word[63:32], |read_word[31:0]};
                    phase <= READ_RESULT;
                end
                READ_RESULT: begin
                    word_at_frame<=word_value==run_frame; word_after_frame<=word_value>run_frame;
                    if (operand==LOOP_MEMORY) begin
                        case (memory_return)
                            SYNC_WRAP, SYNC_NEW_REV: add_words(word_value,64'd1,memory_return);
                            SYNC_REMAINING: add_words(word_value,64'hffffffffffffffff,memory_return);
                            default: phase<=memory_return;
                        endcase
                    end else if (is_ramp != (operand == RAMP)) begin
                        finish_error(`SHOOP_SP_INVALID_STATE);
                    end else begin
                        case (operand)
                            ENABLE: begin
                                if (!word_nonzero) begin
                                    if (opcode == `SHOOP_SP_PROCESS_PLAYBACK) begin
                                        job_data <= pack_job(`SHOOP_SP_JOB_SILENCE, arg4, 0, 0, 0, 0);
                                        phase <= EMIT;
                                    end else begin
                                        phase <= NEXT;
                                    end
                                end else begin
                                    operand_address <= (opcode == `SHOOP_SP_PROCESS_PLAYBACK) ? arg1 : arg0;
                                    operand <= (opcode == `SHOOP_SP_PROCESS_PLAYBACK) ? POSITION : VIEW;
                                    phase <= READ_REQUEST;
                                end
                            end
                            POSITION: begin
                                position <= word_value;
                                operand_address <= arg2;
                                operand <= LENGTH;
                                phase <= READ_REQUEST;
                            end
                            LENGTH: begin
                                length <= word_value;
                                position_in_range <= position < word_value;
                                operand_address <= arg0;
                                operand <= VIEW;
                                phase <= READ_REQUEST;
                            end
                            VIEW: begin
                                if (opcode == `SHOOP_SP_PROCESS_PLAYBACK || opcode == `SHOOP_SP_LOGICAL_PLAYBACK)
                                    job_data <= pack_job(`SHOOP_SP_JOB_READ, arg4, 0, word_value, position, length);
                                else begin
                                    capture_job <= pack_job(`SHOOP_SP_JOB_CAPTURE,
                                        opcode == `SHOOP_SP_LOGICAL_CAPTURE ? arg4 : arg2, 0, word_value, 0, 0);
                                    if (opcode == `SHOOP_SP_LOGICAL_CAPTURE && begin_capture)
                                        job_data <= pack_job(`SHOOP_SP_JOB_BEGIN_CAPTURE, 0, 0, word_value, 0, 0);
                                    else job_data <= pack_job(`SHOOP_SP_JOB_CAPTURE,
                                        opcode == `SHOOP_SP_LOGICAL_CAPTURE ? arg4 : arg2, 0, word_value, 0, 0);
                                end
                                phase <= EMIT;
                            end
                            SMOOTHING: begin
                                job_data <= pack_job(`SHOOP_SP_JOB_SMOOTHING,
                                    opcode==`SHOOP_SP_HISTORY_SMOOTHING ? arg2 : arg4,
                                    opcode==`SHOOP_SP_HISTORY_SMOOTHING ? arg3 : operand_data[15:0],
                                    {operand_data[63:32],16'b0,opcode==`SHOOP_SP_HISTORY_SMOOTHING ? arg4 : operand_data[31:16]}, {62'b0,playback_flags},0);
                                phase <= EMIT;
                            end
                            RAMP: begin
                                job_data <= pack_job(`SHOOP_SP_JOB_GAIN, arg1, arg2,
                                    operand_data[63:0], operand_data[127:64], 0);
                                ramp_state <= operand_data[95:0];
                                ramp_elapsed <= {1'b0,operand_data[127:96]}+{1'b0,run_quantum};
                                writeback <= 1;
                                phase <= EMIT;
                            end
                            default: begin
                                finish_error(`SHOOP_SP_INVALID_STATE);
                            end
                        endcase
                    end
                end
                EMIT: begin
                    if (job_ready) begin
                        if (job_data[7:0] == `SHOOP_SP_JOB_BEGIN_CAPTURE) begin
                            job_data <= capture_job; begin_capture <= 0;
                        end else phase <= emit_return;
                    end else if (STREAM_JOBS == 0)
                        finish_error(`SHOOP_SP_QUEUE_FULL);
                end
                SYNC_BEGIN: begin
                    if (target>=32'(LOOP_DEPTH)) sync_fail();
                    else if (opcode==`SHOOP_SP_PROCESS_LOOP) begin loop_word<=0; loop_read(target,FLAGS,SYNC_LOAD); end
                    else loop_read(target,FLAGS,OWN_FLAGS);
                end
                SYNC_LOAD: begin
                    case (loop_word)
                        FLAGS: flags<=word_value;
                        CONTROL: begin
                            control<=word_value;
                            sync_flags_invalid<=!flags[V] || flags[G+:32]!=generation || flags[M+:2]==3;
                        end
                        POS: begin position<=word_value; sync_control_matches<=control[31:0]==generation; end
                        LEN: length<=word_value;
                        CAP: capture_frames<=word_value;
                        default: begin end
                    endcase
                    if (loop_word==CAP) begin
                        source_valid<=selected_source_valid; source_slot<=selected_source_slot;
                        source_self<=selected_source_valid && selected_source_slot==target;
                        loop_read(target,PF,SYNC_CHECK);
                    end
                    else begin loop_word<=loop_word+1'b1; loop_read(target,loop_word+1'b1,SYNC_LOAD); end
                end
                SYNC_CHECK: begin
                    pulse_pending<=0; delivered<=0; masked<=0; protected_valid<=0; flags[PULSE]<=0; flags[`SHOOP_SP_L_MIDI_RESTART]<=0;
                    if (sync_flags_invalid || (flags[PV] && word_at_frame)) sync_fail();
                    else if (source_valid) begin
                        if (source_slot>=32'(LOOP_DEPTH)) sync_fail();
                        else if (source_self) begin
                            if (!inline_sync && !sync_control_matches) sync_fail(); else phase<=SYNC_SCAN_BEGIN;
                        end else loop_read(source_slot,FLAGS,SYNC_SOURCE_FLAGS);
                    end else phase<=SYNC_SCAN_BEGIN;
                end
                SYNC_SOURCE_FLAGS: begin capture_job[63:0]<=word_value; loop_read(source_slot,PF,SYNC_SOURCE_CHECK); end
                SYNC_SOURCE_CHECK: begin
                    if (!source_flags[V] || (!inline_sync && source_flags[G+:32]!=control[31:0]) || source_flags[M+:2]==3
                        || !source_flags[PV] || !word_at_frame) sync_fail();
                    else phase<=SYNC_SCAN_BEGIN;
                end
                SYNC_SCAN_BEGIN: begin sync_pass<=0; scan_index<=0; winner_valid<=0; phase<=SYNC_SCAN; end
                SYNC_SCAN: begin
                    if (!scan_flags[VALID] || scan_flags[TICKS]!=(sync_pass!=0)) phase<=SYNC_SCAN_NEXT;
                    else loop_read(target,plan_word(scan_index,0),SYNC_EFFECTIVE);
                end
                SYNC_EFFECTIVE: begin
                    if (word_after_frame) phase<=SYNC_SCAN_NEXT;
                    else if (sync_pass!=0) loop_read(target,plan_word(scan_index,1),SYNC_REMAINING);
                    else phase<=SYNC_CANDIDATE;
                end
                SYNC_REMAINING: begin
                    if (word_nonzero) loop_write(plan_word(scan_index,1),arithmetic_sum[63:0],SYNC_SCAN_NEXT);
                    else phase<=SYNC_CANDIDATE;
                end
                SYNC_CANDIDATE: begin
                    if (!winner_valid) begin winner<=scan_index; winner_valid<=1; phase<=SYNC_SCAN_NEXT; end
                    else if (scan_index==flags[NEWER]) begin winner<=scan_index; sync_resolve(winner,1,0,SYNC_SCAN_NEXT); end
                    else sync_resolve(scan_index,1,0,SYNC_SCAN_NEXT);
                end
                SYNC_SCAN_NEXT: begin
                    if (scan_index) phase<=SYNC_APPLY; else begin scan_index<=1; phase<=SYNC_SCAN; end
                end
                SYNC_APPLY: begin
                    if (!winner_valid) phase<=SYNC_GROUP_DONE;
                    else if (protected_valid && (protected_newer || !winner_newer)) sync_resolve(winner,1,0,SYNC_GROUP_DONE);
                    else begin
                        protected_valid<=1; protected_newer<=winner_newer;
                        operation_length<=playable_length; operation_empty<=playable_length==0;
                        loop_read(target,plan_word(winner,2),SYNC_OP_BEGIN);
                    end
                end
                SYNC_OP_BEGIN: begin
                    ramp_state[66:0]<={winner_flags[RET],winner_flags[OP+:2],word_value};
                    operation_in_range<=word_value<operation_length;
                    phase<=SYNC_OP_CHECK;
                end
                SYNC_OP_CHECK: begin
                    applied_pulse<=0;
                    case (operation_code)
                        0: begin
                            changed<=flags[M+:2]!=0 || position!=0;
                            flags[M+:2]<=0; position<=0;
                            if (flags[M+:2]==2) begin
                                length<=capture_frames; operation_return<=SYNC_SUCCESS; loop_read(target,WM,SYNC_RECORD_REV);
                            end else phase<=SYNC_SUCCESS;
                        end
                        1: begin
                            if (operation_empty) sync_resolve(winner,3,EMPTY_CONTENT,SYNC_GROUP_DONE);
                            else if (!preserve && !operation_in_range) sync_resolve(winner,3,INVALID_FRAME,SYNC_GROUP_DONE);
                            else begin
                                changed<=flags[M+:2]!=1 || (!preserve && position!=operation_position);
                                applied_pulse<=!preserve; flags[M+:2]<=1;
                                if (!preserve) position<=operation_position;
                                if (flags[M+:2]==2) begin
                                    length<=capture_frames; operation_return<=SYNC_RESET_BEGIN; loop_read(target,WM,SYNC_RECORD_REV);
                                end else phase<=preserve ? SYNC_SUCCESS : SYNC_RESET_BEGIN;
                            end
                        end
                        2: loop_read(target,WM,SYNC_NEW_REV);
                        3: begin
                            if (flags[M+:2]!=1) sync_resolve(winner,3,INVALID_STATE,SYNC_GROUP_DONE);
                            else if (!operation_in_range) sync_resolve(winner,3,INVALID_FRAME,SYNC_GROUP_DONE);
                            else begin
                                changed<=position!=operation_position; position<=operation_position; applied_pulse<=retrigger;
                                phase<=retrigger ? SYNC_RESET_BEGIN : SYNC_SUCCESS;
                            end
                        end
                    endcase
                end
                SYNC_RECORD_REV: loop_write(CR,word_value,operation_return);
                SYNC_NEW_REV: begin
                    if (arithmetic_sum[64]) sync_resolve(winner,3,EXHAUSTED,SYNC_GROUP_DONE);
                    else begin
                        changed<=1; applied_pulse<=1; flags[M+:2]<=2; capture_frames<=0;
                        loop_write(WM,arithmetic_sum[63:0],SYNC_SUCCESS);
                    end
                end
                SYNC_RESET_BEGIN: loop_read(target,CYC,SYNC_RESET_CYCLES);
                SYNC_RESET_CYCLES: begin
                    changed<=changed || word_nonzero; loop_write(CYC,0,SYNC_SUCCESS);
                end
                SYNC_SUCCESS: sync_resolve(winner,0,0,SYNC_AFTER_APPLY);
                SYNC_AFTER_APPLY: begin
                    if (changed) sync_emit(0,0,0,operation_position,{61'b0,ramp_state[66:64]},SYNC_EFFECTS);
                    else phase<=SYNC_EFFECTS;
                end
                SYNC_EFFECTS: begin
                    if (operation_code==3 && changed) flags[`SHOOP_SP_L_MIDI_RESTART]<=1;
                    if (operation_code==0) begin masked<=1; pulse_pending<=0; cancel_index<=0; phase<=SYNC_CANCEL; end
                    else begin
                        if (applied_pulse) begin masked<=0; if (!delivered) pulse_pending<=1; end
                        phase<=SYNC_GROUP_DONE;
                    end
                end
                SYNC_CANCEL: begin
                    if (cancel_flags[VALID] && protected_newer && cancel_index!=flags[NEWER]) sync_resolve(cancel_index,2,0,SYNC_CANCEL_NEXT);
                    else phase<=SYNC_CANCEL_NEXT;
                end
                SYNC_CANCEL_NEXT: begin
                    if (cancel_index) phase<=SYNC_GROUP_DONE; else begin cancel_index<=1; phase<=SYNC_CANCEL; end
                end
                SYNC_GROUP_DONE: begin
                    sync_natural_return<=sync_pass==0 ? SYNC_SOURCE : sync_pass==1 ? SYNC_DELIVER : SYNC_RENDER;
                    phase<=SYNC_NATURAL;
                end
                SYNC_NATURAL: begin
                    if (flags[M+:2]==1 && length!=0 && position==length
                        && !(flags[REP] && source_valid && !source_self && source_playing && !source_tick))
                        loop_read(target,CYC,SYNC_WRAP);
                    else phase<=sync_natural_return;
                end
                SYNC_WRAP: begin
                    if (arithmetic_sum[64]) sync_fail();
                    else begin
                        position<=0; if (!delivered) pulse_pending<=1;
                        loop_write(CYC,arithmetic_sum[63:0],SYNC_WRAP_EVENT);
                    end
                end
                SYNC_WRAP_EVENT: sync_emit(1,0,0,0,0,sync_natural_return);
                SYNC_SOURCE: begin
                    if (source_tick) begin
                        if (!masked && !delivered) pulse_pending<=1;
                        sync_pass<=1; scan_index<=0; winner_valid<=0; phase<=SYNC_SCAN;
                    end else phase<=SYNC_DELIVER;
                end
                SYNC_DELIVER: begin
                    if (pulse_pending) begin
                        flags[PULSE]<=1; delivered<=1; pulse_pending<=0;
                        sync_emit(2,0,0,0,0,SYNC_SELF);
                    end else phase<=SYNC_RENDER;
                end
                SYNC_SELF: begin
                    if (source_self) begin sync_pass<=2; scan_index<=0; winner_valid<=0; phase<=SYNC_SCAN; end
                    else phase<=SYNC_RENDER;
                end
                SYNC_RENDER: begin
                    position_in_range <= position < length;
                    flags[BC]<=flags[M+:2]==2 && capture_frames==0; flags[PV]<=1;
                    loop_write(RP,position,SYNC_STAMP);
                end
                SYNC_STAMP: loop_write(PF,run_frame,SYNC_ADVANCE);
                SYNC_ADVANCE: begin
                    if (flags[M+:2]==1 && position_in_range) add_words(position,{32'b0,run_quantum},SYNC_ADV_POSITION);
                    else if (flags[M+:2]==2) add_words(capture_frames,{32'b0,run_quantum},SYNC_ADV_CAPTURE);
                    else begin loop_word<=0; phase<=SYNC_STORE; end
                end
                SYNC_ADV_POSITION: begin
                    advanced_invalid <= arithmetic_sum[64] || arithmetic_sum[63:0]>length;
                    phase <= SYNC_ADV_COMMIT;
                end
                SYNC_ADV_COMMIT: begin
                    if (advanced_invalid) sync_fail();
                    else begin position<=arithmetic_sum[63:0]; loop_word<=0; phase<=SYNC_STORE; end
                end
                SYNC_ADV_CAPTURE: begin
                    if (arithmetic_sum[64]) sync_fail();
                    else begin capture_frames<=arithmetic_sum[63:0]; loop_word<=0; phase<=SYNC_STORE; end
                end
                SYNC_STORE: begin
                    loop_word<=loop_word+1'b1;
                    case (loop_word)
                        FLAGS: loop_write(FLAGS,flags,SYNC_STORE);
                        CONTROL: loop_write(CONTROL,control,SYNC_STORE);
                        POS: loop_write(POS,position,SYNC_STORE);
                        LEN: loop_write(LEN,length,SYNC_STORE);
                        CAP: loop_write(CAP,capture_frames,SYNC_FINISH);
                        default: sync_fail();
                    endcase
                end
                SYNC_FINISH: begin
                    sync_remaining <= sync_remaining - 1'b1;
                    if (sync_remaining == 1) add_words(run_frame,{32'b0,run_quantum},FRAME_FINISH);
                    else phase<=NEXT;
                end
                FRAME_FINISH: begin sync_frame<=arithmetic_sum[63:0]; phase<=NEXT; end
                SYNC_WRITE: phase<=memory_return;
                SYNC_EVENT: if (sync_event_ready) phase<=sync_event_return;
                OWN_FLAGS: begin
                    loop_invalid<=!word_value[V] || (opcode!=`SHOOP_SP_HISTORY_SMOOTHING && word_value[G+:32]!=generation) || word_value[M+:2]==3 || !word_value[PV];
                    flags<=word_value;
                    midi_restart<=word_value[PULSE] || word_value[`SHOOP_SP_L_MIDI_RESTART];
                    loop_mode<=word_value[M+:2]; begin_capture<=word_value[BC];
                    loop_read(target,PF,OWN_STAMP);
                end
                OWN_STAMP: begin
                    loop_invalid<=loop_invalid || !word_at_frame;
                    loop_read(target,RP,OWN_RENDER);
                end
                OWN_RENDER: begin
                    position<=midi_loop_instruction ? word_value : word_value>>chunk_shift; loop_at_start<=!word_nonzero;
                    loop_read(target,LEN,OWN_LENGTH);
                end
                OWN_LENGTH: begin length<=midi_loop_instruction ? word_value : word_value>>chunk_shift; phase<=LOOP_RESULT; end
                MIDI_DISPATCH: begin
                    if (opcode==`SHOOP_SP_MIDI_PLAYBACK_INSTRUCTION) begin
                        if (loop_mode==1 && position<length)
                            job_data<=pack_job(`SHOOP_SP_MIDI_READ,{8'b0,arg3[7:0]},
                                {8'b0,instruction_reserved},position,
                                {7'b0,(midi_restart || position==0),arg4,arg3[15:8],run_quantum},
                                {arg2,arg1,16'b0,arg0});
                        else job_data<=pack_job(`SHOOP_SP_MIDI_EMPTY,{8'b0,instruction_reserved},0,0,0,0);
                        phase<=EMIT;
                    end else if(opcode==`SHOOP_SP_MIDI_STATE_INSTRUCTION) begin
                        if(loop_mode==1 && position<length) begin
                            if(!flags[`SHOOP_SP_L_MIDI_PLAYING] || midi_restart || position==0) begin
                                job_data<=pack_job(`SHOOP_SP_MIDI_RESTORE,{8'b0,arg3[7:0]},0,position,{arg2,arg1,16'b0,arg0},0);
                                loop_write(FLAGS,flags|(64'b1<<`SHOOP_SP_L_MIDI_PLAYING),EMIT);
                            end else phase<=NEXT;
                        end else if(flags[`SHOOP_SP_L_MIDI_PLAYING]) begin
                            job_data<=pack_job(`SHOOP_SP_MIDI_CLEANUP,{8'b0,arg3[7:0]},0,0,{arg2,arg1,16'b0,arg0},0);
                            loop_write(FLAGS,flags & ~(64'b1<<`SHOOP_SP_L_MIDI_PLAYING),EMIT);
                        end else phase<=NEXT;
                    end else if (opcode==`SHOOP_SP_MIDI_PREPARE_CAPTURE) begin
                        if (flags[`SHOOP_SP_L_MIDI_CAPTURE_OPEN] && (loop_mode!=2 || begin_capture)) begin
                            flags[`SHOOP_SP_L_MIDI_CAPTURE_OPEN]<=0;
                            loop_write(FLAGS,flags & ~(64'b1<<`SHOOP_SP_L_MIDI_CAPTURE_OPEN),MIDI_CAPTURE_FLAGS);
                        end else phase<=MIDI_CAPTURE_BEGIN;
                    end else if (loop_mode==2) loop_read(target,CAP,MIDI_CAPTURE_POSITION);
                    else phase<=NEXT;
                end
                MIDI_CAPTURE_FLAGS: begin
                    job_data<=pack_job(`SHOOP_SP_MIDI_FINISH_CAPTURE,arg3,0,0,0,0);
                    emit_return<=MIDI_CAPTURE_BEGIN;phase<=EMIT;
                end
                MIDI_CAPTURE_BEGIN: begin
                    if (loop_mode==2 && !flags[`SHOOP_SP_L_MIDI_CAPTURE_OPEN]) begin
                        flags[`SHOOP_SP_L_MIDI_CAPTURE_OPEN]<=1;
                        loop_write(FLAGS,flags | (64'b1<<`SHOOP_SP_L_MIDI_CAPTURE_OPEN),MIDI_CAPTURE_APPEND);
                    end else phase<=NEXT;
                end
                MIDI_CAPTURE_APPEND: begin
                    job_data<=pack_job(`SHOOP_SP_MIDI_BEGIN_CAPTURE,arg3,0,0,0,0);emit_return<=NEXT;phase<=EMIT;
                end
                MIDI_CAPTURE_POSITION: begin
                    job_data<=pack_job(`SHOOP_SP_MIDI_APPEND,arg3,arg4,word_value-{32'b0,run_quantum},{32'b0,run_quantum},0);
                    phase<=EMIT;
                end
                NEXT: begin
                    pc <= pc + 1'b1;
                    if (last_instruction) begin
                        done <= 1;
                        busy <= 0;
                        phase <= IDLE;
                    end else begin
                        phase <= FETCH;
                    end
                end
                default: begin
                    finish_error(`SHOOP_SP_INVALID_PROGRAM);
                end
            endcase
        end
    end
endmodule
