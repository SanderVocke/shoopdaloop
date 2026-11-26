module semantic_group_probe #(
    parameter integer LOOPS=128, STATES=512, PROGRAM_DEPTH=1024, BUFFERS=512,
    parameter integer LOOP_BITS=$clog2(LOOPS*16), STATE_BITS=$clog2(STATES)
) (
    input wire clk, reset, input wire [31:0] epoch, input wire pipeline_fault,
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
    output wire [63:0] active_revision,
    output wire active_region, active_composites,
    output wire [15:0] active_instructions, active_sync_count, active_states, active_buffers,
    output wire select_valid, input wire select_ready, select_error,
    output wire [63:0] select_revision, select_sequence,
    output wire [31:0] select_epoch,
    output wire select_region, select_composites,
    output wire [15:0] select_instructions, select_sync_count, select_states, select_buffers,
    input wire handoff_valid, input wire [31:0] handoff_epoch,
    input wire [63:0] handoff_sequence, input wire handoff_error,
    output wire handoff_pending,
    output wire [31:0] pending_epoch, output wire [63:0] pending_sequence,
    output wire gain_update_valid, output wire [15:0] gain_update_slot,
    output wire [31:0] gain_update_bits,
    output wire faulted, command_phase,
    output wire [63:0] cutoff_sequence, groups_attempted,
    output wire [31:0] peak_command_cycles, current_command_cycles,
    input wire allow_memory, inject_read_error, inject_write_error,
    input wire cfg_valid, cfg_write, cfg_state,
    input wire [15:0] cfg_address, input wire [128:0] cfg_write_data,
    output reg cfg_response_valid, output wire [128:0] cfg_read_data,
    output reg [31:0] renders, gain_updates, memory_writes,
    output reg [31:0] last_gain_bits, output reg [15:0] last_gain_slot,
    output wire main_result_tail
);
    (* ram_style="block" *) reg [63:0] loops [0:LOOPS*16-1];
    (* ram_style="block" *) reg [128:0] states [0:STATES-1];
    wire memory_valid, memory_write, memory_state;
    wire [15:0] memory_address;
    wire [128:0] memory_write_data;
    wire memory_ready=allow_memory;
    reg memory_response_valid, memory_response_error;
    wire [128:0] memory_read_data;
    wire accept=memory_valid && memory_ready;
    wire in_range=memory_state ? 32'(memory_address)<32'(STATES) : 32'(memory_address)<32'(LOOPS*16);
    wire external_access=cfg_valid && boundary_ready && !boundary_valid;
    wire access_valid=(accept && in_range) || external_access;
    wire access_state=accept ? memory_state : cfg_state;
    wire access_write=accept ? memory_write : cfg_write;
    wire [15:0] access_address=accept ? memory_address : cfg_address;
    wire [128:0] access_data=accept ? memory_write_data : cfg_write_data;
    reg [63:0] loop_read;
    reg [128:0] state_read;
    reg response_state;
    always @(posedge clk) begin
        if (access_valid && !access_state) begin
            loop_read<=loops[access_address[LOOP_BITS-1:0]];
            if (access_write) loops[access_address[LOOP_BITS-1:0]]<=access_data[63:0];
        end
        if (access_valid && access_state) begin
            state_read<=states[access_address[STATE_BITS-1:0]];
            if (access_write) states[access_address[STATE_BITS-1:0]]<=access_data;
        end
        if (reset) begin
            memory_response_valid<=0; memory_response_error<=0; cfg_response_valid<=0;
            response_state<=0; renders<=0; gain_updates<=0; memory_writes<=0; last_gain_bits<=0; last_gain_slot<=0;
        end else begin
            memory_response_valid<=accept;
            memory_response_error<=accept && (!in_range || (memory_write ? inject_write_error : inject_read_error));
            cfg_response_valid<=external_access;
            if (access_valid) response_state<=access_state;
            if (render_valid && render_ready) renders<=renders+1'b1;
            if (accept && in_range && memory_write) memory_writes<=memory_writes+1'b1;
            if (gain_update_valid) begin
                gain_updates<=gain_updates+1'b1; last_gain_bits<=gain_update_bits; last_gain_slot<=gain_update_slot;
            end
        end
    end
    assign memory_read_data=response_state ? state_read : {65'd0,loop_read};
    assign cfg_read_data=memory_read_data;
    assign main_result_tail=consumer.main_write && consumer.command_index+1'b1==consumer.command_count;
    fifo_group_consumer #(.LOOPS(LOOPS), .STATES(STATES), .PROGRAM_DEPTH(PROGRAM_DEPTH), .BUFFERS(BUFFERS)) consumer (.*);
endmodule
