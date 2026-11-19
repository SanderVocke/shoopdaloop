module midi_difference (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready,
    input wire [63:0] command_target, input wire [63:0] command_current,
    input wire [63:0] command_map, input wire [12:0] command_first,
    input wire [12:0] command_count,
    output wire response_valid, input wire response_ready, output reg response_error,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output reg [63:0] memory_address, output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    `include "midi_functions.svh"
    localparam [3:0] IDLE=0, TARGET=1, TARGET_WAIT=2, CURRENT=3, CURRENT_WAIT=4,
        PROGRAM=5, PROGRAM_WAIT=6, MAP=7, MAP_WAIT=8, WRITE=9, WRITE_WAIT=10,
        NEXT=11, RESPONSE=12, DECODE=13;
    reg [3:0] phase;
    reg [63:0] target_base, current_base, map_base;
    reg [12:0] cell_index, remaining;
    reg [12:0] channel_base, field;
    reg [31:0] target_value, current_value, map_value;
    reg program_known;
    wire [31:0] bit_mask = 32'b1 << cell_index[4:0];
    wire different = cell_diff(integer'(field),target_value,current_value,program_known)
        && cell_index != 4224;
    wire [31:0] next_map = (map_value & ~bit_mask) | (different ? bit_mask : 0);
    assign command_ready = phase == IDLE && !reset;
    assign response_valid = phase == RESPONSE && !reset;
    assign memory_valid = (phase == TARGET || phase == CURRENT || phase == PROGRAM ||
        phase == MAP || (phase == WRITE && memory_write_data != map_value)) && !reset;
    assign memory_write = phase == WRITE;
    assign memory_write_data = cell_index == 4224 ? next_map & 1 : next_map;
    assign memory_response_ready = phase == TARGET_WAIT || phase == CURRENT_WAIT ||
        phase == PROGRAM_WAIT || phase == MAP_WAIT || phase == WRITE_WAIT;
    always @* begin
        case (phase)
            TARGET,TARGET_WAIT: memory_address = target_base + 64'(cell_index)*4;
            CURRENT,CURRENT_WAIT: memory_address = current_base + 64'(cell_index)*4;
            PROGRAM,PROGRAM_WAIT: memory_address = target_base + (64'(channel_base)+256)*4;
            default: memory_address = map_base + 64'(cell_index >> 5)*4;
        endcase
    end
    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; target_base <= 0; current_base <= 0; map_base <= 0;
            cell_index <= 0; remaining <= 0; response_error <= 0; channel_base <= 0; field <= 0;
            target_value <= 0; current_value <= 0; map_value <= 0; program_known <= 0;
        end else if (memory_response_valid && memory_response_ready && memory_response_error) begin
            response_error <= 1; phase <= RESPONSE;
        end else case (phase)
            IDLE: if (command_valid) begin
                target_base <= command_target; current_base <= command_current; map_base <= command_map;
                cell_index <= command_first; remaining <= command_count; response_error <= 0;
                channel_base <= 0; field <= command_first;
                if (command_count == 0 || {1'b0,command_first}+{1'b0,command_count} > 4225) begin
                    response_error <= 1; phase <= RESPONSE;
                end else phase <= DECODE;
            end
            DECODE: if (field >= 264) begin field <= field - 264; channel_base <= channel_base + 264; end
                else phase <= TARGET;
            TARGET: if (memory_ready) phase <= TARGET_WAIT;
            TARGET_WAIT: if (memory_response_valid) begin
                target_value <= memory_read_data; program_known <= 0; phase <= CURRENT;
            end
            CURRENT: if (memory_ready) phase <= CURRENT_WAIT;
            CURRENT_WAIT: if (memory_response_valid) begin
                current_value <= memory_read_data;
                phase <= field >= 256 && field <= 258 ? PROGRAM : MAP;
            end
            PROGRAM: if (memory_ready) phase <= PROGRAM_WAIT;
            PROGRAM_WAIT: if (memory_response_valid) begin program_known <= memory_read_data < 128; phase <= MAP; end
            MAP: if (memory_ready) phase <= MAP_WAIT;
            MAP_WAIT: if (memory_response_valid) begin map_value <= memory_read_data; phase <= WRITE; end
            WRITE: if (memory_write_data == map_value) phase <= NEXT;
                else if (memory_ready) phase <= WRITE_WAIT;
            WRITE_WAIT: if (memory_response_valid) phase <= NEXT;
            NEXT: if (remaining == 1) phase <= RESPONSE;
                else begin
                    cell_index <= cell_index + 1; remaining <= remaining - 1; phase <= TARGET;
                    if (field == 263) begin field <= 0; channel_base <= channel_base + 264; end
                    else field <= field + 1;
                end
            RESPONSE: if (response_ready) phase <= IDLE;
            default: phase <= IDLE;
        endcase
    end
endmodule
