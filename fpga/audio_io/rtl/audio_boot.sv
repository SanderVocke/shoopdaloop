module audio_boot #(
    parameter integer WORDS = 256,
    parameter FILE_NAME = "fpga/audio_io/examples/default/boot.hex",
    parameter integer ADDRESS_BITS = (WORDS > 1) ? $clog2(WORDS) : 1
) (
    input wire clk, input wire reset,
    output reg done, output reg fault,
    output wire command_valid, input wire command_ready,
    output wire [7:0] command_opcode, output wire [31:0] command_index,
    output wire [639:0] command_data,
    input wire response_valid, output wire response_ready, input wire [3:0] response_error,
    output wire cfg_valid, input wire cfg_ready, output wire cfg_state,
    output wire cfg_write, output wire [15:0] cfg_address, output wire [128:0] cfg_write_data,
    input wire cfg_response_valid, input wire cfg_response_error,
    output wire store_cfg_valid, input wire store_cfg_ready, output wire store_cfg_write,
    output wire [2:0] store_cfg_space, output wire [31:0] store_cfg_address,
    output wire [159:0] store_cfg_write_data,
    input wire store_cfg_response_valid, input wire store_cfg_response_error
);
    reg [745:0] rom [0:WORDS-1];
    initial $readmemh(FILE_NAME,rom);
    reg [ADDRESS_BITS-1:0] address;
    reg waiting;
    wire [745:0] word_value = rom[address];
    wire [1:0] kind = word_value[681:680];
    wire valid = !done && !fault && !waiting && !reset;
    wire unused = &{1'b0,word_value[745:682],word_value[639:160]};
    assign command_valid = valid && kind == 0;
    assign command_opcode = word_value[679:672];
    assign command_index = word_value[671:640];
    assign command_data = word_value[639:0];
    assign response_ready = waiting && kind == 0;
    assign cfg_valid = valid && kind == 1;
    assign cfg_state = word_value[671];
    assign cfg_write = 1;
    assign cfg_address = word_value[655:640];
    assign cfg_write_data = word_value[128:0];
    assign store_cfg_valid = valid && kind == 2;
    assign store_cfg_write = 1;
    assign store_cfg_space = word_value[674:672];
    assign store_cfg_address = word_value[671:640];
    assign store_cfg_write_data = word_value[159:0];
    wire accepted = (command_valid && command_ready) || (cfg_valid && cfg_ready) || (store_cfg_valid && store_cfg_ready);
    wire responded = kind == 0 ? response_valid : kind == 1 ? cfg_response_valid : store_cfg_response_valid;
    wire failed = kind == 0 ? response_error != 0 : kind == 1 ? cfg_response_error : store_cfg_response_error;
    always @(posedge clk) begin
        if (reset) begin address<=0; waiting<=0; done<=0; fault<=0; end
        else begin
            if (valid && kind == 3) done<=1;
            if (accepted) waiting<=1;
            if (waiting && responded) begin
                waiting<=0;
                if (failed || 32'(address)+1 >= WORDS) fault<=1;
                else address<=address+1'b1;
            end
        end
    end
endmodule
