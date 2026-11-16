`include "state_processor_defs.svh"
`include "sp_loop_defs.svh"

module sp_engine #(
    parameter integer PROGRAM_DEPTH = 256,
    parameter integer STATE_DEPTH = 256,
    parameter integer LOOP_DEPTH = STATE_DEPTH,
    parameter integer LOOP_MEMORY_BITS = $clog2(LOOP_DEPTH*16),
    parameter integer PROGRAM_ADDR_BITS = (PROGRAM_DEPTH > 1) ? $clog2(PROGRAM_DEPTH) : 1,
    parameter integer STATE_ADDR_BITS = (STATE_DEPTH > 1) ? $clog2(STATE_DEPTH) : 1,
    parameter integer PC_BITS = $clog2(PROGRAM_DEPTH + 1)
) (
    input  wire                                clk,
    input  wire                                reset,
    input  wire                                start,
    input  wire [31:0]                         instruction_count,
    input wire [31:0] sync_count,
    input  wire [31:0]                         state_count,
    input  wire [31:0]                         quantum,
    input wire [31:0] chunk_frames,
    input  wire [31:0]                         job_budget,
    input  wire [31:0]                         queued_jobs,
    input  wire [31:0]                         queue_remaining,
    output wire                                busy,
    output reg                                 done,
    output reg  [7:0]                          error_code,
    output wire [31:0]                         instruction_pc,
    output wire                                program_read_enable,
    output wire [PROGRAM_ADDR_BITS-1:0]         program_read_address,
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
    input wire [127:0] loop_ram_read_data,
    output wire loop_ram_write_enable,
    output wire [LOOP_MEMORY_BITS-1:0] loop_ram_write_address,
    output reg [127:0] loop_ram_write_data,
    output wire sync_event_valid,
    input wire sync_event_ready,
    output wire [255:0] sync_event_data,
    output reg [63:0] sync_frame,
    output wire faulted
);
    typedef enum logic [5:0] {
        IDLE, ADMIT, FETCH, DECODE, READ_REQUEST, READ_CAPTURE, READ_RESULT, EMIT, NEXT,
        LOOP_REQUEST, LOOP_WAIT, LOOP_RESULT,
        SYNC_BEGIN, SYNC_CHECK, SYNC_SOURCE_CHECK, SYNC_SCAN_BEGIN, SYNC_SCAN, SYNC_SCAN_NEXT,
        SYNC_APPLY, SYNC_APPLY_OP, SYNC_AFTER_APPLY, SYNC_EFFECTS, SYNC_CANCEL, SYNC_CANCEL_NEXT,
        SYNC_GROUP_DONE, SYNC_NATURAL, SYNC_SOURCE, SYNC_DELIVER, SYNC_SELF, SYNC_RENDER,
        SYNC_ADV_POSITION, SYNC_ADV_CAPTURE, SYNC_WRITE_BEGIN, SYNC_WRITE, SYNC_EVENT
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
    reg [31:0] run_sync_count;
    reg begin_capture;
    reg [1:0] playback_flags;
    reg [255:0] capture_job;
    reg [7:0] opcode;
    reg [15:0] arg0, arg1, arg2, arg3, arg4;
    reg [15:0] operand_address;
    reg [63:0] position;
    reg [63:0] length;
    reg writeback;
    reg [95:0] ramp_state;
    reg [32:0] ramp_elapsed;
    reg [`SHOOP_SP_STATE_BITS-1:0] operand_data;
    reg queue_bad, shape_bad, program_bad, state_bad, empty_program;
    reg loop_invalid, loop_at_start;
    reg [1:0] loop_mode;
    wire [63:0] word_value = operand_data[63:0];
    wire is_ramp = operand_data[128];
    wire unused_reserved = &{1'b0, program_read_data[95:88]};
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

    assign busy = phase != IDLE;
    assign instruction_pc = 32'(pc);
    assign program_read_enable = (phase == FETCH) && !reset;
    assign program_read_address = PROGRAM_ADDR_BITS'(pc);
    assign state_read_enable = (phase == READ_REQUEST) && operand != LOOP_MEMORY && operand_in_range && !reset;
    assign state_read_address = STATE_ADDR_BITS'(operand_address);
    assign job_valid = (phase == EMIT) && (queue_remaining != 0) && !reset;
    assign state_write_enable = job_valid && job_ready && writeback;
    assign state_write_address = STATE_ADDR_BITS'(arg0);
    assign state_write_data = {1'b1,
        ramp_elapsed >= {1'b0, ramp_state[95:64]} ? ramp_state[95:64] : ramp_elapsed[31:0],
        ramp_state};
    assign loop_read_valid = phase == LOOP_REQUEST && !reset;
    assign loop_read_slot = {16'b0, arg0};

    localparam integer W=`SHOOP_SP_LOOP_BITS;
    localparam integer V=`SHOOP_SP_L_VALID, G=`SHOOP_SP_L_GEN, M=`SHOOP_SP_L_MODE,
        REP=`SHOOP_SP_L_REPEAT, SV=`SHOOP_SP_L_SOURCE_VALID, SS=`SHOOP_SP_L_SOURCE,
        SG=`SHOOP_SP_L_SOURCE_GEN, PULSE=`SHOOP_SP_L_PULSE, LV=`SHOOP_SP_L_LAST_VALID,
        BC=`SHOOP_SP_L_BEGIN_CAPTURE, PV=`SHOOP_SP_L_PROCESSED_VALID,
        POS=`SHOOP_SP_L_POSITION, LEN=`SHOOP_SP_L_LENGTH, CAP=`SHOOP_SP_L_CAPTURE,
        CYC=`SHOOP_SP_L_CYCLES, CR=`SHOOP_SP_L_CONTENT_REV, AR=`SHOOP_SP_L_CAPTURE_REV,
        WM=`SHOOP_SP_L_WATERMARK, LP=`SHOOP_SP_L_LAST_PULSE,
        RP=`SHOOP_SP_L_RENDER_POSITION, RL=`SHOOP_SP_L_RENDER_LENGTH,
        PF=`SHOOP_SP_L_PROCESSED_FRAME;
    localparam integer PR=`SHOOP_SP_P_REQUEST, SEQ=`SHOOP_SP_P_SEQUENCE,
        EFF=`SHOOP_SP_P_EFFECTIVE, REM=`SHOOP_SP_P_REMAINING,
        PP=`SHOOP_SP_P_POSITION, VALID=`SHOOP_SP_P_VALID, TICKS=`SHOOP_SP_P_TICKS,
        OP=`SHOOP_SP_P_OP, RET=`SHOOP_SP_P_RETRIGGER;
    localparam [7:0] INVALID_STATE=5, INVALID_FRAME=6, EXHAUSTED=8, EMPTY_CONTENT=9;

    typedef enum logic [2:0] { LOOP_ROW, SOURCE_FLAGS, SOURCE_STAMP, OWN_FLAGS, OWN_STAMP, OWN_RENDER } loop_operand_t;
    loop_operand_t loop_operand;
    phase_t sync_event_return, sync_natural_return;
    reg [W-1:0] row;
    reg [LOOP_MEMORY_BITS-1:0] loop_address;
    reg [3:0] loop_word;
    reg [1:0] sync_pass;
    reg scan_index, winner, winner_valid, cancel_index;
    reg pulse_pending, delivered, masked, protected_valid, changed, applied_pulse, sync_fault;
    wire [31:0] target={16'b0,arg0};
    wire [31:0] generation={arg2,arg1};
    wire [127:0] source_flags=capture_job[127:0];
    wire [63:0] protected_sequence=operand_data[63:0];
    wire [31:0] source_slot=row[SS+:32];
    wire source_self=row[SV] && source_slot==target;
    wire source_tick=row[SV] && !source_self && source_flags[PULSE];
    wire source_playing=source_flags[M+:2]==1;
    wire [383:0] scan_plan=scan_index ? row[1152+:384] : row[768+:384];
    wire [383:0] winner_plan=winner ? row[1152+:384] : row[768+:384];
    wire [383:0] cancel_plan=cancel_index ? row[1152+:384] : row[768+:384];
    reg [63:0] arithmetic_left, arithmetic_right;
    wire [64:0] arithmetic_sum={1'b0,arithmetic_left}+{1'b0,arithmetic_right};
    always @* begin
        arithmetic_left=position; arithmetic_right={32'b0,run_quantum};
        case (phase)
            ADMIT, SYNC_WRITE: arithmetic_left=sync_frame;
            SYNC_ADV_CAPTURE: arithmetic_left=row[CAP+:64];
            SYNC_NATURAL: begin arithmetic_left=row[CYC+:64]; arithmetic_right=1; end
            SYNC_APPLY_OP: begin arithmetic_left=row[WM+:64]; arithmetic_right=1; end
            SYNC_SCAN: begin arithmetic_left=scan_plan[REM+:64]; arithmetic_right=64'hffffffffffffffff; end
            READ_RESULT: begin
                if (operand==LOOP_MEMORY) arithmetic_left=operand_data[63:0];
                else arithmetic_left={32'b0,operand_data[127:96]};
            end
            default: begin end
        endcase
    end
    assign faulted=sync_fault || (done && error_code!=0 && run_sync_count!=0);
    assign sync_event_valid=phase==SYNC_EVENT && !reset;
    assign sync_event_data=job_data;
    assign loop_ram_read_enable=phase==READ_REQUEST && operand==LOOP_MEMORY && !reset;
    assign loop_ram_read_address=loop_address;
    assign loop_ram_write_enable=phase==SYNC_WRITE && !reset;
    assign loop_ram_write_address=LOOP_MEMORY_BITS'({target,loop_word});
    always @* begin
        loop_ram_write_data=0;
        case (loop_word)
            4'd0: loop_ram_write_data=row[0+:128];
            4'd1: loop_ram_write_data={length,position};
            4'd2: loop_ram_write_data=row[256+:128];
            4'd3: loop_ram_write_data=row[384+:128];
            4'd4: loop_ram_write_data=row[512+:128];
            4'd5: loop_ram_write_data=row[640+:128];
            4'd6: loop_ram_write_data=row[768+:128];
            4'd7: loop_ram_write_data=row[896+:128];
            4'd8: loop_ram_write_data=row[1024+:128];
            4'd9: loop_ram_write_data=row[1152+:128];
            4'd10: loop_ram_write_data=row[1280+:128];
            4'd11: loop_ram_write_data=row[1408+:128];
            4'd12: loop_ram_write_data=row[1536+:128];
            default: begin end
        endcase
    end
    task automatic loop_read(input [31:0] slot, input [3:0] word_number, input loop_operand_t purpose);
        begin
            loop_address<=LOOP_MEMORY_BITS'({slot,word_number}); loop_operand<=purpose;
            operand<=LOOP_MEMORY; phase<=READ_REQUEST;
        end
    endtask
    task automatic sync_fail;
        begin error_code<=`SHOOP_SP_INVALID_STATE; sync_fault<=1; done<=1; phase<=IDLE; end
    endtask
    task automatic sync_emit(input [7:0] kind, input [7:0] reason, input [7:0] outcome,
        input [63:0] payload, input [63:0] sequence_value, input phase_t destination);
        begin
            job_data<={sync_frame,sequence_value,payload,target,8'b0,outcome,reason,kind};
            sync_event_return<=destination; phase<=SYNC_EVENT;
        end
    endtask
    task automatic sync_resolve(input index, input [7:0] outcome, input [7:0] reason, input phase_t destination);
        reg [383:0] unused_plan;
        begin
            unused_plan=index ? row[1152+:384] : row[768+:384];
            if (index) row[1152+VALID]<=0; else row[768+VALID]<=0;
            sync_emit(3,reason,outcome,unused_plan[PR+:64],unused_plan[SEQ+:64],destination);
        end
    endtask
    function automatic [W+8:0] operation(input [W-1:0] before_row, input [383:0] unused_plan, input [63:0] record_revision);
        reg [W-1:0] value;
        reg [7:0] error;
        reg pulse, preserve;
        begin
            value=before_row; error=0; pulse=0;
            preserve=before_row[M+:2]==1 && !unused_plan[RET];
            case (unused_plan[OP+:2])
                0: begin
                    if (before_row[M+:2]==2) begin value[LEN+:64]=before_row[CAP+:64]; value[CR+:64]=before_row[AR+:64]; end
                    value[M+:2]=0; value[POS+:64]=0;
                end
                1: begin
                    if (before_row[M+:2]==2) begin value[LEN+:64]=before_row[CAP+:64]; value[CR+:64]=before_row[AR+:64]; end
                    if (value[LEN+:64]==0) error=EMPTY_CONTENT;
                    else if (!preserve && unused_plan[PP+:64]>=value[LEN+:64]) error=INVALID_FRAME;
                    value[M+:2]=1;
                    if (!preserve) begin value[POS+:64]=unused_plan[PP+:64]; value[CYC+:64]=0; end
                    pulse=!preserve;
                end
                2: begin
                    if (&before_row[WM+:64]) error=EXHAUSTED;
                    value[AR+:64]=record_revision; value[WM+:64]=value[AR+:64];
                    value[M+:2]=2; value[CAP+:64]=0; pulse=1;
                end
                3: begin
                    if (before_row[M+:2]!=1) error=INVALID_STATE;
                    else if (unused_plan[PP+:64]>=before_row[LEN+:64]) error=INVALID_FRAME;
                    value[POS+:64]=unused_plan[PP+:64];
                    if (unused_plan[RET]) value[CYC+:64]=0;
                    pulse=unused_plan[RET];
                end
            endcase
            operation={error,pulse,value};
        end
    endfunction
    reg [W-1:0] operation_input;
    always @* begin
        operation_input=row;
        operation_input[POS+:64]=position; operation_input[LEN+:64]=length;
    end
    wire [W+8:0] op_result=operation(operation_input,winner_plan,arithmetic_sum[63:0]);
    wire op_changed=operation_input[M+:2]!=op_result[M+:2] || operation_input[POS+:512]!=op_result[POS+:512];

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            operand <= ENABLE;
            pc <= 0;
            run_count <= 0;
            run_state_count <= 0;
            row<=0; loop_address<=0; loop_word<=0; loop_operand<=LOOP_ROW; run_sync_count<=0;
            sync_event_return<=IDLE; sync_natural_return<=IDLE; sync_pass<=0;
            scan_index<=0; winner<=0; winner_valid<=0; cancel_index<=0;
            pulse_pending<=0; delivered<=0; masked<=0; protected_valid<=0;
            changed<=0; applied_pulse<=0; sync_fault<=0; sync_frame<=0;
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
            operand_data <= 0;
            queue_bad <= 0; shape_bad <= 0; program_bad <= 0; state_bad <= 0; empty_program <= 0;
            loop_invalid <= 0; loop_at_start <= 0; loop_mode <= 0;
            job_data <= 0;
            done <= 0;
            error_code <= `SHOOP_SP_OK;
        end else begin
            done <= 0;
            if (done && error_code!=0 && run_sync_count!=0) sync_fault<=1;
            case (phase)
                IDLE: if (start) begin
                    pc <= 0; chunk_shift <= shift_for(chunk_frames);
                    error_code <= `SHOOP_SP_OK;
                    queue_bad <= (queued_jobs != 0) || (queue_remaining < job_budget);
                    shape_bad <= chunk_frames == 0 || (chunk_frames & (chunk_frames-1)) != 0
                        || quantum == 0 || (quantum & (chunk_frames-1)) != 0;
                    program_bad <= instruction_count > 32'(PROGRAM_DEPTH) || sync_count>instruction_count || sync_count>32'(LOOP_DEPTH);
                    state_bad <= state_count > 32'(STATE_DEPTH);
                    empty_program <= instruction_count == 0;
                    run_count <= PC_BITS'(instruction_count);
                    run_state_count <= state_count;
                    run_quantum <= quantum; run_sync_count<=sync_count;
                    if (sync_count!=0 && !(quantum==1 || quantum==16 || quantum==32 || quantum==64 || quantum==128)) shape_bad<=1;
                    if (queued_jobs != 0) begin
                        error_code <= `SHOOP_SP_QUEUE_FULL;
                        done <= 1;
                    end else phase <= ADMIT;
                end
                ADMIT: begin
                    if (queue_bad) begin
                        error_code <= `SHOOP_SP_QUEUE_FULL;
                        done <= 1; phase <= IDLE;
                    end else if (shape_bad || (run_sync_count!=0 && arithmetic_sum[64])) begin
                        error_code <= `SHOOP_SP_INVALID_STATE; done <= 1; phase <= IDLE;
                    end else if (program_bad) begin
                        error_code <= `SHOOP_SP_INVALID_PROGRAM;
                        done <= 1; phase <= IDLE;
                    end else if (state_bad) begin
                        error_code <= `SHOOP_SP_INVALID_STATE;
                        done <= 1; phase <= IDLE;
                    end else if (empty_program) begin
                        done <= 1; phase <= IDLE;
                    end else begin
                        phase <= FETCH;
                    end
                end
                FETCH: phase <= DECODE;
                DECODE: begin
                    opcode <= program_read_data[7:0];
                    arg0 <= program_read_data[23:8];
                    arg1 <= program_read_data[39:24];
                    arg2 <= program_read_data[55:40];
                    arg3 <= program_read_data[71:56];
                    arg4 <= program_read_data[87:72];
                    writeback <= 0;
                    if ((32'(pc)<run_sync_count)!=(program_read_data[7:0]==`SHOOP_SP_PROCESS_LOOP)) begin
                        error_code<=`SHOOP_SP_INVALID_PROGRAM; done<=1; phase<=IDLE;
                    end else case (program_read_data[7:0])
                        `SHOOP_SP_PROCESS_LOOP: begin
                            if (program_read_data[95:56]!=0) begin error_code<=`SHOOP_SP_INVALID_PROGRAM; done<=1; phase<=IDLE; end
                            else phase<=SYNC_BEGIN;
                        end
                        `SHOOP_SP_LOGICAL_PLAYBACK, `SHOOP_SP_LOGICAL_CAPTURE, `SHOOP_SP_PROCESS_SMOOTHING: begin
                            if (run_sync_count!=0) phase<=SYNC_BEGIN;
                            else phase<=LOOP_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_CONSTANT: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_CONSTANT,
                                program_read_data[55:40], 0, {32'b0,program_read_data[39:8]}, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_ARITHMETIC: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_ARITHMETIC,
                                program_read_data[23:8], program_read_data[39:24],
                                {32'b0,program_read_data[71:40]}, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_SILENCE: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_SILENCE,
                                program_read_data[23:8], 0, 0, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_INPUT: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_INPUT,
                                program_read_data[23:8], program_read_data[39:24], 0, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_PLAYBACK: begin
                            operand_address <= program_read_data[71:56];
                            operand <= ENABLE;
                            phase <= READ_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_GAIN: begin
                            operand_address <= program_read_data[23:8];
                            operand <= RAMP;
                            phase <= READ_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_MIX: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_MIX,
                                program_read_data[23:8], program_read_data[39:24],
                                {48'b0, program_read_data[55:40]}, 0, 0);
                            phase <= EMIT;
                        end
                        `SHOOP_SP_PROCESS_CAPTURE: begin
                            operand_address <= program_read_data[39:24];
                            operand <= ENABLE;
                            phase <= READ_REQUEST;
                        end
                        `SHOOP_SP_PROCESS_OUTPUT: begin
                            job_data <= pack_job(`SHOOP_SP_JOB_OUTPUT,
                                program_read_data[23:8], program_read_data[39:24], 0, 0, 0);
                            phase <= EMIT;
                        end
                        default: begin
                            error_code <= `SHOOP_SP_INVALID_PROGRAM;
                            done <= 1;
                            phase <= IDLE;
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
                    if (loop_invalid) begin
                        error_code <= `SHOOP_SP_INVALID_STATE;
                        done <= 1;
                        phase <= IDLE;
                    end else if (opcode == `SHOOP_SP_PROCESS_SMOOTHING) begin
                        playback_flags <= {loop_at_start,loop_mode==1};
                        operand_address <= arg3; operand <= SMOOTHING; phase <= READ_REQUEST;
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
                        && (length == 0 || position >= length)) begin
                        if (opcode == `SHOOP_SP_LOGICAL_PLAYBACK) begin
                            job_data <= pack_job(`SHOOP_SP_JOB_SILENCE, arg4, 0, 0, 0, 0);
                            phase <= EMIT;
                        end else begin
                            error_code <= `SHOOP_SP_INVALID_STATE;
                            done <= 1;
                            phase <= IDLE;
                        end
                    end else if (!operand_in_range) begin
                        error_code <= `SHOOP_SP_INVALID_STATE;
                        done <= 1;
                        phase <= IDLE;
                    end else begin
                        phase <= READ_CAPTURE;
                    end
                end
                READ_CAPTURE: begin
                    operand_data <= operand==LOOP_MEMORY ? {1'b0,loop_ram_read_data} : state_read_data;
                    phase <= READ_RESULT;
                end
                READ_RESULT: begin
                    if (operand==LOOP_MEMORY) begin
                        case (loop_operand)
                            LOOP_ROW: begin
                                if (loop_word==1) begin position<=operand_data[63:0]; length<=operand_data[127:64]; end
                                else for (integer i=0;i<13;i=i+1)
                                    if (i!=1 && loop_word==4'(i)) row[128*i+:128]<=operand_data[127:0];
                                if (loop_word==12) phase<=SYNC_CHECK;
                                else begin loop_word<=loop_word+1'b1; loop_read(target,loop_word+1'b1,LOOP_ROW); end
                            end
                            SOURCE_FLAGS: begin capture_job[127:0]<=operand_data[127:0]; loop_read(source_slot,12,SOURCE_STAMP); end
                            SOURCE_STAMP: phase<=SYNC_SOURCE_CHECK;
                            OWN_FLAGS: begin
                                loop_invalid<=!operand_data[V] || operand_data[G+:32]!=generation
                                    || operand_data[M+:2]==3 || !operand_data[PV];
                                loop_mode<=operand_data[M+:2]; begin_capture<=operand_data[BC];
                                loop_read(target,12,OWN_STAMP);
                            end
                            OWN_STAMP: begin
                                loop_invalid<=loop_invalid || arithmetic_sum[64] || arithmetic_sum[63:0]!=sync_frame;
                                loop_read(target,5,OWN_RENDER);
                            end
                            OWN_RENDER: begin
                                position<=operand_data[63:0]>>chunk_shift; length<=operand_data[127:64]>>chunk_shift;
                                loop_at_start<=operand_data[63:0]==0; phase<=LOOP_RESULT;
                            end
                            default: sync_fail();
                        endcase
                    end else if (is_ramp != (operand == RAMP)) begin
                        error_code <= `SHOOP_SP_INVALID_STATE;
                        done <= 1;
                        phase <= IDLE;
                    end else begin
                        case (operand)
                            ENABLE: begin
                                if (word_value == 0) begin
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
                                job_data <= pack_job(`SHOOP_SP_JOB_SMOOTHING,arg4,operand_data[15:0],
                                    {operand_data[63:32],16'b0,operand_data[31:16]}, {62'b0,playback_flags},0);
                                phase <= EMIT;
                            end
                            RAMP: begin
                                job_data <= pack_job(`SHOOP_SP_JOB_GAIN, arg1, arg2,
                                    operand_data[63:0], operand_data[127:64], 0);
                                ramp_state <= operand_data[95:0];
                                ramp_elapsed <= arithmetic_sum[32:0];
                                writeback <= 1;
                                phase <= EMIT;
                            end
                            default: begin
                                error_code <= `SHOOP_SP_INVALID_STATE;
                                done <= 1;
                                phase <= IDLE;
                            end
                        endcase
                    end
                end
                EMIT: begin
                    if (queue_remaining == 0) begin
                        error_code <= `SHOOP_SP_QUEUE_FULL;
                        done <= 1;
                        phase <= IDLE;
                    end else if (job_ready) begin
                        if (job_data[7:0] == `SHOOP_SP_JOB_BEGIN_CAPTURE) begin
                            job_data <= capture_job; begin_capture <= 0;
                        end else phase <= NEXT;
                    end
                end
                SYNC_BEGIN: begin
                    if (target>=32'(LOOP_DEPTH)) sync_fail();
                    else if (opcode==`SHOOP_SP_PROCESS_LOOP) begin loop_word<=0; loop_read(target,0,LOOP_ROW); end
                    else loop_read(target,0,OWN_FLAGS);
                end
                SYNC_CHECK: begin
                    pulse_pending<=0; delivered<=0; masked<=0; protected_valid<=0; row[PULSE]<=0;
                    if (!row[V] || row[G+:32]!=generation || row[M+:2]==3 || (row[PV] && row[PF+:64]==sync_frame)) sync_fail();
                    else if (row[SV]) begin
                        if (source_slot>=32'(LOOP_DEPTH)) sync_fail();
                        else if (source_self) begin
                            if (row[SG+:32]!=generation) sync_fail(); else phase<=SYNC_SCAN_BEGIN;
                        end else loop_read(source_slot,0,SOURCE_FLAGS);
                    end else phase<=SYNC_SCAN_BEGIN;
                end
                SYNC_SOURCE_CHECK: begin
                    if (!source_flags[V] || source_flags[G+:32]!=row[SG+:32] || source_flags[M+:2]==3
                        || !source_flags[PV] || operand_data[63:0]!=sync_frame) sync_fail();
                    else phase<=SYNC_SCAN_BEGIN;
                end
                SYNC_SCAN_BEGIN: begin sync_pass<=0; scan_index<=0; winner_valid<=0; phase<=SYNC_SCAN; end
                SYNC_SCAN: begin
                    if (!scan_plan[VALID] || scan_plan[EFF+:64]>sync_frame || scan_plan[TICKS]!=(sync_pass!=0)) phase<=SYNC_SCAN_NEXT;
                    else if (sync_pass!=0 && scan_plan[REM+:64]!=0) begin
                        if (scan_index) row[1152+REM+:64]<=arithmetic_sum[63:0]; else row[768+REM+:64]<=arithmetic_sum[63:0];
                        phase<=SYNC_SCAN_NEXT;
                    end else if (!winner_valid) begin winner<=scan_index; winner_valid<=1; phase<=SYNC_SCAN_NEXT; end
                    else if (scan_plan[SEQ+:64]>winner_plan[SEQ+:64]) begin winner<=scan_index; sync_resolve(winner,1,0,SYNC_SCAN_NEXT); end
                    else sync_resolve(scan_index,1,0,SYNC_SCAN_NEXT);
                end
                SYNC_SCAN_NEXT: begin
                    if (scan_index) phase<=SYNC_APPLY; else begin scan_index<=1; phase<=SYNC_SCAN; end
                end
                SYNC_APPLY: begin
                    if (!winner_valid) phase<=SYNC_GROUP_DONE;
                    else if (protected_valid && winner_plan[SEQ+:64]<=protected_sequence) sync_resolve(winner,1,0,SYNC_GROUP_DONE);
                    else begin protected_valid<=1; operand_data[63:0]<=winner_plan[SEQ+:64]; phase<=SYNC_APPLY_OP; end
                end
                SYNC_APPLY_OP: begin
                    if (op_result[W+1+:8]!=0) sync_resolve(winner,3,op_result[W+1+:8],SYNC_GROUP_DONE);
                    else begin
                        row<=op_result[W-1:0]; position<=op_result[POS+:64]; length<=op_result[LEN+:64]; changed<=op_changed;
                        applied_pulse<=op_result[W]; ramp_state[66:0]<={winner_plan[RET],winner_plan[OP+:2],winner_plan[PP+:64]};
                        sync_resolve(winner,0,0,SYNC_AFTER_APPLY);
                    end
                end
                SYNC_AFTER_APPLY: begin
                    if (changed) sync_emit(0,0,0,ramp_state[63:0],{61'b0,ramp_state[66:64]},SYNC_EFFECTS);
                    else phase<=SYNC_EFFECTS;
                end
                SYNC_EFFECTS: begin
                    if (ramp_state[65:64]==0) begin masked<=1; pulse_pending<=0; cancel_index<=0; phase<=SYNC_CANCEL; end
                    else begin
                        if (applied_pulse) begin masked<=0; if (!delivered) pulse_pending<=1; end
                        phase<=SYNC_GROUP_DONE;
                    end
                end
                SYNC_CANCEL: begin
                    if (cancel_plan[VALID] && cancel_plan[SEQ+:64]<protected_sequence) sync_resolve(cancel_index,2,0,SYNC_CANCEL_NEXT);
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
                    if (row[M+:2]==1 && length!=0 && position==length
                        && !(row[REP] && row[SV] && !source_self && source_playing && !source_tick)) begin
                        if (arithmetic_sum[64]) sync_fail();
                        else begin
                            row[CYC+:64]<=arithmetic_sum[63:0]; position<=0;
                            if (!delivered) pulse_pending<=1;
                            sync_emit(1,0,0,0,0,sync_natural_return);
                        end
                    end else phase<=sync_natural_return;
                end
                SYNC_SOURCE: begin
                    if (source_tick) begin
                        if (!masked && !delivered) pulse_pending<=1;
                        sync_pass<=1; scan_index<=0; winner_valid<=0; phase<=SYNC_SCAN;
                    end else phase<=SYNC_DELIVER;
                end
                SYNC_DELIVER: begin
                    if (pulse_pending) begin
                        row[PULSE]<=1; row[LV]<=1; row[LP+:64]<=sync_frame; delivered<=1; pulse_pending<=0;
                        sync_emit(2,0,0,0,0,SYNC_SELF);
                    end else phase<=SYNC_RENDER;
                end
                SYNC_SELF: begin
                    if (source_self) begin sync_pass<=2; scan_index<=0; winner_valid<=0; phase<=SYNC_SCAN; end
                    else phase<=SYNC_RENDER;
                end
                SYNC_RENDER: begin
                    row[RP+:64]<=position; row[RL+:64]<=length; row[BC]<=row[M+:2]==2 && row[CAP+:64]==0;
                    row[PF+:64]<=sync_frame; row[PV]<=1;
                    if (row[M+:2]==1 && length!=0 && position<length) phase<=SYNC_ADV_POSITION;
                    else if (row[M+:2]==2) phase<=SYNC_ADV_CAPTURE;
                    else phase<=SYNC_WRITE_BEGIN;
                end
                SYNC_ADV_POSITION: begin
                    if (arithmetic_sum[64] || arithmetic_sum[63:0]>length) sync_fail();
                    else begin position<=arithmetic_sum[63:0]; phase<=SYNC_WRITE_BEGIN; end
                end
                SYNC_ADV_CAPTURE: begin
                    if (arithmetic_sum[64]) sync_fail();
                    else begin row[CAP+:64]<=arithmetic_sum[63:0]; phase<=SYNC_WRITE_BEGIN; end
                end
                SYNC_WRITE_BEGIN: begin loop_word<=0; phase<=SYNC_WRITE; end
                SYNC_WRITE: begin
                    if (loop_word==12) begin
                        if (32'(pc)+1'b1==run_sync_count) sync_frame<=arithmetic_sum[63:0];
                        phase<=NEXT;
                    end else loop_word<=loop_word+1'b1;
                end
                SYNC_EVENT: if (sync_event_ready) phase<=sync_event_return;
                NEXT: begin
                    pc <= pc + 1'b1;
                    if ((pc + 1'b1) == run_count) begin
                        done <= 1;
                        phase <= IDLE;
                    end else begin
                        phase <= FETCH;
                    end
                end
                default: begin
                    error_code <= `SHOOP_SP_INVALID_PROGRAM;
                    done <= 1;
                    phase <= IDLE;
                end
            endcase
        end
    end
endmodule
