module audio_control #(
    parameter integer DEPTH = 128
) (
    input wire clk, input wire reset, input wire enable,
    input wire [63:0] sequence_number,
    input wire busy, input wire run_start,
    output reg fenced,
    output reg fault,
    input wire control_valid, output wire control_ready,
    input wire [745:0] control_data,
    output wire command_valid, input wire command_ready,
    output wire [7:0] command_opcode,
    output wire [31:0] command_index,
    output wire [639:0] command_data,
    input wire response_valid, output wire response_ready,
    input wire [3:0] response_error,
    output wire cfg_valid, input wire cfg_ready,
    output wire cfg_state, output wire cfg_write,
    output wire [15:0] cfg_address,
    output wire [128:0] cfg_write_data,
    input wire cfg_response_valid, input wire cfg_response_error
);
    wire head_valid;
    wire [745:0] head;
    wire [$clog2(DEPTH+1)-1:0] count;
    reg waiting;
    reg waiting_cfg;
    wire available = enable && !busy && !fenced && !waiting && !fault && head_valid;
    wire correct_sequence = head[745:682] == sequence_number;
    wire [1:0] kind = head[681:680];
    wire dispatch = available && correct_sequence;
    assign command_valid = dispatch && kind == 0;
    assign command_opcode = head[679:672];
    assign command_index = head[671:640];
    assign command_data = head[639:0];
    assign response_ready = waiting && !waiting_cfg;
    assign cfg_valid = dispatch && kind == 1;
    assign cfg_state = head[671];
    assign cfg_write = 1;
    assign cfg_address = head[655:640];
    assign cfg_write_data = head[128:0];
    wire pop = (command_valid && command_ready) || (cfg_valid && cfg_ready) || (dispatch && kind == 2);
    wire unused = &{1'b0,count,head[639:129]};
    audio_command_fifo #(.DEPTH(DEPTH)) queue (
        .clk(clk),.reset(reset),.push_valid(control_valid),.push_ready(control_ready),.push_data(control_data),
        .pop_valid(head_valid),.pop_ready(pop),.pop_data(head),.count(count)
    );
    always @(posedge clk) begin
        if (reset) begin fenced<=0; fault<=0; waiting<=0; waiting_cfg<=0; end
        else begin
            if (run_start) fenced<=0;
            if (available && (!correct_sequence || kind == 3)) fault<=1;
            if (dispatch && kind == 2) fenced<=1;
            if ((command_valid && command_ready) || (cfg_valid && cfg_ready)) begin
                waiting<=1; waiting_cfg<=kind == 1;
            end
            if (waiting && (waiting_cfg ? cfg_response_valid : response_valid)) begin
                waiting<=0;
                if (waiting_cfg ? cfg_response_error : response_error != 0) fault<=1;
            end
        end
    end
endmodule
