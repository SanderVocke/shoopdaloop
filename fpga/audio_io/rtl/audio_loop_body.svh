    wire io_process_valid, io_process_ready, io_busy, fenced, control_fault;
    wire queued_command_valid, queued_command_ready, queued_response_ready;
    wire [7:0] queued_command_opcode;
    wire [31:0] queued_command_index;
    wire [639:0] queued_command_data;
    wire queued_cfg_valid, queued_cfg_ready, queued_cfg_state, queued_cfg_write;
    wire [15:0] queued_cfg_address;
    wire [128:0] queued_cfg_write_data;
    wire [31:0] process_epoch;
    wire unused_epoch = &{1'b0,process_epoch};
    assign session_reset_ready = quiesce && memory_quiescent && !enable;
    wire accepted_reset = session_reset && session_reset_ready;
    assign core_reset = reset || accepted_reset;
    assign core_maintenance_enable = enable && !quiesce && !recovery_requested;
    assign core_run_valid = io_process_valid && fenced && enable && !quiesce;
    assign io_process_ready = run_ready && fenced && enable && !quiesce;
    assign core_command_valid = enable ? queued_command_valid : command_valid;
    assign core_command_opcode = enable ? queued_command_opcode : command_opcode;
    assign core_command_index = enable ? queued_command_index : command_index;
    assign core_command_data = enable ? queued_command_data : command_data;
    assign command_ready = core_command_ready && !enable;
    assign queued_command_ready = core_command_ready && enable;
    assign response_valid = core_response_valid && !enable;
    assign response_error = core_response_error;
    assign response_data = core_response_data;
    assign core_response_ready = enable ? queued_response_ready : response_ready;
    assign core_cfg_valid = enable ? queued_cfg_valid : cfg_valid;
    assign core_cfg_state = enable ? queued_cfg_state : cfg_state;
    assign core_cfg_write = enable ? queued_cfg_write : cfg_write;
    assign core_cfg_address = enable ? queued_cfg_address : cfg_address;
    assign core_cfg_write_data = enable ? queued_cfg_write_data : cfg_write_data;
    assign cfg_ready = core_cfg_ready && !enable;
    assign queued_cfg_ready = core_cfg_ready && enable;
    assign cfg_response_valid = core_cfg_response_valid && !enable;
    assign cfg_response_error = core_cfg_response_error;
    assign cfg_read_data = core_cfg_read_data;

    audio_control #(.DEPTH(CONTROL_DEPTH)) controls (
        .clk(clk),.reset(core_reset),.enable(enable && !recovery_requested && !quiesce),
        .sequence_number(process_sequence),.busy(io_busy),.run_start(core_run_valid && run_ready),
        .fenced(fenced),.fault(control_fault),.control_valid(control_valid),.control_ready(control_ready),.control_data(control_data),
        .command_valid(queued_command_valid),.command_ready(queued_command_ready),
        .command_opcode(queued_command_opcode),.command_index(queued_command_index),.command_data(queued_command_data),
        .response_valid(core_response_valid),.response_ready(queued_response_ready),.response_error(core_response_error),
        .cfg_valid(queued_cfg_valid),.cfg_ready(queued_cfg_ready),.cfg_state(queued_cfg_state),.cfg_write(queued_cfg_write),
        .cfg_address(queued_cfg_address),.cfg_write_data(queued_cfg_write_data),
        .cfg_response_valid(core_cfg_response_valid),.cfg_response_error(core_cfg_response_error)
    );
    audio_io #(.Q(MAX_FRAMES),.P(P),.INPUTS(INPUT_PORTS),.OUTPUTS(OUTPUT_PORTS),
        .RX_BANKS(RX_BANKS),.TX_BANKS(TX_BANKS),.WATCHDOG_CYCLES(WATCHDOG_CYCLES)) io (
        .clk(clk),.audio_clk(audio_clk),.reset(reset),.session_reset(accepted_reset),.enable(enable),.epoch(epoch),
        .frame_tick(frame_tick),.frame_prepare(frame_prepare),.rx_samples(rx_samples),.tx_samples(tx_samples),.tx_valid(tx_valid),
        .audio_frame(audio_frame),.session_frame(session_frame),.process_valid(io_process_valid),.process_ready(io_process_ready),
        .process_sequence(process_sequence),.process_epoch(process_epoch),.process_done(done),
        .process_fault(faulted || control_fault || memory_fault || (enable &&
            (run_quantum != MAX_FRAMES || run_chunk_frames != CHUNK_FRAMES || run_inputs != INPUT_PORTS || run_outputs != OUTPUT_PORTS))),
        .input_read(sample_input_read),.input_address(sample_input_address),.input_data(sample_input_data),
        .output_write(sample_output_write),.output_address(sample_output_address),.output_data(sample_output_data),
        .recovery_requested(recovery_requested),.recovery_reason(recovery_reason),.failure_sequence(failure_sequence),
        .processed_quanta(processed_quanta),.played_quanta(played_quanta),.missed_deadlines(missed_deadlines),
        .discarded_quanta(discarded_quanta),.input_losses(input_losses),.rx_high_water(rx_high_water),.tx_high_water(tx_high_water),.busy(io_busy)
    );
    initial if (MAX_FRAMES != CHUNK_FRAMES && MAX_FRAMES % CHUNK_FRAMES != 0)
        $fatal(1,"Quantum must contain whole media chunks");

    reg [15:0] outstanding_reads, outstanding_writes;
    reg ar_exposed, aw_exposed;
    wire isolate = (quiesce && reclaim_quiescent) || recovery_requested;
    assign m_axi_arvalid = core_m_axi_arvalid && (!isolate || ar_exposed);
    assign core_m_axi_arready = m_axi_arready && (!isolate || ar_exposed);
    assign m_axi_awvalid = core_m_axi_awvalid && (!isolate || aw_exposed);
    assign core_m_axi_awready = m_axi_awready && (!isolate || aw_exposed);
    assign m_axi_wvalid = core_m_axi_wvalid && outstanding_writes != 0;
    assign core_m_axi_wready = m_axi_wready && outstanding_writes != 0;
    assign memory_quiescent = (recovery_requested || reclaim_quiescent)
        && outstanding_reads == 0 && outstanding_writes == 0 && !ar_exposed && !aw_exposed;
    always @(posedge clk) begin
        if (reset) begin outstanding_reads<=0; outstanding_writes<=0; ar_exposed<=0; aw_exposed<=0; end
        else begin
            if (m_axi_arvalid) ar_exposed<=!m_axi_arready;
            if (m_axi_awvalid) aw_exposed<=!m_axi_awready;
            case ({m_axi_arvalid && m_axi_arready,m_axi_rvalid && m_axi_rready && m_axi_rlast})
                2'b10: outstanding_reads<=outstanding_reads+1'b1;
                2'b01: outstanding_reads<=outstanding_reads-1'b1;
                default: begin end
            endcase
            case ({m_axi_awvalid && m_axi_awready,m_axi_bvalid && m_axi_bready})
                2'b10: outstanding_writes<=outstanding_writes+1'b1;
                2'b01: outstanding_writes<=outstanding_writes-1'b1;
                default: begin end
            endcase
        end
    end
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
