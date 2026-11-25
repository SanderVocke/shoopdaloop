`include "state_processor_defs.svh"

module host_dispatch #(
    parameter integer QUANTUM = 32,
    parameter integer CHUNK_FRAMES = 1,
    parameter integer INPUT_PORTS = 1,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer STATE_DEPTH = 16,
    parameter integer CONTROL_CYCLES = 16666,
    parameter integer WATCHDOG_CYCLES = 20_000_000
) (
    input wire clk,
    input wire reset,
    input wire [31:0] epoch,
    input wire audio_fault,
    input wire event_overflow,
    input wire host_abort,
    input wire recovery_quiesce,
    output reg fault,
    output reg [31:0] fault_epoch, fault_cause, fault_tag, fault_op,
    output reg [31:0] fault_cycles, fault_control_cycles, fault_detail,
    output reg [63:0] fault_revision, fault_frame,
    output reg audio_enable,
    input wire quantum_valid,
    output wire quantum_ready,
    output reg quantum_done,
    output reg [31:0] quantum_cycles,
    output reg [63:0] quantum_frame,
    output reg [63:0] quantum_revision,
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
    output wire [3:0] store_cmd_opcode,
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
    output wire [15:0] switch_buffers,
    output wire [43:0] switch_future,
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
    typedef enum logic [4:0] {IDLE, CFG_REQUEST, CFG_WAIT, AUDIO_REQUEST, AUDIO_WAIT,
        STORE_CFG_REQUEST, STORE_CFG_WAIT, CLEAR_REQUEST, CLEAR_WAIT, SWITCH_REQUEST, SWITCH_WAIT,
        RUN_REQUEST, RUN_WAIT, SNAPSHOT_REQUEST, SNAPSHOT_WAIT, SNAPSHOT_READ, SNAPSHOT_COPY, RESPOND,
        BATCH_LOAD, BATCH_SELECT, BATCH_READ_REQUEST, BATCH_READ_WAIT,
        BATCH_WRITE_REQUEST, BATCH_WRITE_WAIT} phase_t;
    phase_t phase;
    reg [319:0] args;
    reg [31:0] cycles;
    reg [31:0] current_tag, current_op;
    reg paced;
    reg audio_started;
    reg control_held;
    reg draining;
    reg [31:0] control_cycles;
    localparam integer SNAPSHOT_WORDS = STATE_DEPTH*16;
    localparam integer SNAPSHOT_INDEX = $clog2(2*SNAPSHOT_WORDS);
    (* ram_style = "block" *) reg [63:0] snapshot_memory [0:2*SNAPSHOT_WORDS-1];
    reg [63:0] snapshot_read_data;
    reg [31:0] snapshot_generation, snapshot_size, snapshot_pos, snapshot_limit, snapshot_base;
    reg snapshot_bank;
    reg [2:0] snapshot_reply_word;

    localparam integer BATCH_PATCHES = 512;
    (* ram_style = "block" *) reg [272:0] batch_memory [0:BATCH_PATCHES-1];
    (* ram_style = "block" *) reg [63:0] batch_values [0:BATCH_PATCHES-1];
    reg [272:0] batch_entry;
    reg [63:0] batch_value;
    reg batch_open, batch_writing, batch_deadline_valid;
    reg [9:0] batch_count, batch_limit, batch_index;
    reg [15:0] batch_last_address;
    reg [63:0] batch_revision, batch_frame, batch_deadline;
    wire [15:0] batch_address = batch_entry[15:0];
    wire batch_rebase = batch_entry[16];
    wire [63:0] batch_expected = batch_entry[17 +: 64];
    wire [63:0] batch_guard = batch_entry[81 +: 64];
    wire [63:0] batch_mask = batch_entry[145 +: 64];
    wire [63:0] batch_next = batch_entry[209 +: 64];
    wire [64:0] batch_rebased = {1'b0,sync_frame} + {1'b0,batch_next} - {1'b0,batch_frame};
    wire [63:0] batch_replacement = batch_rebase ? batch_rebased[63:0] : batch_next;
    wire batch_reading = phase == BATCH_READ_REQUEST || phase == BATCH_READ_WAIT;
    wire batch_applying = phase == BATCH_WRITE_REQUEST || phase == BATCH_WRITE_WAIT;
    wire batch_last = batch_index+1 == batch_limit;
    wire batch_write_failed = phase == BATCH_WRITE_WAIT && cfg_response_valid && cfg_response_error;

    wire [2:0] space = args[2:0];
    wire [31:0] address = args[32 +: 32];
    wire writing = args[8];
    wire [HOST_PAYLOAD_WORDS*32-1:0] request = command_data;
    wire [31:0] request_control = request[0 +: 32];
    wire bad_switch_width = |{request[159:141], request[255:240], request[287:272], request[319:304]}
        || ({request[223:208],request[191:176]} == 0 && request[140:129] != 0);
    wire request_extra = |request[HOST_PAYLOAD_WORDS*32-1:320];
    wire executing = phase != IDLE && phase != RESPOND;
    wire unused_argument_bits = &{1'b0,args[319:304],args[287:240]};

    wire control_expired = control_held && control_cycles >= CONTROL_CYCLES-1;
    wire device_fault = memory_fault || audio_fault || event_overflow || control_expired || host_abort;
    wire watchdog_expired = executing && cycles >= WATCHDOG_CYCLES;
    wire pipeline_failed = (phase == RUN_WAIT && done && error_code != 0) || batch_write_failed;
    wire [31:0] causes = (memory_fault ? FAULT_MEMORY : 0)
        | (audio_fault ? FAULT_AUDIO : 0) | (event_overflow ? FAULT_EVENT_OVERFLOW : 0)
        | (control_expired ? FAULT_CONTROL_TIMEOUT : 0) | (host_abort ? FAULT_ABORT : 0)
        | (watchdog_expired ? FAULT_WATCHDOG : 0) | (pipeline_failed ? FAULT_PIPELINE : 0)
        | (host_abort && recovery_quiesce ? FAULT_RECOVERY : 0);
    assign command_ready = phase == IDLE && !fault && !device_fault && !(audio_enable && quantum_valid && !control_held);
    assign quantum_ready = phase == RUN_REQUEST && paced && run_ready && !device_fault;
    assign response_valid = phase == RESPOND;
    assign cfg_valid = phase == CFG_REQUEST || phase == SNAPSHOT_REQUEST ||
        phase == BATCH_READ_REQUEST || phase == BATCH_WRITE_REQUEST;
    assign cfg_state = phase == CFG_REQUEST && space[1:0] == 1;
    assign cfg_loop = batch_reading || batch_applying || phase == SNAPSHOT_REQUEST || space[1:0] == 2;
    assign cfg_write = phase == BATCH_WRITE_REQUEST || (phase == CFG_REQUEST && writing);
    assign cfg_address = batch_reading || batch_applying ? batch_address :
        phase == SNAPSHOT_REQUEST ? snapshot_base[15:0]+snapshot_pos[15:0] : address[15:0];
    assign cfg_write_data = batch_applying ? {{(`SHOOP_SP_STATE_BITS-64){1'b0}},batch_value} : args[64 +: `SHOOP_SP_STATE_BITS];
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
    assign store_cmd_opcode = draining ? 4'd7 : 4'd2;
    assign store_cmd_view = args[63:0];
    assign switch_valid = phase == SWITCH_REQUEST;
    assign switch_expected = args[0 +: 64];
    assign switch_revision = args[64 +: 64];
    assign switch_region = args[128];
    assign switch_instructions = args[160 +: 16];
    assign switch_sync_count = args[192 +: 16];
    assign switch_states = args[224 +: 16];
    assign switch_buffers = args[288 +: 16];
    assign switch_future = {args[223:208],args[191:176],args[140:129]};
    assign switch_response_ready = phase == SWITCH_WAIT;
    assign run_valid = (phase == RUN_REQUEST && (!paced || quantum_valid)) || phase == SWITCH_REQUEST;
    assign run_quantum = QUANTUM;
    assign run_chunk_frames = CHUNK_FRAMES;
    assign run_inputs = INPUT_PORTS;
    assign run_outputs = OUTPUT_PORTS;

    initial if (STATE_DEPTH < 1 || STATE_DEPTH > 4096) $fatal(1,"invalid snapshot loop capacity");
    initial if (CONTROL_CYCLES < 1) $fatal(1,"invalid control window");

    always @(posedge clk) begin
        if (phase == SNAPSHOT_WAIT && cfg_response_valid && !cfg_response_error)
            snapshot_memory[SNAPSHOT_INDEX'((snapshot_bank ? 0 : SNAPSHOT_WORDS) + snapshot_pos)] <= cfg_read_data[63:0];
        if (phase == BATCH_LOAD) begin
            batch_entry <= batch_memory[batch_index[8:0]];
            batch_value <= batch_values[batch_index[8:0]];
        end
        if (phase == BATCH_READ_WAIT && cfg_response_valid && !cfg_response_error)
            batch_values[batch_index[8:0]] <= (cfg_read_data[63:0] & ~batch_mask) | (batch_replacement & batch_mask);
        if (phase == IDLE && command_valid && command_ready && command_op == OP_STAGE_PATCH &&
            batch_open && batch_count < batch_limit && 32'(batch_count) < BATCH_PATCHES)
            batch_memory[batch_count[8:0]] <= {request[287:32],request[16:0]};
        if (phase == SNAPSHOT_READ)
            snapshot_read_data <= snapshot_memory[SNAPSHOT_INDEX'((snapshot_bank ? SNAPSHOT_WORDS : 0) + snapshot_pos)];
    end

    always @(posedge clk) begin
        if (reset) begin
            fault_epoch <= 0;
            fault_cause <= 0;
            fault_tag <= 0;
            fault_op <= 0;
            fault_cycles <= 0;
            fault_control_cycles <= 0;
            fault_detail <= 0;
            fault_revision <= 0;
            fault_frame <= 0;
        end else if (!fault && causes != 0) begin
            fault_epoch <= epoch;
            fault_cause <= causes;
            fault_tag <= current_tag;
            fault_op <= current_op;
            fault_cycles <= cycles;
            fault_control_cycles <= control_cycles;
            fault_detail <= {8'b0, error_code, 6'b0, error_stage, 3'b0, phase};
            fault_revision <= active_revision;
            fault_frame <= sync_frame;
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            batch_open <= 0;
            batch_writing <= 0;
            batch_deadline_valid <= 0;
            batch_count <= 0;
            batch_limit <= 0;
            batch_index <= 0;
            batch_last_address <= 0;
            batch_revision <= 0;
            batch_frame <= 0;
            batch_deadline <= 0;
            fault <= 0;
            current_tag <= 0;
            current_op <= 0;
            audio_enable <= 0;
            audio_started <= 0;
            control_held <= 0;
            draining <= 0;
            control_cycles <= 0;
            phase <= IDLE;
            args <= 0;
            cycles <= 0;
            paced <= 0;
            quantum_done <= 0;
            quantum_cycles <= 0;
            quantum_frame <= 0;
            quantum_revision <= 0;
            snapshot_generation <= 0;
            snapshot_size <= 0;
            snapshot_pos <= 0;
            snapshot_limit <= 0;
            snapshot_base <= 0;
            snapshot_bank <= 0;
            snapshot_reply_word <= 0;
            response_epoch <= 0;
            response_tag <= 0;
            response_code <= RESULT_OK;
            response_data <= 0;
        end else if (executing && (device_fault || cycles >= WATCHDOG_CYCLES)) begin
            quantum_done <= 0;
            fault <= 1;
            audio_enable <= 0;
            control_held <= 0;
            response_code <= RESULT_FAULT;
            phase <= paced ? IDLE : RESPOND;
        end else begin
            quantum_done <= 0;
            if (control_held)
                control_cycles <= control_cycles+1;
            if (device_fault) begin
                fault <= 1;
                audio_enable <= 0;
                control_held <= 0;
            end
            if (executing)
                cycles <= cycles+1;
            case (phase)
                IDLE: if (command_valid && command_ready) begin
                    current_tag <= command_tag;
                    current_op <= command_op;
                    args <= request[319:0];
                    response_epoch <= epoch;
                    response_tag <= command_tag;
                    response_code <= RESULT_INVALID;
                    response_data <= 0;
                    cycles <= 0;
                    paced <= 0;
                    phase <= RESPOND;
                    if (command_op == OP_BEGIN_BATCH || command_op == OP_COMMIT_BATCH ||
                        command_op == OP_DISCARD_BATCH || (command_op == OP_STAGE_PATCH && request_extra))
                        batch_open <= 0;
                    if (!request_extra) case (command_op)
                        OP_CORE_CONFIG: if ((request_control & ~32'h103) == 0 && request_control[1:0] != 3 &&
                            request[63:48] == 0 && request[319:193] == 0) phase <= CFG_REQUEST;
                        OP_AUDIO_CONFIG: if ((request_control & ~32'h103) == 0 && request[319:96] == 0) phase <= AUDIO_REQUEST;
                        OP_STORE_CONFIG: if ((request_control & ~32'h107) == 0 && request[319:224] == 0) phase <= STORE_CFG_REQUEST;
                        OP_CLEAR_VIEW: if (request[319:64] == 0) begin
                            draining <= 0;
                            phase <= CLEAR_REQUEST;
                        end
                        OP_SWITCH_PROGRAM: if (!bad_switch_width) phase <= SWITCH_REQUEST;
                        OP_RUN_QUANTUM: if (request[319:0] == 0) begin
                            if (audio_started || control_held) response_code <= RESULT_REJECTED;
                            else phase <= RUN_REQUEST;
                        end
                        OP_CAPTURE_SNAPSHOT: if (request[319:64] == 0 && request[63:32] <= STATE_DEPTH &&
                            request_control <= STATE_DEPTH-request[63:32] &&
                            snapshot_generation != 32'hffffffff) begin
                            snapshot_pos <= 0;
                            snapshot_limit <= request_control << 4;
                            snapshot_base <= request[63:32] << 4;
                            if (request_control == 0) begin
                                snapshot_generation <= snapshot_generation+1;
                                snapshot_size <= 0;
                                response_code <= RESULT_OK;
                                response_data[159:0] <= {active_revision, sync_frame, snapshot_generation+32'd1};
                            end else phase <= SNAPSHOT_REQUEST;
                        end
                        OP_READ_SNAPSHOT: if (request[319:96] == 0 && request_control == snapshot_generation &&
                            request[63:32] < snapshot_size && request[95:64] > 0 &&
                            request[95:64] <= HOST_PAYLOAD_WORDS/2 &&
                            request[95:64] <= snapshot_size-request[63:32]) begin
                            snapshot_pos <= request[63:32];
                            snapshot_limit <= request[63:32]+request[95:64];
                            snapshot_reply_word <= 0;
                            phase <= SNAPSHOT_READ;
                        end
                        OP_ENABLE_AUDIO: if (request[319:1] == 0) begin
                            if (!request[0] || audio_enable || (!audio_started && active_revision != 0)) begin
                                audio_enable <= request[0];
                                audio_started <= audio_started || request[0];
                                response_code <= RESULT_OK;
                            end else response_code <= RESULT_REJECTED;
                        end
                        OP_BEGIN_CONTROL: if (request[319:64] == 0) begin
                            if (!control_held && request[63:0] == active_revision) begin
                                control_held <= 1;
                                control_cycles <= 0;
                                response_code <= RESULT_OK;
                                response_data[127:0] <= {active_revision,sync_frame};
                            end else response_code <= RESULT_REJECTED;
                        end
                        OP_END_CONTROL: if (request[319:0] == 0) begin
                            if (control_held) begin
                                control_held <= 0;
                                response_code <= RESULT_OK;
                            end else response_code <= RESULT_REJECTED;
                        end
                        OP_BEGIN_BATCH: if (request[319:288] == 0 && !control_held &&
                            request[159:128] > 0 && request[159:128] <= BATCH_PATCHES &&
                            request[191:160] > 0 && request[191:160] <= 32 &&
                            request[159:128] <= request[191:160]*16 && request[223:193] == 0) begin
                            batch_open <= 1;
                            batch_count <= 0;
                            batch_limit <= request[137:128];
                            batch_revision <= request[63:0];
                            batch_frame <= request[127:64];
                            batch_deadline_valid <= request[192];
                            batch_deadline <= request[287:224];
                            response_code <= RESULT_OK;
                        end
                        OP_STAGE_PATCH: begin
                            if (batch_open && !control_held && batch_count < batch_limit &&
                                request[319:288] == 0 && request[31:17] == 0 &&
                                32'(request[15:0]) < STATE_DEPTH*16 &&
                                (batch_count == 0 || request[15:0] > batch_last_address) &&
                                (!request[16] || (request[3:0] == 10 || request[3:0] == 13))) begin
                                batch_count <= batch_count+1;
                                batch_last_address <= request[15:0];
                                response_code <= RESULT_OK;
                            end else batch_open <= 0;
                        end
                        OP_COMMIT_BATCH: if (request[319:0] == 0) begin
                            batch_open <= 0;
                            response_data[127:0] <= {active_revision,sync_frame};
                            response_code <= RESULT_REJECTED;
                            if (batch_open && !control_held && batch_count == batch_limit) begin
                                if (batch_revision != active_revision) response_data[159:128] <= 2;
                                else if (batch_frame > sync_frame ||
                                    (batch_deadline_valid && sync_frame > batch_deadline)) response_data[159:128] <= 1;
                                else begin
                                    control_held <= 1;
                                    control_cycles <= 0;
                                    batch_index <= 0;
                                    batch_writing <= 0;
                                    phase <= BATCH_LOAD;
                                end
                            end
                        end
                        OP_DISCARD_BATCH: if (request[319:0] == 0) begin
                            batch_open <= 0;
                            response_code <= RESULT_OK;
                        end
                        OP_DRAIN_STOP: if (request[319:0] == 0 && !control_held) begin
                            audio_enable <= 0;
                            draining <= 1;
                            phase <= CLEAR_REQUEST;
                        end
                        default: phase <= RESPOND;
                    endcase
                end else if (quantum_valid && !control_held && !fault && !device_fault) begin
                    current_tag <= 0;
                    current_op <= OP_RUN_QUANTUM;
                    paced <= 1;
                    cycles <= 0;
                    phase <= RUN_REQUEST;
                end
                BATCH_LOAD: phase <= BATCH_SELECT;
                BATCH_SELECT: begin
                    if (!batch_writing) phase <= BATCH_READ_REQUEST;
                    else if (batch_mask != 0) phase <= BATCH_WRITE_REQUEST;
                    else if (batch_last) begin
                        control_held <= 0;
                        response_code <= RESULT_OK;
                        response_data[191:160] <= control_cycles;
                        phase <= RESPOND;
                    end else begin
                        batch_index <= batch_index+1;
                        phase <= BATCH_LOAD;
                    end
                end
                BATCH_READ_REQUEST: if (cfg_ready) phase <= BATCH_READ_WAIT;
                BATCH_READ_WAIT: if (cfg_response_valid) begin
                    if (cfg_response_error || ((cfg_read_data[63:0] ^ batch_expected) & batch_guard) != 0 ||
                        (batch_rebase && (batch_next < batch_frame || batch_rebased[64]))) begin
                        control_held <= 0;
                        response_code <= RESULT_REJECTED;
                        response_data[159:128] <= 3;
                        phase <= RESPOND;
                    end else begin
                        if (batch_last) begin
                            batch_index <= 0;
                            batch_writing <= 1;
                        end else batch_index <= batch_index+1;
                        phase <= BATCH_LOAD;
                    end
                end
                BATCH_WRITE_REQUEST: if (cfg_ready) phase <= BATCH_WRITE_WAIT;
                BATCH_WRITE_WAIT: if (cfg_response_valid) begin
                    if (cfg_response_error) begin
                        fault <= 1;
                        audio_enable <= 0;
                        control_held <= 0;
                        response_code <= RESULT_FAULT;
                        phase <= RESPOND;
                    end else if (batch_last) begin
                        control_held <= 0;
                        response_code <= RESULT_OK;
                        response_data[191:160] <= control_cycles;
                        phase <= RESPOND;
                    end else begin
                        batch_index <= batch_index+1;
                        phase <= BATCH_LOAD;
                    end
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
                SNAPSHOT_REQUEST: if (cfg_ready) phase <= SNAPSHOT_WAIT;
                SNAPSHOT_WAIT: if (cfg_response_valid) begin
                    if (cfg_response_error) begin
                        response_code <= RESULT_REJECTED;
                        phase <= RESPOND;
                    end else if (snapshot_pos+1 == snapshot_limit) begin
                        snapshot_generation <= snapshot_generation+1;
                        snapshot_size <= snapshot_limit;
                        snapshot_bank <= !snapshot_bank;
                        response_code <= RESULT_OK;
                        response_data[191:0] <= {snapshot_limit >> 4, active_revision, sync_frame, snapshot_generation+32'd1};
                        phase <= RESPOND;
                    end else begin
                        snapshot_pos <= snapshot_pos+1;
                        phase <= SNAPSHOT_REQUEST;
                    end
                end
                SNAPSHOT_READ: phase <= SNAPSHOT_COPY;
                SNAPSHOT_COPY: begin
                    response_data[64*snapshot_reply_word +: 64] <= snapshot_read_data;
                    snapshot_reply_word <= snapshot_reply_word+1;
                    snapshot_pos <= snapshot_pos+1;
                    response_code <= RESULT_OK;
                    phase <= snapshot_pos+1 == snapshot_limit ? RESPOND : SNAPSHOT_READ;
                end
                RUN_REQUEST: if (run_valid && run_ready) phase <= RUN_WAIT;
                RUN_WAIT: if (done) begin
                    fault <= fault || error_code != 0;
                    if (error_code != 0) audio_enable <= 0;
                    if (paced) begin
                        quantum_done <= error_code == 0;
                        quantum_cycles <= cycles;
                        quantum_frame <= sync_frame;
                        quantum_revision <= active_revision;
                        phase <= IDLE;
                    end else begin
                        response_code <= error_code == 0 ? RESULT_OK : RESULT_FAULT;
                        response_data[169:0] <= {error_stage, error_code, active_revision, sync_frame, cycles};
                        phase <= RESPOND;
                    end
                end
                RESPOND: if (response_ready) phase <= IDLE;
                default: phase <= IDLE;
            endcase
        end
    end
endmodule
