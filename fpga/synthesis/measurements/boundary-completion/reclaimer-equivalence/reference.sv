module reclaimer_reference #(
    parameter integer GUARD_CYCLES = 1024
) (
    input wire clk, input wire reset, input wire enable, input wire drain,
    input wire grant, input wire [31:0] grant_cycles, input wire [31:0] grant_blocks,
    input wire [31:0] pending_tables, input wire [31:0] free_count, input wire [31:0] allocated_count,
    output wire lock_request, input wire lock_grant, output reg lock_held,
    output reg request_valid, input wire request_ready, output reg [3:0] request_opcode,
    output reg [31:0] request_index, output reg [31:0] request_data,
    input wire response_valid, input wire [31:0] response_data, input wire [7:0] response_error,
    output wire retire_valid, input wire retire_ready, output reg [31:0] active_block,
    input wire retire_done, input wire [7:0] retire_error,
    output reg freed, output reg released, output wire active, output reg fault,
    output reg [63:0] released_blocks, output reg [63:0] freed_blocks,
    output reg [63:0] maintenance_cycles
);
    typedef enum logic [4:0] {
        IDLE, LOOKUP, LOOKUP_WAIT, ACQUIRE_REFERENCE, READ_REFERENCE, REFERENCE_WAIT,
        WRITE_REFERENCE, WRITE_WAIT, RETIRE, RETIRE_WAIT, ACQUIRE_FREE,
        ZERO_REFERENCE, ZERO_WAIT, PUSH_FREE, PUSH_WAIT, COMMIT_FREE,
        TRIM, TRIM_WAIT, FAULT
    } phase_t;
    phase_t phase;
    reg [31:0] cycles_left, blocks_left, reference_count, free_index;
    reg block_limited;
    wire permitted = enable && (drain || (cycles_left > 32'(GUARD_CYCLES) && (!block_limited || blocks_left != 0)));
    wire transaction_pending = phase == LOOKUP_WAIT || phase == REFERENCE_WAIT || phase == WRITE_WAIT
        || phase == RETIRE_WAIT || phase == ZERO_WAIT || phase == PUSH_WAIT || phase == TRIM_WAIT;
    wire maintenance_work = !reset && !fault && (lock_held || transaction_pending
        || (phase == IDLE && pending_tables != 0 && permitted)
        || (request_valid && request_ready) || (lock_request && lock_grant) || (retire_valid && retire_ready));
    assign lock_request = (phase == ACQUIRE_REFERENCE || phase == ACQUIRE_FREE) && permitted;
    assign retire_valid = phase == RETIRE && permitted && !reset && !fault;
    assign active = phase != IDLE && phase != FAULT;
    always @* begin
        request_valid = 0; request_opcode = 0; request_index = 0; request_data = 0;
        case (phase)
            LOOKUP: begin request_valid = 1; request_opcode = 11; end
            READ_REFERENCE: begin request_valid = 1; request_opcode = 2; request_index = active_block; end
            WRITE_REFERENCE: begin request_valid = 1; request_opcode = 3; request_index = active_block; request_data = reference_count-1; end
            ZERO_REFERENCE: begin request_valid = 1; request_opcode = 3; request_index = active_block; end
            PUSH_FREE: begin request_valid = 1; request_opcode = 5; request_index = free_index; request_data = active_block; end
            TRIM: begin request_valid = 1; request_opcode = 12; end
            default: begin end
        endcase
        request_valid = request_valid && (permitted || lock_held) && !reset && !fault;
    end
    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; cycles_left <= 0; blocks_left <= 0; block_limited <= 0; reference_count <= 0; free_index <= 0;
            active_block <= 0; lock_held <= 0; freed <= 0; released <= 0; fault <= 0;
            released_blocks <= 0; freed_blocks <= 0; maintenance_cycles <= 0;
        end else begin
            freed <= 0;
            if (maintenance_work && cycles_left != 0) cycles_left <= cycles_left-1;
            if (grant) begin
                cycles_left <= grant_cycles == 0 ? 0 : grant_cycles-32'(maintenance_work);
                blocks_left <= grant_blocks; block_limited <= grant_blocks != 0;
            end
            if (maintenance_work) maintenance_cycles <= maintenance_cycles+1;
            case (phase)
                IDLE: if (pending_tables != 0 && permitted) phase <= LOOKUP;
                LOOKUP: if (request_valid && request_ready) phase <= LOOKUP_WAIT;
                LOOKUP_WAIT: if (response_valid) begin
                    active_block <= response_data;
                    if (response_data >= allocated_count) begin fault <= 1; phase <= FAULT; end
                    else phase <= ACQUIRE_REFERENCE;
                end
                ACQUIRE_REFERENCE: if (lock_request && lock_grant) begin lock_held <= 1; phase <= READ_REFERENCE; end
                READ_REFERENCE: if (request_valid && request_ready) phase <= REFERENCE_WAIT;
                REFERENCE_WAIT: if (response_valid) begin
                    reference_count <= response_data;
                    if (response_data == 0) begin fault <= 1; lock_held <= 0; phase <= FAULT; end
                    else if (response_data == 1) begin lock_held <= 0; phase <= RETIRE; end
                    else phase <= WRITE_REFERENCE;
                end
                WRITE_REFERENCE: if (request_valid && request_ready) phase <= WRITE_WAIT;
                WRITE_WAIT: if (response_valid) begin released <= 1; lock_held <= 0; phase <= TRIM; end
                RETIRE: if (retire_valid && retire_ready) phase <= RETIRE_WAIT;
                RETIRE_WAIT: if (retire_done) begin
                    if (retire_error != 0) begin fault <= 1; phase <= FAULT; end
                    else phase <= ACQUIRE_FREE;
                end
                ACQUIRE_FREE: if (lock_request && lock_grant) begin
                    if (free_count >= allocated_count) begin fault <= 1; phase <= FAULT; end
                    else begin lock_held <= 1; free_index <= free_count; phase <= ZERO_REFERENCE; end
                end
                ZERO_REFERENCE: if (request_valid && request_ready) phase <= ZERO_WAIT;
                ZERO_WAIT: if (response_valid) phase <= PUSH_FREE;
                PUSH_FREE: if (request_valid && request_ready) phase <= PUSH_WAIT;
                PUSH_WAIT: if (response_valid) begin
                    freed <= 1; freed_blocks <= freed_blocks+1; released <= 1; phase <= COMMIT_FREE;
                end
                COMMIT_FREE: begin lock_held <= 0; phase <= TRIM; end
                TRIM: if (request_valid && request_ready) phase <= TRIM_WAIT;
                TRIM_WAIT: if (response_valid) begin
                    released_blocks <= released_blocks+1; released <= 0;
                    if (grant) blocks_left <= grant_blocks == 0 ? 0 : grant_blocks-1;
                    else if (blocks_left != 0) blocks_left <= blocks_left-1;
                    phase <= IDLE;
                end
                FAULT: phase <= FAULT;
                default: begin fault <= 1; phase <= FAULT; end
            endcase
            if (response_valid && response_error != 0) begin fault <= 1; lock_held <= 0; phase <= FAULT; end
        end
    end
endmodule
