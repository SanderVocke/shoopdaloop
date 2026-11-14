    localparam [2:0] IDLE=0, RESETTING=1, LOADING=2, RUNNING=3, DRAINING=4, STOPPED=5;
    reg [2:0] phase;
    reg [3:0] reset_count;
    reg start_previous, stop_previous, snapshot_previous;
    wire start_edge = start && !start_previous;
    wire stop_edge = stop && !stop_previous;
    reg stop_pending, failed, inflight;
    reg [7:0] failure_code;
    reg [1:0] failure_stage;
    reg [31:0] limit, warmup, gap, watchdog, gap_remaining, run_cycles;
    reg [63:0] quantum, measured, total_cycles, wall_cycles;
    reg [31:0] last_cycles, min_cycles, max_cycles;
    reg [63:0] read_bursts, write_bursts, read_bytes, write_bytes;
    reg [63:0] address_stalls, response_stalls, output_words, output_digest;
    wire boot_done, boot_fault, control_fault, run_permitted, batch_fault, image_fault;
    wire image_busy, image_override, image_response_valid;
    wire [3:0] image_response_error;
    wire [128:0] image_response_data;
    wire [31:0] image_instructions, image_states, image_jobs, image_buffers;
    wire action_busy, action_fault, action_done, action_cfg_valid, action_cfg_ready, action_cfg_state, action_response_valid;
    wire [7:0] action_cfg_opcode;
    wire [15:0] action_cfg_address;
    wire [639:0] action_cfg_data;
    wire [3:0] action_response_error;
    wire [831:0] action_response_data;
    reg cut_reserved, cut_ready;
    wire action_start = running && run_permitted && !inflight && !action_busy && !cut_reserved
        && !stop_pending && !stop_edge && !failure && gap_remaining == 0;
    always @(posedge clk) begin
        if (core_reset) begin cut_reserved<=0; cut_ready<=0; end
        else begin
            if (action_start) cut_reserved<=1;
            if (action_done) cut_ready<=1;
            if (run_pulse) begin cut_reserved<=0; cut_ready<=0; end
        end
    end
    assign control_fault = batch_fault || image_fault || action_fault;
    wire control_result_valid;
    wire [1087:0] control_result_data;
    wire unused_control_results = &{1'b0,control_result_valid,control_result_data};
    wire memory_quiescent;
    reg read_outstanding, write_outstanding, ar_exposed, aw_exposed;
    reg [2:0] read_size, write_size;
    wire isolate = (phase == DRAINING && (failed || faulted || memory_fault || reclaim_quiescent)) || phase == STOPPED;
    wire running = phase == RUNNING;
    wire timed_out = inflight && watchdog != 0 && run_cycles >= watchdog;
    wire failure = boot_fault || control_fault || memory_fault || faulted || timed_out;
    wire [31:0] completed_cycles = run_cycles+1;
    assign core_reset = reset || phase == RESETTING;
    assign run_valid = running && run_permitted && cut_ready && !inflight && !stop_pending && !stop_edge
        && !failure && gap_remaining == 0;
    assign run_pulse = run_valid && run_ready;
    assign done_pulse = done;
    assign debug_quantum = quantum;
    assign debug_cycles = run_cycles;
    assign status = {11'b0,memory_fault,control_fault,boot_fault,failure_stage,failure_code,
                     busy,inflight,boot_done,failed,memory_quiescent,phase};
    assign run_instruction_count = image_override ? image_instructions : INSTRUCTION_COUNT;
    assign run_state_count = image_override ? image_states : STATE_COUNT;
    assign run_quantum = MAX_FRAMES;
    assign run_chunk_frames = CHUNK_FRAMES;
    assign run_job_budget = image_override ? image_jobs : JOB_BUDGET;
    assign run_buffers = image_override ? image_buffers : BUFFER_COUNT;
    assign run_inputs = INPUT_PORTS;
    assign run_outputs = OUTPUT_PORTS;
    assign job_enable = 1;
    assign maintenance_enable = running;
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

    reg [31:0] generated_sample;
    assign sample_input_data = generated_sample;
    always @(posedge clk) begin
        if (core_reset) generated_sample <= 0;
        else if (sample_input_read) generated_sample <= (sample_input_address+1)*37 ^ quantum[31:0];
    end

    wire boot_command_valid, boot_response_ready, boot_cfg_valid, boot_cfg_state, boot_cfg_write;
    wire [7:0] boot_command_opcode;
    wire [31:0] boot_command_index;
    wire [639:0] boot_command_data;
    wire [15:0] boot_cfg_address;
    wire [128:0] boot_cfg_write_data;
    wire live_command_valid, live_response_ready, live_cfg_valid, live_cfg_state, live_cfg_write;
    wire [7:0] live_command_opcode;
    wire [31:0] live_command_index;
    wire [639:0] live_command_data;
    wire [15:0] live_cfg_address;
    wire [128:0] live_cfg_write_data;
    wire queued_cfg_valid, queued_cfg_ready, queued_cfg_state, queued_cfg_write;
    wire [7:0] queued_cfg_opcode;
    wire [15:0] queued_cfg_address;
    wire [639:0] queued_cfg_data;
    wire unused_image_control = &{1'b0,image_busy,queued_cfg_write};
    wire control_valid, control_ready;
    wire [745:0] control_data;
    wire loading = phase == LOADING;
    assign command_valid = loading ? boot_command_valid : live_command_valid;
    assign command_opcode = loading ? boot_command_opcode : live_command_opcode;
    assign command_index = loading ? boot_command_index : live_command_index;
    assign command_data = loading ? boot_command_data : live_command_data;
    assign response_ready = loading ? boot_response_ready : live_response_ready;
    assign cfg_valid = loading ? boot_cfg_valid : live_cfg_valid;
    assign cfg_state = loading ? boot_cfg_state : live_cfg_state;
    assign cfg_write = loading ? boot_cfg_write : live_cfg_write;
    assign cfg_address = loading ? boot_cfg_address : live_cfg_address;
    assign cfg_write_data = loading ? boot_cfg_write_data : live_cfg_write_data;
    audio_boot #(.WORDS(BOOT_WORDS),.FILE_NAME(BOOT_FILE)) loader (
        .clk(clk),.reset(core_reset || phase == IDLE),.done(boot_done),.fault(boot_fault),
        .command_valid(boot_command_valid),.command_ready(command_ready),.command_opcode(boot_command_opcode),
        .command_index(boot_command_index),.command_data(boot_command_data),.response_valid(response_valid),
        .response_ready(boot_response_ready),.response_error(response_error),
        .cfg_valid(boot_cfg_valid),.cfg_ready(cfg_ready),.cfg_state(boot_cfg_state),.cfg_write(boot_cfg_write),
        .cfg_address(boot_cfg_address),.cfg_write_data(boot_cfg_write_data),
        .cfg_response_valid(cfg_response_valid),.cfg_response_error(cfg_response_error),
        .store_cfg_valid(store_cfg_valid),.store_cfg_ready(store_cfg_ready),.store_cfg_write(store_cfg_write),
        .store_cfg_space(store_cfg_space),.store_cfg_address(store_cfg_address),.store_cfg_write_data(store_cfg_write_data),
        .store_cfg_response_valid(store_cfg_response_valid),.store_cfg_response_error(store_cfg_response_error)
    );
    audio_events #(.WORDS(EVENT_WORDS),.FILE_NAME(EVENT_FILE)) events (
        .clk(clk),.reset(core_reset),.enable(running),.valid(control_valid),.ready(control_ready),.data(control_data)
    );
    wire unused_action_resident_valid;
    wire [7:0] unused_action_resident_opcode;
    wire [511:0] unused_action_resident_data;
    wire [63:0] unused_action_resident_sequence;
    wire [63:0] unused_action_resident_frame;
    wire [47:0] unused_action_resident_reference;
    wire [15:0] unused_action_resident_slot;
    wire [31:0] unused_action_resident_generation;
    wire [1:0] unused_action_resident_mode;
    wire unused_action_lookup_ready;
    wire unused_action_lookup_done;
    wire [3:0] unused_action_lookup_error;
    wire [767:0] unused_action_lookup_data;
    wire unused_action_update_ready;
    wire unused_action_update_done;
    wire [3:0] unused_action_update_error;
    wire unused_action_private=&{1'b0,unused_action_resident_valid,unused_action_resident_opcode,unused_action_resident_data,unused_action_resident_sequence,unused_action_resident_frame,unused_action_resident_reference,unused_action_resident_slot,unused_action_resident_generation,unused_action_resident_mode,unused_action_lookup_ready,unused_action_lookup_done,unused_action_lookup_error,unused_action_lookup_data,unused_action_update_ready,unused_action_update_done,unused_action_update_error};
    audio_actions #(.SLOTS(128),.QUANTUM(MAX_FRAMES),.ACTIVATION_WORK(10*STATE_DEPTH+4*PROGRAM_DEPTH+32)) actions (
        .resident_valid(unused_action_resident_valid),
        .resident_opcode(unused_action_resident_opcode),
        .resident_data(unused_action_resident_data),
        .resident_sequence(unused_action_resident_sequence),
        .resident_frame(unused_action_resident_frame),
        .resident_reference(unused_action_resident_reference),
        .resident_slot(unused_action_resident_slot),
        .resident_generation(unused_action_resident_generation),
        .resident_mode(unused_action_resident_mode),
        .lookup_ready(unused_action_lookup_ready),
        .lookup_done(unused_action_lookup_done),
        .lookup_error(unused_action_lookup_error),
        .lookup_data(unused_action_lookup_data),
        .update_ready(unused_action_update_ready),
        .update_done(unused_action_update_done),
        .update_error(unused_action_update_error),
        .cut_mode(2'b0),
        .resident_ready(1'b0),
        .resident_done(1'b0),
        .resident_pending(1'b0),
        .resident_error(4'b0),
        .resident_result(512'b0),
        .lookup_valid(1'b0),
        .lookup_slot(16'b0),
        .update_valid(1'b0),
        .update_slot(16'b0),
        .update_generation(32'b0),
        .update_data(768'b0),

        .clk(clk),.reset(core_reset),.current_frame(quantum*64'(MAX_FRAMES)),
        .cut_start(action_start),.cut_done(action_done),.busy(action_busy),.fault(action_fault),
        .request_valid(queued_cfg_valid),.request_ready(queued_cfg_ready),.request_opcode(queued_cfg_opcode),
        .request_state(queued_cfg_state),.request_address(queued_cfg_address),.request_data(queued_cfg_data),
        .response_valid(action_response_valid),.response_error(action_response_error),.response_data(action_response_data),
        .cfg_valid(action_cfg_valid),.cfg_ready(action_cfg_ready),.cfg_opcode(action_cfg_opcode),
        .cfg_state(action_cfg_state),.cfg_address(action_cfg_address),.cfg_data(action_cfg_data),
        .cfg_response_valid(image_response_valid),.cfg_response_error(image_response_error),.cfg_response_data(image_response_data)
    );
    audio_image #(.PROGRAM_DEPTH(PROGRAM_DEPTH),.STATE_DEPTH(STATE_DEPTH),.JOB_DEPTH(JOB_DEPTH),.BUFFERS(BUFFERS)) images (
        .clk(clk),.reset(core_reset),.execution_idle(!busy && !inflight),.current_state_count(32'(STATE_COUNT)),
        .request_valid(action_cfg_valid),.request_ready(action_cfg_ready),.request_opcode(action_cfg_opcode),
        .request_state(action_cfg_state),.request_address(action_cfg_address),.request_data(action_cfg_data),
        .busy(image_busy),.response_valid(image_response_valid),.response_error(image_response_error),.response_data(image_response_data),.fault(image_fault),
        .active_override(image_override),.active_instructions(image_instructions),.active_states(image_states),.active_jobs(image_jobs),.active_buffers(image_buffers),
        .cfg_valid(live_cfg_valid),.cfg_ready(cfg_ready),.cfg_state(live_cfg_state),.cfg_write(live_cfg_write),
        .cfg_address(live_cfg_address),.cfg_write_data(live_cfg_write_data),.cfg_response_valid(cfg_response_valid),
        .cfg_response_error(cfg_response_error),.cfg_read_data(cfg_read_data)
    );
    audio_control #(.DEPTH(128),.ACTIVATION_WORK(10*STATE_DEPTH+4*PROGRAM_DEPTH+32)) controls (
        .clk(clk),.reset(core_reset),.enable(running),.sequence_number(quantum),.epoch(32'd1),
        .busy(busy || inflight || action_busy || cut_reserved),.run_start(run_pulse),.run_permitted(run_permitted),.fault(batch_fault),
        .admission_blocked(1'b0),
        .control_valid(control_valid),.control_ready(control_ready),.control_data(control_data),
        .command_valid(live_command_valid),.command_ready(command_ready),.command_opcode(live_command_opcode),
        .command_index(live_command_index),.command_data(live_command_data),.response_valid(response_valid),
        .response_ready(live_response_ready),.response_error(response_error),
        .response_data(response_data),.result_valid(control_result_valid),.result_ready(1'b1),.result_data(control_result_data),
        .cfg_valid(queued_cfg_valid),.cfg_ready(queued_cfg_ready),.cfg_state(queued_cfg_state),.cfg_write(queued_cfg_write),
        .cfg_opcode(queued_cfg_opcode),.cfg_address(queued_cfg_address),.cfg_write_data(queued_cfg_data),
        .cfg_response_valid(action_response_valid),.cfg_response_error(action_response_error),.cfg_read_data(action_response_data)
    );

    assign m_axi_arvalid = core_m_axi_arvalid && (!isolate || ar_exposed);
    assign core_m_axi_arready = m_axi_arready && (!isolate || ar_exposed);
    assign m_axi_awvalid = core_m_axi_awvalid && (!isolate || aw_exposed);
    assign core_m_axi_awready = m_axi_awready && (!isolate || aw_exposed);
    assign m_axi_wvalid = core_m_axi_wvalid && write_outstanding;
    assign core_m_axi_wready = m_axi_wready && write_outstanding;
    assign memory_quiescent = (failed || faulted || memory_fault || reclaim_quiescent)
        && !read_outstanding && !write_outstanding && !ar_exposed && !aw_exposed;
    assign m_axi_awid = core_m_axi_awid;
    assign m_axi_awaddr = core_m_axi_awaddr;
    assign m_axi_awlen = core_m_axi_awlen;
    assign m_axi_awsize = core_m_axi_awsize;
    assign m_axi_awburst = core_m_axi_awburst;
    assign m_axi_wid = core_m_axi_wid;
    assign m_axi_wdata = core_m_axi_wdata;
    assign m_axi_wstrb = core_m_axi_wstrb;
    assign m_axi_wlast = core_m_axi_wlast;
    assign core_m_axi_bid = m_axi_bid;
    assign core_m_axi_bresp = m_axi_bresp;
    assign core_m_axi_bvalid = m_axi_bvalid;
    assign m_axi_bready = core_m_axi_bready;
    assign m_axi_arid = core_m_axi_arid;
    assign m_axi_araddr = core_m_axi_araddr;
    assign m_axi_arlen = core_m_axi_arlen;
    assign m_axi_arsize = core_m_axi_arsize;
    assign m_axi_arburst = core_m_axi_arburst;
    assign core_m_axi_rid = m_axi_rid;
    assign core_m_axi_rdata = m_axi_rdata;
    assign core_m_axi_rresp = m_axi_rresp;
    assign core_m_axi_rlast = m_axi_rlast;
    assign core_m_axi_rvalid = m_axi_rvalid;
    assign m_axi_rready = core_m_axi_rready;

    always @(posedge clk) begin
        if (reset) begin
            read_outstanding<=0; write_outstanding<=0; ar_exposed<=0; aw_exposed<=0;
            read_size<=0; write_size<=0;
        end else begin
            if (m_axi_arvalid) ar_exposed<=!m_axi_arready;
            if (m_axi_awvalid) aw_exposed<=!m_axi_awready;
            if (m_axi_arvalid && m_axi_arready) begin read_outstanding<=1; read_size<=m_axi_arsize; end
            if (m_axi_rvalid && m_axi_rready && m_axi_rlast) read_outstanding<=0;
            if (m_axi_awvalid && m_axi_awready) begin write_outstanding<=1; write_size<=m_axi_awsize; end
            if (m_axi_bvalid && m_axi_bready) write_outstanding<=0;
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            phase<=IDLE; reset_count<=0; start_previous<=0; stop_previous<=0; snapshot_previous<=0;
            stop_pending<=0; failed<=0; inflight<=0; failure_code<=0; failure_stage<=0;
            limit<=0; warmup<=0; gap<=0; watchdog<=0; gap_remaining<=0; run_cycles<=0;
            quantum<=0; measured<=0; total_cycles<=0; wall_cycles<=0;
            last_cycles<=0; min_cycles<=32'hffffffff; max_cycles<=0;
            read_bursts<=0; write_bursts<=0; read_bytes<=0; write_bytes<=0;
            address_stalls<=0; response_stalls<=0; output_words<=0; output_digest<=0; statistics<=0;
        end else begin
            start_previous<=start; stop_previous<=stop; snapshot_previous<=snapshot;
            if (start_edge && (phase == IDLE || phase == STOPPED) && memory_quiescent) begin
                phase<=RESETTING; reset_count<=0; stop_pending<=0; failed<=0; inflight<=0;
                failure_code<=0; failure_stage<=0;
                limit<=quanta_limit; warmup<=warmup_quanta; gap<=gap_cycles; watchdog<=watchdog_cycles;
                gap_remaining<=0; run_cycles<=0; quantum<=0; measured<=0; total_cycles<=0; wall_cycles<=0;
                last_cycles<=0; min_cycles<=32'hffffffff; max_cycles<=0;
                read_bursts<=0; write_bursts<=0; read_bytes<=0; write_bytes<=0;
                address_stalls<=0; response_stalls<=0; output_words<=0; output_digest<=0;
            end else begin
                case (phase)
                    RESETTING: if (reset_count == 7) phase<=LOADING; else reset_count<=reset_count+1;
                    LOADING: if (stop_edge) phase<=DRAINING;
                    else if (boot_fault) begin
                        failed<=1; failure_code<=8'hf3; phase<=DRAINING;
                    end else if (boot_done) phase<=RUNNING;
                    RUNNING: begin
                        if (quantum >= {32'b0,warmup}) wall_cycles<=wall_cycles+1;
                        if (gap_remaining != 0) gap_remaining<=gap_remaining-1;
                        if (stop_edge) stop_pending<=1;
                        if (run_pulse) begin inflight<=1; run_cycles<=0; end
                        else if (inflight) run_cycles<=run_cycles+1;
                        if (done && inflight) begin
                            inflight<=0; quantum<=quantum+1; gap_remaining<=gap;
                            if (quantum >= {32'b0,warmup} && !failure) begin
                                measured<=measured+1; total_cycles<=total_cycles+{32'b0,completed_cycles};
                                last_cycles<=completed_cycles;
                                if (completed_cycles < min_cycles) min_cycles<=completed_cycles;
                                if (completed_cycles > max_cycles) max_cycles<=completed_cycles;
                            end
                            if ((limit != 0 && quantum+1 >= {32'b0,limit}) || stop_pending || stop_edge)
                                phase<=DRAINING;
                        end
                        if ((stop_pending || stop_edge) && !inflight) phase<=DRAINING;
                        if (failure) begin
                            failed<=1; phase<=DRAINING; failure_stage<=error_stage;
                            failure_code<=timed_out ? 8'hf4 : control_fault ? 8'hf2 : memory_fault ? 8'hf1 : error_code;
                        end
                    end
                    DRAINING: if (memory_quiescent) begin phase<=STOPPED; inflight<=0; end
                    default: begin end
                endcase
                if (running || phase == DRAINING) begin
                    if (m_axi_arvalid && m_axi_arready) read_bursts<=read_bursts+1;
                    if (m_axi_awvalid && m_axi_awready) write_bursts<=write_bursts+1;
                    if (m_axi_rvalid && m_axi_rready) read_bytes<=read_bytes+(64'd1 << read_size);
                    if (m_axi_wvalid && m_axi_wready) write_bytes<=write_bytes+(64'd1 << write_size);
                    if ((m_axi_arvalid && !m_axi_arready) || (m_axi_awvalid && !m_axi_awready)) address_stalls<=address_stalls+1;
                    if ((m_axi_rvalid && !m_axi_rready) || (m_axi_bvalid && !m_axi_bready)) response_stalls<=response_stalls+1;
                    if (sample_output_write) begin
                        output_words<=output_words+1;
                        output_digest<={output_digest[62:0],output_digest[63]} ^ {sample_output_address,sample_output_data};
                    end
                end
            end
            if ((snapshot && !snapshot_previous) || phase == STOPPED) begin
                statistics <= {output_digest,output_words,response_stalls,address_stalls,
                               write_bytes,read_bytes,write_bursts,read_bursts,wall_cycles,
                               32'b0,max_cycles,32'b0,min_cycles,32'b0,last_cycles,total_cycles,measured,quantum,32'b0,status};
            end
        end
    end
    wire unused = &{1'b0,cut_snapshot_valid,cut_snapshot_data,reclaim_pages_used,reclaim_pages_available,reclaim_pages_freed,reclaim_capacity_failures,reclaim_pending,reclaim_high_water,reclaim_references,reclaim_active,reclaim_released,reclaim_block,reclaim_blocks,reclaim_freed,reclaim_cycles,EXTERNAL_IO,buffer_ready,cfg_read_data,audio_cfg_ready,audio_cfg_response_valid,
        audio_cfg_response_error,audio_cfg_read_data,monitor_valid,monitor_ready,monitor_data,queued_jobs,
        drain_valid,response_data,transition_busy,store_cmd_ready,store_busy,store_done,store_error_code,
        store_result_view,store_result_empty,store_write_ready,store_read_valid,store_read_data,
        store_cfg_read_data,state_busy,state_done,jobs_busy,jobs_done};
