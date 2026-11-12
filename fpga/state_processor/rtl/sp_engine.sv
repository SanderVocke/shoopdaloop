`include "state_processor_defs.svh"

module sp_engine #(
    parameter integer PROGRAM_DEPTH = 256,
    parameter integer STATE_DEPTH = 256,
    parameter integer PROGRAM_ADDR_BITS = (PROGRAM_DEPTH > 1) ? $clog2(PROGRAM_DEPTH) : 1,
    parameter integer STATE_ADDR_BITS = (STATE_DEPTH > 1) ? $clog2(STATE_DEPTH) : 1,
    parameter integer PC_BITS = $clog2(PROGRAM_DEPTH + 1)
) (
    input  wire                                clk,
    input  wire                                reset,
    input  wire                                start,
    input  wire [31:0]                         instruction_count,
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
    output reg  [`SHOOP_SP_STATE_BITS-1:0]       state_write_data,
    output wire                                job_valid,
    input  wire                                job_ready,
    output reg  [`SHOOP_SP_JOB_BITS-1:0]         job_data,
    output wire loop_read_valid,
    input wire loop_read_ready,
    output wire [31:0] loop_read_slot,
    input wire loop_response_valid,
    input wire loop_response_error,
    input wire [163:0] loop_response_data
);
    typedef enum logic [3:0] {
        IDLE, FETCH, DECODE, READ_REQUEST, READ_RESULT, EMIT, NEXT, LOOP_REQUEST, LOOP_RESULT
    } phase_t;
    typedef enum logic [2:0] {
        ENABLE, POSITION, LENGTH, VIEW, RAMP
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
    reg begin_capture;
    reg [255:0] capture_job;
    reg [7:0] opcode;
    reg [15:0] arg0, arg1, arg2, arg3, arg4;
    reg [15:0] operand_address;
    reg [63:0] position;
    reg [63:0] length;
    reg writeback;
    wire [63:0] word_value = state_read_data[63:0];
    wire is_ramp = state_read_data[128];
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

    function automatic [31:0] advance_ramp(
        input [31:0] elapsed,
        input [31:0] duration,
        input [31:0] frames
    );
        reg [32:0] sum;
        begin
            sum = {1'b0, elapsed} + {1'b0, frames};
            advance_ramp = (sum >= {1'b0, duration}) ? duration : sum[31:0];
        end
    endfunction

    assign busy = phase != IDLE;
    assign instruction_pc = 32'(pc);
    assign program_read_enable = (phase == FETCH) && !reset;
    assign program_read_address = PROGRAM_ADDR_BITS'(pc);
    assign state_read_enable = (phase == READ_REQUEST) && operand_in_range && !reset;
    assign state_read_address = STATE_ADDR_BITS'(operand_address);
    assign job_valid = (phase == EMIT) && (queue_remaining != 0) && !reset;
    assign state_write_enable = job_valid && job_ready && writeback;
    assign state_write_address = STATE_ADDR_BITS'(arg0);
    assign loop_read_valid = phase == LOOP_REQUEST && !reset;
    assign loop_read_slot = {16'b0, arg0};

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            operand <= ENABLE;
            pc <= 0;
            run_count <= 0;
            run_state_count <= 0;
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
            state_write_data <= 0;
            job_data <= 0;
            done <= 0;
            error_code <= `SHOOP_SP_OK;
        end else begin
            done <= 0;
            case (phase)
                IDLE: if (start) begin
                    pc <= 0; chunk_shift <= shift_for(chunk_frames);
                    error_code <= `SHOOP_SP_OK;
                    if ((queued_jobs != 0) || (queue_remaining < job_budget)) begin
                        error_code <= `SHOOP_SP_QUEUE_FULL;
                        done <= 1;
                    end else if (chunk_frames == 0 || (chunk_frames & (chunk_frames-1)) != 0
                        || quantum == 0 || (quantum & (chunk_frames-1)) != 0) begin
                        error_code <= `SHOOP_SP_INVALID_STATE; done <= 1;
                    end else if (instruction_count > 32'(PROGRAM_DEPTH)) begin
                        error_code <= `SHOOP_SP_INVALID_PROGRAM;
                        done <= 1;
                    end else if (state_count > 32'(STATE_DEPTH)) begin
                        error_code <= `SHOOP_SP_INVALID_STATE;
                        done <= 1;
                    end else if (instruction_count == 0) begin
                        done <= 1;
                    end else begin
                        run_count <= PC_BITS'(instruction_count);
                        run_state_count <= state_count;
                        run_quantum <= quantum;
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
                    case (program_read_data[7:0])
                        `SHOOP_SP_LOGICAL_PLAYBACK, `SHOOP_SP_LOGICAL_CAPTURE: phase <= LOOP_REQUEST;
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
                LOOP_REQUEST: if (loop_read_ready) phase <= LOOP_RESULT;
                LOOP_RESULT: if (loop_response_valid) begin
                    if (loop_response_error || !loop_response_data[0]
                        || loop_response_data[1+:32] != {arg2,arg1}
                        || loop_response_data[33+:2] == 3) begin
                        error_code <= `SHOOP_SP_INVALID_STATE;
                        done <= 1;
                        phase <= IDLE;
                    end else if (opcode == `SHOOP_SP_LOGICAL_PLAYBACK) begin
                        if (loop_response_data[33+:2] == 1 && (loop_response_data[35+:64] >> chunk_shift) != 0
                            && (loop_response_data[99+:64] >> chunk_shift) < (loop_response_data[35+:64] >> chunk_shift)) begin
                            position <= loop_response_data[99+:64] >> chunk_shift;
                            length <= loop_response_data[35+:64] >> chunk_shift;
                            operand_address <= arg3; operand <= VIEW; phase <= READ_REQUEST;
                        end else begin
                            job_data <= pack_job(`SHOOP_SP_JOB_SILENCE, arg4, 0, 0, 0, 0);
                            phase <= EMIT;
                        end
                    end else if (loop_response_data[33+:2] == 2) begin
                        begin_capture <= loop_response_data[163];
                        operand_address <= arg3; operand <= VIEW; phase <= READ_REQUEST;
                    end else phase <= NEXT;
                end
                READ_REQUEST: begin
                    if (!operand_in_range) begin
                        error_code <= `SHOOP_SP_INVALID_STATE;
                        done <= 1;
                        phase <= IDLE;
                    end else begin
                        phase <= READ_RESULT;
                    end
                end
                READ_RESULT: begin
                    if (is_ramp != (operand == RAMP)) begin
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
                                if ((word_value == 0) || (position >= word_value)) begin
                                    error_code <= `SHOOP_SP_INVALID_STATE;
                                    done <= 1;
                                    phase <= IDLE;
                                end else begin
                                    length <= word_value;
                                    operand_address <= arg0;
                                    operand <= VIEW;
                                    phase <= READ_REQUEST;
                                end
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
                            RAMP: begin
                                job_data <= pack_job(`SHOOP_SP_JOB_GAIN, arg1, arg2,
                                    state_read_data[63:0], state_read_data[127:64], 0);
                                state_write_data <= {1'b1,
                                    advance_ramp(state_read_data[127:96], state_read_data[95:64], run_quantum),
                                    state_read_data[95:0]};
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
