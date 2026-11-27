module fifo_group_consumer #(
    parameter integer LOOPS=128, STATES=512, PROGRAM_DEPTH=1024, BUFFERS=512
) (
    input wire clk, input wire reset, input wire [31:0] epoch,
    input wire pipeline_fault,
    input wire host_valid, output wire host_ready,
    input wire [2:0] host_operation, input wire [1:0] host_bank,
    input wire [31:0] host_epoch, host_word,
    input wire [3:0] host_strobe, input wire [63:0] host_sequence,
    output wire host_response_valid, output wire [3:0] host_receipt,
    input wire [1:0] inspect_bank,
    output wire [2:0] bank_state, output wire [8:0] written_words,
    output wire [3:0] free_mask, result_ready_mask,
    output wire [63:0] next_sequence, last_admitted,
    output wire sequence_exhausted,
    input wire result_read_valid, input wire [1:0] result_read_bank,
    input wire [5:0] result_read_word,
    output wire result_response_valid, result_response_error,
    output wire [31:0] result_read_data,
    input wire boundary_valid, output wire boundary_ready,
    input wire [63:0] boundary_frame,
    output wire render_valid, input wire render_ready,
    output reg [63:0] active_revision,
    output reg active_region, active_composites,
    output reg [15:0] active_instructions, active_sync_count, active_states, active_buffers,
    output wire select_valid, input wire select_ready, input wire select_error,
    output wire [63:0] select_revision, select_sequence,
    output wire [31:0] select_epoch,
    output wire select_region, select_composites,
    output wire [15:0] select_instructions, select_sync_count, select_states, select_buffers,
    input wire handoff_valid, input wire [31:0] handoff_epoch,
    input wire [63:0] handoff_sequence, input wire handoff_error,
    output wire handoff_pending,
    output wire [31:0] pending_epoch, output wire [63:0] pending_sequence,
    output wire memory_valid, input wire memory_ready,
    output wire memory_write, memory_state,
    output wire [15:0] memory_address, output wire [128:0] memory_write_data,
    input wire memory_response_valid, memory_response_error,
    input wire [128:0] memory_read_data,
    output reg gain_update_valid, output reg [15:0] gain_update_slot,
    output reg [31:0] gain_update_bits,
    output reg faulted, output wire command_phase,
    output reg [63:0] cutoff_sequence, groups_attempted,
    output reg [31:0] peak_command_cycles, output wire [31:0] current_command_cycles,
    output wire fault_event, output wire [7:0] fault_reason,
    output wire [63:0] fault_sequence, fault_frame, output wire [31:0] fault_opcode
);
    localparam [7:0] APPLIED=1, DEADLINE=2, STALE=3, TARGET=4, INVALID=5,
        UNSUPPORTED=6, CANNOT=7, COMPOSITE=23;
    typedef enum logic [3:0] {
        IDLE, HEADER_READ, HEADER_CHECK, SCAN, SCAN_CHECK, COMMAND_READ,
        COMMAND_CHECK, KERNEL_START, KERNEL_WAIT, SELECT, WRITE_CODE,
        WRITE_HEADER, FINISH, RENDER, FAILED, HALTED
    } phase_t;
    phase_t phase;
    reg [1:0] attempt_bank;
    reg [31:0] attempt_epoch;
    reg [63:0] attempt_sequence, attempt_frame, initial_revision;
    reg [63:0] expected_graph, expected_topology, latest_frame;
    reg has_deadline, bad_placement, had_switch, selected_switch, group_active;
    reg [5:0] command_count, command_index, stream_sent, response_index;
    reg [3:0] header_index;
    reg [383:0] packet;
    reg [7:0] header_code, code;
    reg [31:0] command_cycles;
    reg pending, pending_finished, fence_seen, fence_failed;
    reg [1:0] pending_bank;
    reg [31:0] pending_incarnation;
    reg [63:0] pending_group, pending_frame;
    wire [31:0] kind=packet[31:0];
    wire [31:0] refusal=packet[159:128];
    wire [159:0] args=packet[351:192];
    wire [15:0] slot=packet[47:32], gain_address=packet[63:48];
    wire [5:0] read_limit=phase==HEADER_READ ? 6'd16 : phase==SCAN ? command_count : 6'd12;
    wire [8:0] stream_offset={3'd0,stream_sent}*9'd12;
    wire [8:0] command_offset={3'd0,command_index}*9'd12;
    wire head_valid, take;
    wire [1:0] head_bank;
    wire [63:0] head_sequence;
    wire command_read_valid=(phase==HEADER_READ || phase==SCAN || phase==COMMAND_READ)
        && stream_sent<read_limit && !faulted;
    wire [1:0] command_read_bank=attempt_bank;
    wire [8:0] command_read_word=phase==HEADER_READ ? {3'd0,stream_sent}
        : phase==SCAN ? 9'd16+stream_offset : 9'd16+command_offset+{3'd0,stream_sent};
    wire command_response_valid, command_response_error;
    wire [31:0] command_read_data;
    wire bank_fault;
    wire kernel_ready, kernel_result_valid, kernel_write_fault, kernel_memory_valid;
    wire [3:0] kernel_code;
    wire [31:0] kernel_cycles;
    wire handoff_matches=handoff_valid && pending && handoff_epoch==pending_incarnation
        && handoff_sequence==pending_group;
    wire handoff_failure=(handoff_matches && handoff_error) || (pending && fence_seen && fence_failed);
    wire kernel_memory_ready=memory_ready && !faulted && !pipeline_fault && !handoff_failure;
    wire finalize=pending && pending_finished && fence_seen;
    wire finalize_selected=finalize && !fence_failed;
    wire finalize_write=finalize_selected && !faulted && !handoff_failure;
    wire main_write=(phase==WRITE_CODE || (phase==WRITE_HEADER && (!selected_switch || header_index!=15)))
        && !faulted && !handoff_failure;
    wire result_write_valid=finalize_write || main_write;
    wire [1:0] result_write_bank=finalize_selected ? pending_bank : attempt_bank;
    wire [5:0] result_write_word=finalize_selected ? 6'd15
        : phase==WRITE_CODE ? 6'd16+command_index : {2'd0,header_index};
    reg [31:0] header_word;
    wire [31:0] result_write_data=finalize_selected ? 32'd2
        : phase==WRITE_CODE ? {24'd0,code} : header_word;
    wire result_write_ready;
    wire main_written=main_write && !finalize_selected && result_write_ready;
    wire final_written=finalize_write && result_write_ready;
    wire finish_valid=phase==FINISH || (phase==FAILED && group_active);
    wire [1:0] finish_bank=attempt_bank;
    wire [31:0] finish_epoch=attempt_epoch;
    wire [63:0] finish_sequence=attempt_sequence;
    wire finish_wait_fence=selected_switch;
    wire finish_fault=phase==FAILED || handoff_failure || pipeline_fault;
    wire fence_valid=finalize && (fence_failed || final_written);
    wire [1:0] fence_bank=pending_bank;
    wire [31:0] fence_epoch=pending_incarnation;
    wire [63:0] fence_sequence=pending_group;
    wire fence_fault=fence_failed;
    reg [7:0] value_code;
    wire valid_refusal=(refusal>=4 && refusal<=9) || (refusal>=11 && refusal<=29);
    wire valid_now=args[31:0]<=2 || args[31:0]==32'hffffffff;
    wire valid_first=args[63:32]<=2 || (args[63:32]==32'hffffffff && args[95:64]==0);
    wire valid_second=args[127:96]<=2 || (args[127:96]==32'hffffffff && args[159:128]==0);
    wire bad_gain=packet[190:183]==8'hff;

    assign boundary_ready=phase==IDLE && render_ready && !faulted && !pipeline_fault && !handoff_failure && epoch!=0;
    assign take=boundary_valid && boundary_ready && head_valid;
    assign render_valid=phase==RENDER && !faulted && !pipeline_fault && !handoff_failure;
    assign command_phase=phase!=IDLE && phase!=RENDER && phase!=HALTED;
    assign current_command_cycles=command_cycles;
    wire fault_fence_context=handoff_failure || (result_write_valid && !result_write_ready && finalize_selected);
    assign fault_reason={result_write_valid && !result_write_ready, fault_fence_context,
        group_active && command_cycles>=4095, kernel_write_fault,
        memory_response_valid && memory_response_error, command_response_valid && command_response_error,
        bank_fault, pipeline_fault};
    assign fault_event=|fault_reason && !faulted && !reset;
    assign fault_sequence=fault_fence_context ? pending_group : group_active ? attempt_sequence : 64'd0;
    assign fault_frame=fault_fence_context ? pending_frame : attempt_frame;
    assign fault_opcode=fault_fence_context ? 32'd5 : group_active && header_code==APPLIED && (phase==COMMAND_CHECK || phase==KERNEL_START
        || phase==KERNEL_WAIT || phase==SELECT || phase==WRITE_CODE) ? kind : 32'hffffffff;
    assign memory_valid=kernel_memory_valid && !faulted && !pipeline_fault && !handoff_failure;
    assign handoff_pending=pending;
    assign pending_epoch=pending_incarnation;
    assign pending_sequence=pending_group;
    assign select_valid=phase==SELECT && !faulted && !pipeline_fault && !handoff_failure;
    assign select_revision=args[63:0];
    assign select_region=args[64];
    assign select_instructions=args[95:80];
    assign select_sync_count=args[111:96];
    assign select_states=args[127:112];
    assign select_buffers=args[143:128];
    assign select_composites=args[144];
    assign select_sequence=attempt_sequence;
    assign select_epoch=attempt_epoch;

    fifo_group_banks banks (
        .clk(clk), .reset(reset), .epoch(epoch), .fault(faulted || pipeline_fault || handoff_failure),
        .host_valid(host_valid), .host_ready(host_ready), .host_operation(host_operation),
        .host_bank(host_bank), .host_epoch(host_epoch), .host_word(host_word),
        .host_strobe(host_strobe), .host_sequence(host_sequence),
        .host_response_valid(host_response_valid), .host_receipt(host_receipt),
        .inspect_bank(inspect_bank), .bank_state(bank_state), .written_words(written_words),
        .free_mask(free_mask), .result_ready_mask(result_ready_mask),
        .next_sequence(next_sequence), .last_admitted(last_admitted), .sequence_exhausted(sequence_exhausted),
        .head_valid(head_valid), .head_bank(head_bank), .head_sequence(head_sequence), .take(take),
        .command_read_valid(command_read_valid), .command_read_bank(command_read_bank), .command_read_word(command_read_word),
        .command_response_valid(command_response_valid), .command_response_error(command_response_error),
        .command_read_data(command_read_data),
        .result_write_valid(result_write_valid), .result_write_ready(result_write_ready),
        .result_write_bank(result_write_bank), .result_write_word(result_write_word), .result_write_data(result_write_data),
        .finish_valid(finish_valid), .finish_bank(finish_bank), .finish_epoch(finish_epoch), .finish_sequence(finish_sequence),
        .finish_wait_fence(finish_wait_fence), .finish_fault(finish_fault),
        .fence_valid(fence_valid), .fence_bank(fence_bank), .fence_epoch(fence_epoch), .fence_sequence(fence_sequence),
        .fence_fault(fence_fault),
        .result_read_valid(result_read_valid), .result_read_bank(result_read_bank), .result_read_word(result_read_word),
        .result_response_valid(result_response_valid), .result_response_error(result_response_error),
        .result_read_data(result_read_data), .protocol_fault(bank_fault)
    );
    fifo_edit_kernel #(.LOOPS(LOOPS)) kernel (
        .clk(clk), .reset(reset), .command_valid(phase==KERNEL_START && !faulted), .command_ready(kernel_ready),
        .command_kind(kind[2:0]), .command_slot(slot), .command_generation(packet[95:64]),
        .command_self_sync(packet[96]), .command_frame(attempt_frame), .command_args(args),
        .command_gain_address(gain_address),
        .memory_valid(kernel_memory_valid), .memory_ready(kernel_memory_ready),
        .memory_write(memory_write), .memory_state(memory_state), .memory_address(memory_address),
        .memory_write_data(memory_write_data), .memory_response_valid(memory_response_valid),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data),
        .result_valid(kernel_result_valid), .result_ready(phase==KERNEL_WAIT),
        .result_code(kernel_code), .write_fault(kernel_write_fault), .command_cycles(kernel_cycles)
    );

    always @* begin
        value_code=0;
        if (kind>6) value_code=UNSUPPORTED;
        else if (packet[383:352]!=0) value_code=INVALID;
        else if (kind==6) begin
            if (packet[127:32]!=0 || packet[351:160]!=0 || !valid_refusal) value_code=INVALID;
            else value_code=refusal[7:0];
        end else if (kind==5) begin
            if (packet[127:32]!=0 || packet[191:160]!=0 || args[159:145]!=0
                || args[79:64]>1 || args[95:80]>16'(PROGRAM_DEPTH)
                || args[111:96]>args[95:80] || args[127:112]>16'(STATES)
                || args[143:128]>16'(BUFFERS) || (refusal!=0 && !valid_refusal)) value_code=INVALID;
            else if (refusal!=0) value_code=refusal[7:0];
            else if (args[63:0]<=active_revision) value_code=STALE;
            else if (active_revision!=0 && (active_composites || args[144])) value_code=COMPOSITE;
            else if (pending || (active_revision!=0 && args[64]==active_region)) value_code=CANNOT;
        end else begin
            if (packet[127:97]!=0 || (kind!=3 && (gain_address!=0 || packet[191:160]!=0))
                || (refusal!=0 && !valid_refusal)) value_code=INVALID;
            else case (kind)
                0: if (!valid_now || !valid_first || !valid_second) value_code=INVALID;
                1: if (args[159:32]!=0 || args[31:0]==0 || args[31:0]>3) value_code=INVALID;
                2: if (args[159:1]!=0) value_code=INVALID;
                3: if (args[159:32]!=0 || bad_gain) value_code=INVALID;
                4: if (args[159:66]!=0) value_code=INVALID;
                default: value_code=UNSUPPORTED;
            endcase
            if (value_code==0) begin
                if (refusal!=0) value_code=refusal[7:0];
                else if (32'(slot)>=32'(LOOPS) || (kind==3 && 32'(gain_address)>=32'(STATES))) value_code=TARGET;
            end
        end
    end

    always @* begin
        case (header_index)
            0: header_word=32'h53465231;
            1: header_word={10'd0,command_count,16'd16};
            2: header_word=attempt_epoch;
            3: header_word=0;
            4: header_word=attempt_sequence[31:0];
            5: header_word=attempt_sequence[63:32];
            6: header_word=attempt_frame[31:0];
            7: header_word=attempt_frame[63:32];
            8: header_word=initial_revision[31:0];
            9: header_word=initial_revision[63:32];
            10: header_word=active_revision[31:0];
            11: header_word=active_revision[63:32];
            12: header_word={24'd0,header_code};
            13: header_word=selected_switch ? attempt_frame[31:0] : 32'd0;
            14: header_word=selected_switch ? attempt_frame[63:32] : 32'd0;
            15: header_word=had_switch ? 32'd1 : 32'd0;
            default: header_word=0;
        endcase
    end

    always @(posedge clk) begin
        if (reset) begin
            phase<=IDLE; faulted<=0; active_revision<=0; active_region<=0; active_composites<=0;
            active_instructions<=0; active_sync_count<=0; active_states<=0; active_buffers<=0;
            pending<=0; pending_finished<=0; fence_seen<=0; fence_failed<=0;
            pending_bank<=0; pending_incarnation<=0; pending_group<=0; pending_frame<=0;
            group_active<=0; command_cycles<=0; peak_command_cycles<=0; groups_attempted<=0; cutoff_sequence<=0;
            gain_update_valid<=0; gain_update_slot<=0; gain_update_bits<=0;
            attempt_bank<=0; attempt_epoch<=0; attempt_sequence<=0; attempt_frame<=0; initial_revision<=0;
            expected_graph<=0; expected_topology<=0; latest_frame<=0; has_deadline<=0;
            bad_placement<=0; had_switch<=0; selected_switch<=0;
            command_count<=0; command_index<=0; stream_sent<=0; response_index<=0;
            header_index<=0; packet<=0; header_code<=0; code<=0;
        end else begin
            gain_update_valid<=0;
            if (group_active && phase!=FAILED && phase!=HALTED) command_cycles<=command_cycles+1'b1;
            if (command_read_valid) begin response_index<=stream_sent; stream_sent<=stream_sent+1'b1; end
            if (handoff_matches) begin fence_seen<=1; fence_failed<=handoff_error; end
            if (final_written || (finalize && fence_failed)) begin
                pending<=0; pending_finished<=0; fence_seen<=0;
            end
            if (fault_event) begin
                faulted<=1; phase<=FAILED;
            end else case (phase)
                IDLE: if (boundary_valid && boundary_ready) begin
                    cutoff_sequence<=last_admitted;
                    if (head_valid) begin
                        attempt_bank<=head_bank; attempt_sequence<=head_sequence; attempt_epoch<=epoch;
                        attempt_frame<=boundary_frame; initial_revision<=active_revision;
                        group_active<=1; command_cycles<=0;
                        if (!(&groups_attempted)) groups_attempted<=groups_attempted+1'b1;
                        command_index<=0; stream_sent<=0; bad_placement<=0; had_switch<=0; selected_switch<=0;
                        header_code<=APPLIED; phase<=HEADER_READ;
                    end else phase<=RENDER;
                end
                HEADER_READ: if (command_response_valid) begin
                    case (response_index)
                        1: command_count<=command_read_data[21:16];
                        3: has_deadline<=command_read_data[0];
                        6: expected_graph[31:0]<=command_read_data;
                        7: expected_graph[63:32]<=command_read_data;
                        8: expected_topology[31:0]<=command_read_data;
                        9: expected_topology[63:32]<=command_read_data;
                        10: latest_frame[31:0]<=command_read_data;
                        11: latest_frame[63:32]<=command_read_data;
                        default: ;
                    endcase
                    if (response_index==15) phase<=HEADER_CHECK;
                end
                HEADER_CHECK: begin
                    if (expected_graph!=initial_revision || expected_topology!=initial_revision) header_code<=STALE;
                    else if (has_deadline && attempt_frame>latest_frame) header_code<=DEADLINE;
                    stream_sent<=0; phase<=SCAN;
                end
                SCAN: if (command_response_valid) begin
                    if (command_read_data==5) begin
                        had_switch<=1;
                        if (response_index+1'b1!=command_count) bad_placement<=1;
                    end
                    if (response_index+1'b1==command_count) phase<=SCAN_CHECK;
                end
                SCAN_CHECK: begin
                    if (header_code==APPLIED && bad_placement) begin header_code<=CANNOT; code<=CANNOT; phase<=WRITE_CODE; end
                    else if (header_code!=APPLIED) begin code<=header_code; phase<=WRITE_CODE; end
                    else begin stream_sent<=0; phase<=COMMAND_READ; end
                end
                COMMAND_READ: if (command_response_valid) begin
                    packet[32*response_index +: 32]<=command_read_data;
                    if (response_index==11) phase<=COMMAND_CHECK;
                end
                COMMAND_CHECK: begin
                    if (value_code!=0) begin code<=value_code; phase<=WRITE_CODE; end
                    else if (kind==5) phase<=SELECT;
                    else phase<=KERNEL_START;
                end
                KERNEL_START: if (kernel_ready) phase<=KERNEL_WAIT;
                KERNEL_WAIT: if (kernel_result_valid) begin
                    code<={4'd0,kernel_code}; phase<=WRITE_CODE;
                    if (kind==3 && {4'd0,kernel_code}==APPLIED) begin
                        gain_update_valid<=1; gain_update_slot<=slot; gain_update_bits<=packet[191:160];
                    end
                end
                SELECT: begin
                    if (!select_ready || select_error) code<=CANNOT;
                    else begin
                        code<=APPLIED; selected_switch<=1;
                        active_revision<=select_revision; active_region<=select_region; active_composites<=select_composites;
                        active_instructions<=select_instructions; active_sync_count<=select_sync_count;
                        active_states<=select_states; active_buffers<=select_buffers;
                        pending<=1; pending_finished<=0; pending_bank<=attempt_bank;
                        pending_incarnation<=attempt_epoch; pending_group<=attempt_sequence; pending_frame<=attempt_frame;
                        fence_seen<=handoff_valid && handoff_epoch==attempt_epoch && handoff_sequence==attempt_sequence;
                        fence_failed<=handoff_error;
                    end
                    phase<=WRITE_CODE;
                end
                WRITE_CODE: if (main_written) begin
                    if (command_index+1'b1==command_count) begin header_index<=0; phase<=WRITE_HEADER; end
                    else begin
                        command_index<=command_index+1'b1;
                        if (header_code!=APPLIED) code<=header_code;
                        else begin stream_sent<=0; phase<=COMMAND_READ; end
                    end
                end
                WRITE_HEADER: if (selected_switch && header_index==15) phase<=FINISH;
                    else if (main_written) begin
                        if (header_index==15) phase<=FINISH;
                        else header_index<=header_index+1'b1;
                    end
                FINISH: begin
                    group_active<=0;
                    if (selected_switch) pending_finished<=1;
                    if (command_cycles+32'd2>peak_command_cycles) peak_command_cycles<=command_cycles+32'd2;
                    phase<=RENDER;
                end
                RENDER: if (render_ready) phase<=IDLE;
                FAILED: begin group_active<=0; phase<=HALTED; end
                HALTED: ;
                default: begin faulted<=1; phase<=FAILED; end
            endcase
        end
    end
endmodule
