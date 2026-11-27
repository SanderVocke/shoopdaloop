module semantic_mmio_probe #(
    parameter integer LOOPS=128, TRACE_REGISTERS=0
) (
    input wire clk, reset, runtime_reset, input wire [31:0] epoch,
    input wire [11:0] s_axi_awaddr, input wire s_axi_awvalid, output wire s_axi_awready,
    input wire [31:0] s_axi_wdata, input wire [3:0] s_axi_wstrb,
    input wire s_axi_wvalid, output wire s_axi_wready,
    output wire [1:0] s_axi_bresp, output wire s_axi_bvalid, input wire s_axi_bready,
    input wire [11:0] s_axi_araddr, input wire s_axi_arvalid, output wire s_axi_arready,
    output wire [31:0] s_axi_rdata, output wire [1:0] s_axi_rresp,
    output wire s_axi_rvalid, input wire s_axi_rready,
    input wire boundary_valid, output wire boundary_ready,
    input wire [63:0] boundary_frame, input wire render_ready,
    input wire handoff_valid, input wire [31:0] handoff_epoch,
    input wire [63:0] handoff_sequence, input wire handoff_error,
    output wire handoff_pending, faulted, output wire [63:0] active_revision,
    output wire [31:0] renders, input wire pipeline_fault,
    input wire [63:0] diagnostic_sequence,
    input wire trace_valid, output wire trace_ready,
    input wire [31:0] trace_epoch, input wire [63:0] trace_revision, trace_lost,
    input wire [255:0] trace_data,
    input wire capture_valid, output wire capture_ready, input wire capture_initial,
    input wire [63:0] capture_frame, capture_revision, input wire [31:0] capture_quantum,
    output wire snapshot_fault, output wire [63:0] snapshot_last_sequence,
    output wire [1:0] snapshot_ready_mask, output wire [3:0] result_ready_mask,
    output wire [63:0] last_admitted
);
    wire core_reset=reset || runtime_reset;
    wire host_valid, host_ready, host_response_valid;
    wire [2:0] host_operation, bank_state;
    wire [1:0] host_bank, inspect_bank, result_read_bank;
    wire [31:0] host_epoch, host_word, result_read_data;
    wire [3:0] host_strobe, host_receipt, free_mask;
    wire [63:0] host_sequence, next_sequence, cutoff_sequence, groups_attempted;
    wire [8:0] written_words;
    wire [31:0] peak_command_cycles, peak_snapshot_cycles;
    wire result_read_valid, result_response_valid, result_response_error;
    wire [5:0] result_read_word;
    wire [63:0] snapshot_skipped;
    wire snapshot_sequence_exhausted;
    wire snapshot_read_valid, snapshot_read_bank, snapshot_response_valid, snapshot_response_error;
    wire [12:0] snapshot_read_word;
    wire [31:0] snapshot_read_data;
    wire snapshot_ack_valid, snapshot_ack_ready, snapshot_ack_bank, snapshot_ack_response_valid;
    wire [31:0] snapshot_ack_epoch;
    wire [63:0] snapshot_ack_sequence;
    wire [3:0] snapshot_ack_receipt;
    fifo_mmio #(.LOOPS(LOOPS), .SYSTEM_REGISTERS(TRACE_REGISTERS), .TRACE_REGISTERS(TRACE_REGISTERS)) mmio (
        .fault_sequence(diagnostic_sequence), .recovery_state(3'd0),
        .recovery_request(), .recovery_epoch(), .irq(), .abort_request(),
        .service_live(), .service_valid(), .service_operation(), .service_index(), .service_read_index(), .service_word(),
        .service_receipt(4'd0), .service_request_space(1'b0), .service_response_pending(1'b0),
        .service_response_code(2'd0), .service_read_data(32'd0),
        .quantum_done(1'b0), .quantum_cycles(32'd0), .audio_xruns(32'd0),
        .first_fault_epoch(32'd0), .first_fault_cause(32'd0), .first_fault_op(32'd0),
        .first_fault_cycles(32'd0), .first_fault_control_cycles(32'd0), .first_fault_detail(32'd0),
        .first_fault_revision(64'd0), .first_fault_frame(64'd0), .*);
    semantic_group_probe #(.LOOPS(LOOPS)) core (
        .clk(clk), .reset(core_reset), .epoch(epoch), .pipeline_fault(pipeline_fault),
        .host_valid(host_valid), .host_ready(host_ready), .host_operation(host_operation), .host_bank(host_bank),
        .host_epoch(host_epoch), .host_word(host_word), .host_strobe(host_strobe), .host_sequence(host_sequence),
        .host_response_valid(host_response_valid), .host_receipt(host_receipt), .inspect_bank(inspect_bank),
        .bank_state(bank_state), .written_words(written_words), .free_mask(free_mask), .result_ready_mask(result_ready_mask),
        .next_sequence(next_sequence), .last_admitted(last_admitted), .sequence_exhausted(),
        .result_read_valid(result_read_valid), .result_read_bank(result_read_bank), .result_read_word(result_read_word),
        .result_response_valid(result_response_valid), .result_response_error(result_response_error), .result_read_data(result_read_data),
        .boundary_valid(boundary_valid), .boundary_ready(boundary_ready), .boundary_frame(boundary_frame),
        .render_valid(), .render_ready(render_ready), .active_revision(active_revision),
        .active_region(), .active_composites(), .active_instructions(), .active_sync_count(), .active_states(), .active_buffers(),
        .select_valid(), .select_ready(1'b1), .select_error(1'b0), .select_revision(), .select_sequence(), .select_epoch(),
        .select_region(), .select_composites(), .select_instructions(), .select_sync_count(), .select_states(), .select_buffers(),
        .handoff_valid(handoff_valid), .handoff_epoch(handoff_epoch), .handoff_sequence(handoff_sequence), .handoff_error(handoff_error),
        .handoff_pending(handoff_pending), .pending_epoch(), .pending_sequence(), .gain_update_valid(), .gain_update_slot(), .gain_update_bits(),
        .faulted(faulted), .command_phase(), .cutoff_sequence(cutoff_sequence), .groups_attempted(groups_attempted),
        .peak_command_cycles(peak_command_cycles), .current_command_cycles(), .fault_event(), .fault_reason(),
        .fault_sequence(), .fault_frame(), .fault_opcode(), .allow_memory(1'b1), .inject_read_error(1'b0), .inject_write_error(1'b0),
        .cfg_valid(1'b0), .cfg_write(1'b0), .cfg_state(1'b0), .cfg_address(16'd0), .cfg_write_data(129'd0),
        .cfg_response_valid(), .cfg_read_data(), .renders(renders), .gain_updates(), .memory_writes(),
        .last_gain_bits(), .last_gain_slot(), .main_result_tail(), .final_result_selected()
    );
    wire memory_valid, metadata_valid;
    wire [15:0] memory_address, metadata_slot;
    reg memory_response_valid, metadata_response_valid;
    reg [128:0] memory_read_data, metadata_read_data;
    always @(posedge clk) begin
        memory_response_valid<=!core_reset && memory_valid;
        metadata_response_valid<=!core_reset && metadata_valid;
        if (memory_valid) memory_read_data<={65'd0,(64'h0123456700000000 | {48'd0,memory_address})};
        if (metadata_valid) metadata_read_data<={1'b1,
            (64'h1020304050600000 | {48'd0,metadata_slot}),
            (32'h70000000 | {16'd0,metadata_slot}), (32'h3f800000 | {16'd0,metadata_slot})};
    end
    fifo_snapshots #(.LOOPS(LOOPS)) snapshots (
        .clk(clk), .reset(core_reset), .epoch(epoch), .fault(pipeline_fault || faulted),
        .capture_valid(capture_valid), .capture_ready(capture_ready), .capture_initial(capture_initial),
        .capture_frame(capture_frame), .capture_revision(capture_revision), .capture_quantum(capture_quantum),
        .memory_valid(memory_valid), .memory_ready(1'b1), .memory_address(memory_address),
        .memory_response_valid(memory_response_valid), .memory_response_error(1'b0), .memory_read_data(memory_read_data),
        .metadata_valid(metadata_valid), .metadata_ready(1'b1), .metadata_slot(metadata_slot),
        .metadata_response_valid(metadata_response_valid), .metadata_response_error(1'b0), .metadata_read_data(metadata_read_data),
        .ready_mask(snapshot_ready_mask), .free_mask(), .sequence0(), .sequence1(), .last_sequence(snapshot_last_sequence),
        .skipped(snapshot_skipped), .sequence_exhausted(snapshot_sequence_exhausted), .capture_fault(snapshot_fault), .peak_cycles(peak_snapshot_cycles),
        .read_valid(snapshot_read_valid), .read_bank(snapshot_read_bank), .read_word(snapshot_read_word),
        .response_valid(snapshot_response_valid), .response_error(snapshot_response_error), .read_data(snapshot_read_data),
        .ack_valid(snapshot_ack_valid), .ack_ready(snapshot_ack_ready), .ack_bank(snapshot_ack_bank),
        .ack_epoch(snapshot_ack_epoch), .ack_sequence(snapshot_ack_sequence),
        .ack_response_valid(snapshot_ack_response_valid), .ack_receipt(snapshot_ack_receipt)
    );
endmodule
