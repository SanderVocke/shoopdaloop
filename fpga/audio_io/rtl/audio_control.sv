module audio_control #(
    parameter integer DEPTH=128, RESULT_DEPTH=DEPTH,
    parameter integer MAX_BATCH_CYCLES=16384, MAX_BATCH_WORK=12000,
    parameter integer ACTIVATION_WORK=7200, ACTION_SLOTS=128, RESIDENT_ACTIONS=0
) (
    input wire clk, input wire reset, input wire enable,
    input wire [63:0] sequence_number, input wire [31:0] epoch,
    input wire busy, input wire run_start, input wire admission_blocked,
    output wire run_permitted, output reg fault,
    input wire control_valid, output wire control_ready, input wire [745:0] control_data,
    output wire command_valid, input wire command_ready, output wire [7:0] command_opcode,
    output wire [31:0] command_index, output wire [639:0] command_data,
    input wire response_valid, output wire response_ready, input wire [3:0] response_error,
    input wire [831:0] response_data,
    output wire result_valid, input wire result_ready, output wire [1087:0] result_data,
    output wire cfg_valid, input wire cfg_ready, output wire cfg_state, output wire cfg_write,
    output wire [7:0] cfg_opcode, output wire [15:0] cfg_address, output wire [639:0] cfg_write_data,
    input wire cfg_response_valid, input wire [3:0] cfg_response_error, input wire [831:0] cfg_read_data
);
    localparam [7:0] BATCH_HEADER=8'ha3;
    localparam integer C=$clog2(MAX_BATCH_WORK+2);
    localparam integer S=$clog2(ACTIVATION_WORK+14*ACTION_SLOTS+129);
    localparam integer SUM=(C>S ? C : S)+1;
    wire head_valid;
    wire [745:0] head, peek_data;
    wire [$clog2(DEPTH+1)-1:0] count;
    reg active, serviced, waiting, waiting_cfg, rejecting, ready_to_admit;
    reg decision_sequence;
    reg scan_active, scan_waiting, scan_ready, scan_rejected;
    reg [31:0] scan_index, scan_length;
    reg [C-1:0] scan_cost;
    wire peek_ready, peek_response_valid;
    reg [31:0] remaining, elapsed;
    reg [63:0] batch_id, admitted_sequence;
    reg [31:0] batch_epoch, ordinal, issued_ordinal;
    reg [1:0] issued_kind;
    reg [7:0] issued_opcode;
    wire [$clog2(RESULT_DEPTH+1)-1:0] result_count;
    wire result_push_ready;
    function automatic allowed(input [9:0] operation);
        reg [7:0] op;
        begin
            op=operation[7:0];
            allowed=(operation[9:8] == 0 && (op == 0 || (op >= 4 && op <= 6)
                || op == 12 || op == 13 || op == 15 || (op >= 17 && op <= 21)))
                || (operation[9:8] == 1 && (op <= 8 || op == 48 || (RESIDENT_ACTIONS != 0 && (op == 49 || op == 50)) || (op >= 16 && op <= 24)
                    || op == 128 || op == 133 || op == 134 || (op >= 136 && op <= 148)));
        end
    endfunction
    wire [1:0] kind=head[681:680];
    wire valid_header=kind == 2 && head[679:672] == BATCH_HEADER && head[671:640] != 0
        && head[671:640] < 32'(DEPTH) && head[671:640] <= 32'(RESULT_DEPTH) && head[63:0] != 0;
    wire complete_batch=head_valid && valid_header && 32'(count) > head[671:640];
    wire batch_due=complete_batch && head[745:682] <= sequence_number;
    wire result_space=32'(result_count)+head[671:640] <= 32'(RESULT_DEPTH);
    wire available=enable && !busy && !waiting && !fault;
    wire decision_current=decision_sequence == sequence_number[0];
    wire begin_batch=available && !active && !serviced && !admission_blocked && ready_to_admit && decision_current;
    wire blocked_operation=RESIDENT_ACTIONS != 0 &&
        ((kind == 0 && ((head[679:672] >= 4 && head[679:672] <= 6) || head[679:672] >= 17))
        || (kind == 1 && (head[679:672] == 0 || head[679:672] == 5 || head[679:672] == 6
            || head[679:672] == 128 || head[679:672] == 133 || head[679:672] == 134 || head[679:672] == 136)));
    wire dispatch=available && active && !rejecting && !blocked_operation && head_valid;
    wire discard=available && active && (rejecting || blocked_operation) && head_valid;
    wire peek_valid=enable && !fault && scan_active && !scan_waiting;
    wire [S-1:0] step_cost=peek_data[681:680] == 0 ? S'(80) : peek_data[679:672] == 5 ? S'(ACTIVATION_WORK)
        : peek_data[679:672] == 6 ? S'(128) : peek_data[679:672] == 18 ? S'(14*ACTION_SLOTS+32) : S'(16);
    wire [SUM-1:0] accumulated_cost=SUM'(scan_cost)+SUM'(step_cost);
    assign run_permitted=enable && !fault && !active && !waiting
        && (busy || serviced || admission_blocked || (decision_current && !ready_to_admit));
    assign command_valid=dispatch && kind == 0;
    assign command_opcode=head[679:672];
    assign command_index=head[671:640];
    assign command_data=head[639:0];
    assign response_ready=waiting && !waiting_cfg && result_push_ready;
    assign cfg_valid=dispatch && kind == 1;
    assign cfg_opcode=head[679:672];
    assign cfg_state=head[671];
    assign cfg_write=1;
    assign cfg_address=head[655:640];
    assign cfg_write_data=head[639:0];
    wire accepted=(command_valid && command_ready) || (cfg_valid && cfg_ready);
    wire completed=discard || (waiting && (waiting_cfg ? cfg_response_valid : response_valid && response_ready));
    wire [3:0] result_error=discard ? (rejecting ? 4'd2 : 4'd10) : waiting_cfg ? cfg_response_error : response_error;
    wire [831:0] result_payload=discard ? 832'b0 : waiting_cfg ? cfg_read_data : response_data;
    wire [1087:0] completed_result={50'b0,batch_epoch,admitted_sequence,batch_id,
        discard ? ordinal : issued_ordinal,discard ? head[679:672] : issued_opcode,
        discard ? kind : issued_kind,result_error,result_payload};
    wire pop=begin_batch || accepted || discard;
    wire unused_peek_ready, unused_peek_response;
    wire [1087:0] unused_peek_data;
    wire unused=&{1'b0,unused_peek_ready,unused_peek_response,unused_peek_data,peek_data[671:0]};
    audio_command_fifo #(.DEPTH(DEPTH),.ENABLE_PEEK(1)) queue (
        .clk(clk),.reset(reset),.push_valid(control_valid),.push_ready(control_ready),.push_data(control_data),
        .pop_valid(head_valid),.pop_ready(pop),.pop_data(head),.count(count),
        .peek_valid(peek_valid),.peek_ready(peek_ready),.peek_offset(scan_index),
        .peek_response_valid(peek_response_valid),.peek_data(peek_data)
    );
    audio_command_fifo #(.WIDTH(1088),.DEPTH(RESULT_DEPTH)) results (
        .clk(clk),.reset(reset),.push_valid(completed),.push_ready(result_push_ready),.push_data(completed_result),
        .pop_valid(result_valid),.pop_ready(result_ready),.pop_data(result_data),.count(result_count),
        .peek_valid(1'b0),.peek_ready(unused_peek_ready),.peek_offset(32'b0),
        .peek_response_valid(unused_peek_response),.peek_data(unused_peek_data)
    );
    initial if (DEPTH < 2 || DEPTH > 65536 || RESULT_DEPTH < 1 || RESULT_DEPTH > 65536
        || MAX_BATCH_WORK < 1 || MAX_BATCH_WORK > MAX_BATCH_CYCLES || MAX_BATCH_CYCLES > 1000000000
        || ACTIVATION_WORK < 1) $fatal(1,"Invalid control batch limits");
    always @(posedge clk) begin
        if (reset) begin
            fault<=0; active<=0; serviced<=0; waiting<=0; waiting_cfg<=0; rejecting<=0; ready_to_admit<=0;
            remaining<=0; elapsed<=0; decision_sequence<=0;
            batch_id<=0; admitted_sequence<=0; batch_epoch<=0; ordinal<=0;
            issued_ordinal<=0; issued_kind<=0; issued_opcode<=0;
            scan_active<=0; scan_waiting<=0; scan_ready<=0; scan_rejected<=0;
            scan_index<=0; scan_length<=0; scan_cost<=0;
        end else begin
            if (run_start) serviced<=0;
            ready_to_admit<=!active && batch_due && scan_ready && result_space;
            decision_sequence<=sequence_number[0];
            if (available && !active && !serviced && head_valid && !valid_header) fault<=1;
            if (enable && !fault && !active && !scan_active && !scan_ready && complete_batch) begin
                scan_active<=1; scan_index<=1; scan_length<=head[671:640]; scan_cost<=C'(8);
            end
            if (peek_valid && peek_ready) scan_waiting<=1;
            if (scan_waiting && peek_response_valid) begin
                scan_waiting<=0;
                if (!allowed(peek_data[681:672]) || peek_data[745:682] != head[745:682]
                    || (peek_data[681:680] == 1 && peek_data[670:656] != 0)) fault<=1;
                scan_cost<=accumulated_cost > SUM'(MAX_BATCH_WORK) ? C'(MAX_BATCH_WORK+1) : C'(accumulated_cost);
                if (scan_index == scan_length) begin
                    scan_active<=0; scan_ready<=1; scan_rejected<=accumulated_cost > SUM'(MAX_BATCH_WORK);
                end else scan_index<=scan_index+1'b1;
            end
            if (begin_batch) begin
                active<=1; serviced<=1; remaining<=head[671:640]; rejecting<=scan_rejected;
                elapsed<=0;
                batch_id<=head[63:0]; admitted_sequence<=sequence_number; batch_epoch<=epoch; ordinal<=0;
                scan_ready<=0;
                ready_to_admit<=0;
            end
            if (active && !fault) begin
                if (elapsed == 32'(MAX_BATCH_CYCLES-1)) fault<=1;
                else elapsed<=elapsed+1'b1;
            end
            if (accepted || discard) begin
                remaining<=remaining-1'b1;
                waiting<=!discard; waiting_cfg<=kind == 1;
                issued_kind<=kind; issued_opcode<=head[679:672]; issued_ordinal<=ordinal;
                ordinal<=ordinal+1'b1;
            end
            if (completed) begin
                waiting<=0;
                if (discard ? remaining == 1 : remaining == 0) active<=0;
                if (!result_push_ready) fault<=1;
            end
        end
    end
endmodule
