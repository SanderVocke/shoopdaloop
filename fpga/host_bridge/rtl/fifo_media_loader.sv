module fifo_media_loader #(
    parameter integer MAX_WORDS=32,
    parameter integer INDEX_BITS=MAX_WORDS<2 ? 1 : $clog2(MAX_WORDS),
    parameter integer COUNT_BITS=$clog2(MAX_WORDS+1)
) (
    input wire clk, reset,
    input wire cmd_valid, output wire cmd_ready,
    input wire [3:0] cmd_opcode,
    input wire [63:0] cmd_view, cmd_start, cmd_length,
    input wire [31:0] cmd_count,
    input wire write_valid, output wire write_ready, input wire [31:0] write_data,
    output wire read_valid, input wire read_ready, output wire [31:0] read_data,
    output wire response_valid, input wire response_ready,
    output reg [7:0] response_error,
    output reg [31:0] result_view, output reg result_empty,
    output wire [31:0] result_count,
    output reg faulted,
    output wire backend_valid, input wire backend_ready,
    output wire [3:0] backend_opcode,
    output wire [63:0] backend_view, backend_start, backend_length,
    output wire [31:0] backend_count,
    output wire backend_write_valid, input wire backend_write_ready,
    output wire [31:0] backend_write_data,
    input wire backend_read_valid, output wire backend_read_ready,
    input wire [31:0] backend_read_data,
    input wire backend_done, input wire [7:0] backend_error,
    input wire [31:0] backend_result_view, input wire backend_result_empty
);
    initial if (MAX_WORDS<1 || MAX_WORDS>32) $fatal(1,"loader chunks must fit 32 words");
    localparam [3:0] APPEND=5, READ=6;
    typedef enum logic [2:0] {IDLE, FILL, QUEUED, ACTIVE, RESULT} phase_t;
    phase_t phase;
    reg [3:0] opcode;
    reg [63:0] view, start, length;
    reg [31:0] count;
    reg [COUNT_BITS-1:0] filled, sent, received, consumed;
    reg acknowledged;
    (* ram_style="distributed" *) reg [31:0] data [0:MAX_WORDS-1];
    wire fill_word=write_valid && write_ready;
    wire send_word=backend_write_valid && backend_write_ready;
    wire receive_word=backend_read_valid && backend_read_ready;
    wire consume_word=read_valid && read_ready;
    wire [31:0] sent_at_done=32'(sent)+(send_word ? 32'd1 : 32'd0);
    wire [31:0] received_at_done=32'(received)+(receive_word ? 32'd1 : 32'd0);
    wire invalid_completion=(opcode==APPEND && sent_at_done!=count)
        || (opcode==READ && received_at_done!=(backend_result_empty ? 32'd0 : count));
    assign cmd_ready=phase==IDLE && !faulted && !reset;
    assign write_ready=phase==FILL && 32'(filled)<count && !reset;
    assign read_valid=phase==RESULT && consumed<received && !reset;
    assign read_data=data[consumed[INDEX_BITS-1:0]];
    assign response_valid=phase==RESULT && !acknowledged && !reset;
    assign result_count=32'(received);
    assign backend_valid=phase==QUEUED && !faulted && !reset;
    assign backend_opcode=opcode;
    assign backend_view=view;
    assign backend_start=start;
    assign backend_length=length;
    assign backend_count=count;
    assign backend_write_valid=phase==ACTIVE && opcode==APPEND && 32'(sent)<count && !reset;
    assign backend_write_data=data[sent[INDEX_BITS-1:0]];
    assign backend_read_ready=phase==ACTIVE && opcode==READ && 32'(received)<count && !reset;
    always @(posedge clk) begin
        if (fill_word) data[filled[INDEX_BITS-1:0]]<=write_data;
        else if (receive_word) data[received[INDEX_BITS-1:0]]<=backend_read_data;
    end
    always @(posedge clk) begin
        if (reset) begin
            phase<=IDLE; opcode<=0; view<=0; start<=0; length<=0; count<=0;
            filled<=0; sent<=0; received<=0; consumed<=0;
            acknowledged<=0; response_error<=0; result_view<=0; result_empty<=0; faulted<=0;
        end else begin
            if (consume_word) consumed<=consumed+1'b1;
            if (response_valid && response_ready) acknowledged<=1;
            case (phase)
                IDLE: if (cmd_valid && cmd_ready) begin
                    opcode<=cmd_opcode; view<=cmd_view; start<=cmd_start;
                    length<=cmd_length; count<=cmd_count;
                    filled<=0; sent<=0; received<=0; consumed<=0;
                    acknowledged<=0; response_error<=0; result_view<=0; result_empty<=0;
                    if ((cmd_opcode==APPEND || cmd_opcode==READ) && cmd_count>32'(MAX_WORDS)) begin
                        response_error<=4; phase<=RESULT;
                    end else phase<=cmd_opcode==APPEND && cmd_count!=0 ? FILL : QUEUED;
                end
                FILL: if (fill_word) begin
                    filled<=filled+1'b1;
                    if (32'(filled)+1==count) phase<=QUEUED;
                end
                QUEUED: if (backend_valid && backend_ready) phase<=ACTIVE;
                ACTIVE: begin
                    if (send_word) sent<=sent+1'b1;
                    if (receive_word) received<=received+1'b1;
                    if (backend_done) begin
                        response_error<=backend_error;
                        result_view<=backend_result_view; result_empty<=backend_result_empty;
                        if (backend_error==5 || (backend_error==0 && invalid_completion)) begin
                            response_error<=5; faulted<=1;
                        end
                        phase<=RESULT;
                    end
                end
                RESULT: if ((acknowledged || (response_valid && response_ready))
                    && (consumed==received || (consume_word && consumed+1'b1==received))) phase<=IDLE;
                default: begin faulted<=1; end
            endcase
        end
    end
endmodule
