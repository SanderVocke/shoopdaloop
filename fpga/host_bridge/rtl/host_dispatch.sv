`include "state_processor_defs.svh"

module host_dispatch #(
    parameter integer QUANTUM = 32,
    parameter integer CHUNK_FRAMES = 1,
    parameter integer INPUT_PORTS = 1,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer WATCHDOG_CYCLES = 20_000_000
) (
    input wire clk,
    input wire reset,
    input wire [31:0] epoch,
    output reg fault,
    input wire command_valid,
    output wire command_ready,
    input wire [31:0] command_tag,
    input wire [31:0] command_op,
    input wire [32*host_registers::HOST_PAYLOAD_WORDS-1:0] command_data,
    output wire response_valid,
    input wire response_ready,
    output reg [31:0] response_epoch,
    output reg [31:0] response_tag,
    output reg [31:0] response_code,
    output reg [32*host_registers::HOST_PAYLOAD_WORDS-1:0] response_data,
    output wire cfg_valid,
    input wire cfg_ready,
    output wire cfg_state,
    output wire cfg_loop,
    output wire cfg_write,
    output wire [15:0] cfg_address,
    output wire [`SHOOP_SP_STATE_BITS-1:0] cfg_write_data,
    input wire cfg_response_valid,
    input wire cfg_response_error,
    input wire [`SHOOP_SP_STATE_BITS-1:0] cfg_read_data,
    output wire audio_cfg_valid,
    input wire audio_cfg_ready,
    output wire [1:0] audio_cfg_space,
    output wire audio_cfg_write,
    output wire [31:0] audio_cfg_address,
    output wire [31:0] audio_cfg_write_data,
    input wire audio_cfg_response_valid,
    input wire audio_cfg_response_error,
    input wire [31:0] audio_cfg_read_data,
    output wire store_cfg_valid,
    input wire store_cfg_ready,
    output wire [2:0] store_cfg_space,
    output wire store_cfg_write,
    output wire [31:0] store_cfg_address,
    output wire [159:0] store_cfg_write_data,
    input wire store_cfg_response_valid,
    input wire store_cfg_response_error,
    input wire [159:0] store_cfg_read_data,
    output wire store_cmd_valid,
    input wire store_cmd_ready,
    output wire [63:0] store_cmd_view,
    input wire store_done,
    input wire [7:0] store_error_code,
    output wire switch_valid,
    input wire switch_ready,
    output wire [63:0] switch_expected,
    output wire [63:0] switch_revision,
    output wire switch_region,
    output wire [15:0] switch_instructions,
    output wire [15:0] switch_sync_count,
    output wire [15:0] switch_states,
    output wire [15:0] switch_jobs,
    output wire [15:0] switch_buffers,
    input wire switch_response_valid,
    output wire switch_response_ready,
    input wire switch_response_error,
    input wire [63:0] switch_first_frame,
    input wire [63:0] active_revision,
    output wire run_valid,
    input wire run_ready,
    output wire [31:0] run_quantum,
    output wire [31:0] run_chunk_frames,
    output wire [31:0] run_inputs,
    output wire [31:0] run_outputs,
    input wire done,
    input wire [1:0] error_stage,
    input wire [7:0] error_code,
    input wire [63:0] sync_frame,
    input wire memory_fault
);
    import host_registers::*;
    typedef enum logic [3:0] {IDLE, CFG_REQUEST, CFG_WAIT, AUDIO_REQUEST, AUDIO_WAIT,
        STORE_CFG_REQUEST, STORE_CFG_WAIT, CLEAR_REQUEST, CLEAR_WAIT, SWITCH_REQUEST, SWITCH_WAIT,
        RUN_REQUEST, RUN_WAIT, RESPOND} phase_t;
    phase_t phase;
    reg [319:0] args;
    reg [31:0] cycles;

    wire [2:0] space = args[2:0];
    wire [31:0] address = args[32 +: 32];
    wire writing = args[8];
    wire [HOST_PAYLOAD_WORDS*32-1:0] request = command_data;
    wire [31:0] request_control = request[0 +: 32];
    wire bad_switch_width = |{request[159:129], request[191:176], request[223:208],
        request[255:240], request[287:272], request[319:304]};
    wire request_extra = |request[HOST_PAYLOAD_WORDS*32-1:320];
    wire executing = phase != IDLE && phase != RESPOND;
    wire unused_argument_bits = &{1'b0,args[319:304],args[287:272],args[255:240]};

    assign command_ready = phase == IDLE && !fault && !memory_fault;
    assign response_valid = phase == RESPOND;
    assign cfg_valid = phase == CFG_REQUEST;
    assign cfg_state = space[1:0] == 1;
    assign cfg_loop = space[1:0] == 2;
    assign cfg_write = writing;
    assign cfg_address = address[15:0];
    assign cfg_write_data = args[64 +: `SHOOP_SP_STATE_BITS];
    assign audio_cfg_valid = phase == AUDIO_REQUEST;
    assign audio_cfg_space = space[1:0];
    assign audio_cfg_write = writing;
    assign audio_cfg_address = address;
    assign audio_cfg_write_data = args[64 +: 32];
    assign store_cfg_valid = phase == STORE_CFG_REQUEST;
    assign store_cfg_space = space;
    assign store_cfg_write = writing;
    assign store_cfg_address = address;
    assign store_cfg_write_data = args[64 +: 160];
    assign store_cmd_valid = phase == CLEAR_REQUEST;
    assign store_cmd_view = args[63:0];
    assign switch_valid = phase == SWITCH_REQUEST;
    assign switch_expected = args[0 +: 64];
    assign switch_revision = args[64 +: 64];
    assign switch_region = args[128];
    assign switch_instructions = args[160 +: 16];
    assign switch_sync_count = args[192 +: 16];
    assign switch_states = args[224 +: 16];
    assign switch_jobs = args[256 +: 16];
    assign switch_buffers = args[288 +: 16];
    assign switch_response_ready = phase == SWITCH_WAIT;
    assign run_valid = phase == RUN_REQUEST || phase == SWITCH_REQUEST;
    assign run_quantum = QUANTUM;
    assign run_chunk_frames = CHUNK_FRAMES;
    assign run_inputs = INPUT_PORTS;
    assign run_outputs = OUTPUT_PORTS;

    always @(posedge clk) begin
        if (reset) begin
            fault <= 0;
            phase <= IDLE;
            args <= 0;
            cycles <= 0;
            response_epoch <= 0;
            response_tag <= 0;
            response_code <= RESULT_OK;
            response_data <= 0;
        end else if (executing && (memory_fault || cycles >= WATCHDOG_CYCLES)) begin
            fault <= 1;
            response_code <= RESULT_FAULT;
            phase <= RESPOND;
        end else begin
            if (memory_fault)
                fault <= 1;
            if (executing)
                cycles <= cycles+1;
            case (phase)
                IDLE: if (command_valid && command_ready) begin
                    args <= request[319:0];
                    response_epoch <= epoch;
                    response_tag <= command_tag;
                    response_code <= RESULT_INVALID;
                    response_data <= 0;
                    cycles <= 0;
                    phase <= RESPOND;
                    if (!request_extra) case (command_op)
                        OP_CORE_CONFIG: if ((request_control & ~32'h103) == 0 && request_control[1:0] != 3 &&
                            request[63:48] == 0 && request[319:193] == 0) phase <= CFG_REQUEST;
                        OP_AUDIO_CONFIG: if ((request_control & ~32'h103) == 0 && request[319:96] == 0) phase <= AUDIO_REQUEST;
                        OP_STORE_CONFIG: if ((request_control & ~32'h107) == 0 && request[319:224] == 0) phase <= STORE_CFG_REQUEST;
                        OP_CLEAR_VIEW: if (request[319:64] == 0) phase <= CLEAR_REQUEST;
                        OP_SWITCH_PROGRAM: if (!bad_switch_width) phase <= SWITCH_REQUEST;
                        OP_RUN_QUANTUM: if (request[319:0] == 0) phase <= RUN_REQUEST;
                        default: phase <= RESPOND;
                    endcase
                end
                CFG_REQUEST: if (cfg_ready) phase <= CFG_WAIT;
                CFG_WAIT: if (cfg_response_valid) begin
                    response_code <= cfg_response_error ? RESULT_REJECTED : RESULT_OK;
                    response_data[`SHOOP_SP_STATE_BITS-1:0] <= cfg_read_data;
                    phase <= RESPOND;
                end
                AUDIO_REQUEST: if (audio_cfg_ready) phase <= AUDIO_WAIT;
                AUDIO_WAIT: if (audio_cfg_response_valid) begin
                    response_code <= audio_cfg_response_error ? RESULT_REJECTED : RESULT_OK;
                    response_data[31:0] <= audio_cfg_read_data;
                    phase <= RESPOND;
                end
                STORE_CFG_REQUEST: if (store_cfg_ready) phase <= STORE_CFG_WAIT;
                STORE_CFG_WAIT: if (store_cfg_response_valid) begin
                    response_code <= store_cfg_response_error ? RESULT_REJECTED : RESULT_OK;
                    response_data[159:0] <= store_cfg_read_data;
                    phase <= RESPOND;
                end
                CLEAR_REQUEST: if (store_cmd_ready) phase <= CLEAR_WAIT;
                CLEAR_WAIT: if (store_done) begin
                    response_code <= store_error_code == 0 ? RESULT_OK : RESULT_REJECTED;
                    response_data[7:0] <= store_error_code;
                    phase <= RESPOND;
                end
                SWITCH_REQUEST: if (switch_ready) phase <= SWITCH_WAIT;
                SWITCH_WAIT: if (switch_response_valid) begin
                    response_code <= switch_response_error ? RESULT_REJECTED : RESULT_OK;
                    response_data[127:0] <= {active_revision, switch_first_frame};
                    phase <= RESPOND;
                end
                RUN_REQUEST: if (run_ready) phase <= RUN_WAIT;
                RUN_WAIT: if (done) begin
                    fault <= error_code != 0;
                    response_code <= error_code == 0 ? RESULT_OK : RESULT_FAULT;
                    response_data[169:0] <= {error_stage, error_code, active_revision, sync_frame, cycles};
                    phase <= RESPOND;
                end
                RESPOND: if (response_ready) phase <= IDLE;
                default: phase <= IDLE;
            endcase
        end
    end
endmodule
