module audio_control #(
    parameter integer DEPTH = 128,
    parameter integer RESULT_DEPTH = DEPTH,
    parameter integer MAX_BATCH_CYCLES = 16384
) (
    input wire clk, input wire reset, input wire enable,
    input wire [63:0] sequence_number,
    input wire [31:0] epoch,
    input wire busy, input wire run_start, input wire admission_blocked,
    output wire run_permitted,
    output reg fault,
    input wire control_valid, output wire control_ready,
    input wire [745:0] control_data,
    output wire command_valid, input wire command_ready,
    output wire [7:0] command_opcode,
    output wire [31:0] command_index,
    output wire [639:0] command_data,
    input wire response_valid, output wire response_ready,
    input wire [3:0] response_error,
    input wire [831:0] response_data,
    output wire result_valid, input wire result_ready,
    output wire [1087:0] result_data,
    output wire cfg_valid, input wire cfg_ready,
    output wire cfg_state, output wire cfg_write,
    output wire [15:0] cfg_address,
    output wire [128:0] cfg_write_data,
    input wire cfg_response_valid, input wire cfg_response_error
);
    localparam [7:0] BATCH_HEADER = 8'ha1;
    wire head_valid;
    wire [745:0] head;
    wire [$clog2(DEPTH+1)-1:0] count;
    reg active, serviced, waiting, waiting_cfg;
    reg [31:0] remaining, elapsed;
    reg [63:0] batch_sequence, batch_id, admitted_sequence;
    reg [31:0] batch_epoch, ordinal, issued_ordinal;
    reg [1:0] issued_kind;
    reg [7:0] issued_opcode;
    wire [$clog2(RESULT_DEPTH+1)-1:0] result_count;
    wire result_push_ready;
    wire [1:0] kind = head[681:680];
    wire valid_header = kind == 2 && head[679:672] == BATCH_HEADER
        && head[671:640] != 0 && head[671:640] < 32'(DEPTH)
        && head[671:640] <= 32'(RESULT_DEPTH) && head[63:0] != 0;
    wire complete_batch = head_valid && valid_header && 32'(count) > head[671:640];
    wire batch_due = complete_batch && head[745:682] <= sequence_number;
    wire result_space = 32'(result_count)+head[671:640] <= 32'(RESULT_DEPTH);
    wire available = enable && !busy && !waiting && !fault;
    wire begin_batch = available && !active && !serviced && !admission_blocked && batch_due && result_space;
    wire supported_command = head[679:672] == 0
        || (head[679:672] >= 4 && head[679:672] <= 6)
        || head[679:672] == 12 || head[679:672] == 13 || head[679:672] == 15
        || (head[679:672] >= 17 && head[679:672] <= 21);
    wire valid_operation = (kind == 0 && supported_command) || kind == 1;
    wire dispatch = available && active && head_valid && valid_operation
        && head[745:682] == batch_sequence;
    assign run_permitted = enable && !fault && !active && !waiting
        && (serviced || admission_blocked || !batch_due || !result_space);
    assign command_valid = dispatch && kind == 0;
    assign command_opcode = head[679:672];
    assign command_index = head[671:640];
    assign command_data = head[639:0];
    assign response_ready = waiting && !waiting_cfg && result_push_ready;
    assign cfg_valid = dispatch && kind == 1;
    assign cfg_state = head[671];
    assign cfg_write = 1;
    assign cfg_address = head[655:640];
    assign cfg_write_data = head[128:0];
    wire accepted = (command_valid && command_ready) || (cfg_valid && cfg_ready);
    wire completed = waiting && (waiting_cfg ? cfg_response_valid : response_valid && response_ready);
    wire [3:0] result_error = waiting_cfg ? {3'b0,cfg_response_error} : response_error;
    wire [831:0] result_payload = waiting_cfg ? 832'b0 : response_data;
    wire [1087:0] completed_result = {50'b0,batch_epoch,admitted_sequence,batch_id,
        issued_ordinal,issued_opcode,issued_kind,result_error,result_payload};
    wire pop = begin_batch || accepted;
    wire unused = &{1'b0,head[639:129]};
    audio_command_fifo #(.DEPTH(DEPTH)) queue (
        .clk(clk),.reset(reset),.push_valid(control_valid),.push_ready(control_ready),.push_data(control_data),
        .pop_valid(head_valid),.pop_ready(pop),.pop_data(head),.count(count)
    );
    audio_command_fifo #(.WIDTH(1088),.DEPTH(RESULT_DEPTH)) results (
        .clk(clk),.reset(reset),.push_valid(completed),.push_ready(result_push_ready),.push_data(completed_result),
        .pop_valid(result_valid),.pop_ready(result_ready),.pop_data(result_data),.count(result_count)
    );
    initial if (DEPTH < 2 || DEPTH > 65536 || RESULT_DEPTH < 1 || RESULT_DEPTH > 65536
        || MAX_BATCH_CYCLES < 1) $fatal(1,"Invalid control batch limits");
    always @(posedge clk) begin
        if (reset) begin
            fault<=0; active<=0; serviced<=0; waiting<=0; waiting_cfg<=0;
            remaining<=0; elapsed<=0; batch_sequence<=0;
            batch_id<=0; admitted_sequence<=0; batch_epoch<=0; ordinal<=0;
            issued_ordinal<=0; issued_kind<=0; issued_opcode<=0;
        end else begin
            if (run_start) serviced<=0;
            if (available && !active && !serviced && head_valid && !valid_header) fault<=1;
            if (available && active && head_valid
                && (!valid_operation || head[745:682] != batch_sequence)) fault<=1;
            if (begin_batch) begin
                active<=1; serviced<=1; remaining<=head[671:640];
                batch_sequence<=head[745:682]; elapsed<=0;
                batch_id<=head[63:0]; admitted_sequence<=sequence_number; batch_epoch<=epoch; ordinal<=0;
            end
            if (active && !fault) begin
                if (elapsed == 32'(MAX_BATCH_CYCLES-1)) fault<=1;
                else elapsed<=elapsed+1'b1;
            end
            if (accepted) begin
                remaining<=remaining-1'b1;
                waiting<=1; waiting_cfg<=kind == 1;
                issued_kind<=kind; issued_opcode<=head[679:672]; issued_ordinal<=ordinal;
                ordinal<=ordinal+1'b1;
            end
            if (completed) begin
                waiting<=0;
                if (remaining == 0) active<=0;
                if (!result_push_ready) fault<=1;
            end
        end
    end
endmodule
