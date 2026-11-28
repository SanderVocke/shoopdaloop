module host_fault_record (
    input wire clk, reset, capture,
    input wire [31:0] epoch, cause, operation, cycles, control_cycles, detail,
    input wire [63:0] sequence_value, revision, frame,
    output reg [31:0] record_epoch, record_cause, record_op,
    output reg [31:0] record_cycles, record_control_cycles, record_detail,
    output reg [63:0] record_sequence, record_revision, record_frame
);
    always @(posedge clk) begin
        if (reset) begin
            record_epoch<=0; record_cause<=0; record_op<=0;
            record_cycles<=0; record_control_cycles<=0; record_detail<=0;
            record_sequence<=0; record_revision<=0; record_frame<=0;
        end else if (capture && epoch!=0 && epoch>record_epoch) begin
            record_epoch<=epoch; record_cause<=cause; record_op<=operation;
            record_cycles<=cycles; record_control_cycles<=control_cycles; record_detail<=detail;
            record_sequence<=sequence_value; record_revision<=revision; record_frame<=frame;
        end
    end
endmodule
