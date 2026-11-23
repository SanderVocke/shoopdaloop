module program_axi_mux (
    input wire clk, input wire reset,
    input wire media_valid,
    output wire media_ready,
    input wire media_write,
    input wire [63:0] media_address,
    input wire [31:0] media_count,
    input wire [31:0] media_write_data,
    input wire media_write_valid,
    output wire media_write_ready,
    output wire media_read_valid,
    input wire media_read_ready,
    output wire [31:0] media_read_data,
    output wire media_response_valid,
    input wire media_response_ready,
    output wire media_response_error,
    input wire program_valid,
    output wire program_ready,
    input wire program_write,
    input wire [63:0] program_address,
    input wire [31:0] program_count,
    input wire [31:0] program_write_data,
    input wire program_write_valid,
    output wire program_write_ready,
    output wire program_read_valid,
    input wire program_read_ready,
    output wire [31:0] program_read_data,
    output wire program_response_valid,
    input wire program_response_ready,
    output wire program_response_error,
    output wire memory_valid,
    input wire memory_ready,
    output wire memory_write,
    output wire [63:0] memory_address,
    output wire [31:0] memory_count,
    output wire [31:0] memory_write_data,
    output wire memory_write_valid,
    input wire memory_write_ready,
    input wire memory_read_valid,
    output wire memory_read_ready,
    input wire [31:0] memory_read_data,
    input wire memory_response_valid,
    output wire memory_response_ready,
    input wire memory_response_error
);
    reg active, owner, prefer_program;
    wire selected = program_valid && (!media_valid || prefer_program);
    assign memory_valid = !active && (media_valid || program_valid);
    assign media_ready = !active && !selected && memory_ready;
    assign program_ready = !active && selected && memory_ready;

    assign memory_write = selected ? program_write : media_write;
    assign memory_address = selected ? program_address : media_address;
    assign memory_count = selected ? program_count : media_count;
    assign memory_write_data = owner ? program_write_data : media_write_data;
    assign memory_write_valid = active && (owner ? program_write_valid : media_write_valid);
    assign memory_read_ready = active && (owner ? program_read_ready : media_read_ready);
    assign memory_response_ready = active && (owner ? program_response_ready : media_response_ready);
    assign media_write_ready = active && !owner && memory_write_ready;
    assign media_read_valid = active && !owner && memory_read_valid;
    assign media_read_data = memory_read_data;
    assign media_response_valid = active && !owner && memory_response_valid;
    assign media_response_error = memory_response_error;
    assign program_write_ready = active && owner && memory_write_ready;
    assign program_read_valid = active && owner && memory_read_valid;
    assign program_read_data = memory_read_data;
    assign program_response_valid = active && owner && memory_response_valid;
    assign program_response_error = memory_response_error;

    always @(posedge clk) begin
        if (reset) begin
            active <= 0;
            owner <= 0;
            prefer_program <= 0;
        end else if (memory_valid && memory_ready) begin
            active <= 1;
            owner <= selected;
        end else if (memory_response_valid && memory_response_ready) begin
            active <= 0;
            prefer_program <= !owner;
        end
    end
endmodule
