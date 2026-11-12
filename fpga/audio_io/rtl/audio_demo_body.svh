    localparam [2:0] LOADING=0, RUNNING=1, DRAINING=2, RESETTING=3, FAILED=4;
    reg [2:0] phase;
    reg [31:0] reset_count;
    assign enable = phase == RUNNING;
    assign quiesce = phase == DRAINING || phase == RESETTING;
    assign session_reset = phase == RESETTING;
    assign run_instruction_count = INSTRUCTIONS;
    assign run_state_count = STATES;
    assign run_quantum = Q;
    assign run_chunk_frames = CHUNK_FRAMES;
    assign run_job_budget = JOB_BUDGET;
    assign run_buffers = BUFFERS;
    assign run_inputs = CHANNELS;
    assign run_outputs = CHANNELS;
    assign drain_ready = 0;
    assign arena_base = 0;
    assign audio_cfg_valid = 0;
    assign audio_cfg_space = 0;
    assign audio_cfg_write = 0;
    assign audio_cfg_address = 0;
    assign audio_cfg_write_data = 0;
    assign store_cmd_valid = 0;
    assign store_cmd_opcode = 0;
    assign store_cmd_view = 0;
    assign store_cmd_start = 0;
    assign store_cmd_length = 0;
    assign store_cmd_count = 0;
    assign store_write_valid = 0;
    assign store_write_data = 0;
    assign store_read_ready = 0;
    wire reload_reset = reset || (session_reset && session_reset_ready);
    audio_boot #(.WORDS(BOOT_WORDS),.FILE_NAME(BOOT_FILE)) loader (
        .clk(clk),.reset(reload_reset),.done(boot_done),.fault(boot_fault),
        .command_valid(command_valid),.command_ready(command_ready),.command_opcode(command_opcode),
        .command_index(command_index),.command_data(command_data),.response_valid(response_valid),
        .response_ready(response_ready),.response_error(response_error),
        .cfg_valid(cfg_valid),.cfg_ready(cfg_ready),.cfg_state(cfg_state),.cfg_write(cfg_write),
        .cfg_address(cfg_address),.cfg_write_data(cfg_write_data),.cfg_response_valid(cfg_response_valid),.cfg_response_error(cfg_response_error),
        .store_cfg_valid(store_cfg_valid),.store_cfg_ready(store_cfg_ready),.store_cfg_write(store_cfg_write),
        .store_cfg_space(store_cfg_space),.store_cfg_address(store_cfg_address),.store_cfg_write_data(store_cfg_write_data),
        .store_cfg_response_valid(store_cfg_response_valid),.store_cfg_response_error(store_cfg_response_error)
    );
    audio_events #(.WORDS(EVENT_WORDS),.FILE_NAME(EVENT_FILE)) events (
        .clk(clk),.reset(reload_reset),.enable(boot_done && !boot_fault),
        .valid(control_valid),.ready(control_ready),.data(control_data)
    );
    for (genvar channel=0;channel<CHANNELS;channel=channel+1) begin : generator
        assign rx_samples[channel*32 +: 32] = 32'((channel+1)*100000)+32'(session_frame)*37+1;
    end
    always @(posedge clk) begin
        if (reset) begin
            phase<=LOADING; epoch<=1; reset_count<=0; recoveries<=0;
            last_recovery_reason<=0; last_failure_sequence<=0; last_failure_logical_frame<=0;
            last_error_code<=0; last_error_stage<=0;
        end
        else case (phase)
            LOADING: if (boot_fault) phase<=FAILED; else if (boot_done) phase<=RUNNING;
            RUNNING: if (recovery_requested || restart) begin
                phase<=DRAINING;
                recoveries<=recoveries == 32'hffffffff ? recoveries : recoveries+1;
                last_recovery_reason<=recovery_requested ? recovery_reason : 0;
                last_failure_sequence<=failure_sequence; last_failure_logical_frame<=failure_sequence*Q;
                last_error_code<=error_code; last_error_stage<=error_stage;
            end
            DRAINING: if (memory_quiescent) begin phase<=RESETTING; reset_count<=0; end
            RESETTING: if (reset_count == RESET_CYCLES-1) begin phase<=LOADING; epoch<=epoch+1; end
                else reset_count<=reset_count+1;
            default: begin end
        endcase
    end
