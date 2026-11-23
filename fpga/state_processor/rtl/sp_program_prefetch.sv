module sp_program_prefetch (
    input wire clk, input wire reset,
    input wire active, input wire restart,
    input wire [15:0] restart_address,
    input wire [16:0] restart_count,
    input wire take_instruction,
    output reg instruction_valid,
    output reg [15:0] instruction_address,
    output reg [95:0] instruction_data,
    output reg instruction_error,
    input wire cfg_valid, output wire cfg_ready, output wire cfg_busy,
    input wire cfg_write, input wire [15:0] cfg_address, input wire [95:0] cfg_write_data,
    output reg cfg_response_valid, output reg cfg_response_error,
    output wire [95:0] cfg_read_data,
    output wire memory_program_valid, input wire memory_program_ready,
    output reg memory_program_write,
    output reg [15:0] memory_program_address, output wire [31:0] memory_program_count,
    output wire [31:0] memory_program_write_data,
    output wire memory_program_write_valid, input wire memory_program_write_ready,
    input wire memory_program_read_valid, output wire memory_program_read_ready,
    input wire [31:0] memory_program_read_data,
    input wire memory_program_response_valid, output wire memory_program_response_ready,
    input wire memory_program_response_error
);
    typedef enum logic [1:0] {IDLE, REQUEST, TRANSFER} transfer_t;
    transfer_t transfer;
    reg cfg_transfer, discarded;
    reg [2:0] request_instructions;
    reg [3:0] transfer_word;
    reg [15:0] next_address;
    reg [16:0] remaining;
    reg [95:0] cfg_payload;

    reg [6:0] head, tail;
    wire [6:0] occupancy = tail - head;
    wire [2:0] batch = remaining >= 17'd4 ? 3'd4 : remaining[2:0];
    wire memory_word = memory_program_read_valid || (memory_program_write_valid && memory_program_write_ready);
    wire fill = transfer == TRANSFER && !cfg_transfer && !discarded && !restart && memory_program_read_valid;
    wire pull = instruction_valid && take_instruction && !instruction_error && !restart;

    assign cfg_ready = transfer == IDLE && !reset;
    assign cfg_busy = transfer != IDLE && cfg_transfer;
    assign cfg_read_data = cfg_payload;
    assign memory_program_valid = transfer == REQUEST && !reset;
    assign memory_program_count = {27'b0, request_instructions, 2'b0};
    assign memory_program_write_valid = transfer == TRANSFER && memory_program_write && !reset;
    assign memory_program_write_data = transfer_word[1:0] == 0 ? cfg_payload[31:0]
        : transfer_word[1:0] == 1 ? cfg_payload[63:32]
        : transfer_word[1:0] == 2 ? cfg_payload[95:64] : 32'b0;
    assign memory_program_read_ready = transfer == TRANSFER && !reset;
    assign memory_program_response_ready = transfer == TRANSFER && !reset;

    // The four-word stride aligns each record. The fourth word is padding.
    (* ram_style = "block" *) reg [31:0] fifo [0:255];
    reg [31:0] read_word;
    reg [1:0] read_step;
    always @(posedge clk) begin
        if (fill && transfer_word[1:0] != 3)
            fifo[{tail[5:0],transfer_word[1:0]}] <= memory_program_read_data;
        if (!instruction_valid && head != tail && read_step != 3)
            read_word <= fifo[{head[5:0],read_step}];
    end

    always @(posedge clk) begin
        if (reset || restart) begin
            head <= 0;
            instruction_address <= reset ? 16'd0 : restart_address;
            read_step <= 0;
            instruction_valid <= 0;
            instruction_data <= 0;
        end else if (pull) begin
            head <= head + 1'b1;
            instruction_address <= instruction_address + 1'b1;
            read_step <= 0;
            instruction_valid <= 0;
        end else if (!instruction_valid && head != tail) begin
            read_step <= read_step + 1'b1;
            case (read_step)
                1: instruction_data[31:0] <= read_word;
                2: instruction_data[63:32] <= read_word;
                3: begin
                    instruction_data[95:64] <= read_word;
                    instruction_valid <= 1;
                end
                default: begin end
            endcase
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            transfer <= IDLE;
            cfg_transfer <= 0; discarded <= 0;
            request_instructions <= 0; transfer_word <= 0;
            memory_program_write <= 0; memory_program_address <= 0;
            next_address <= 0; remaining <= 0; tail <= 0;
            instruction_error <= 0;
            cfg_payload <= 0; cfg_response_valid <= 0; cfg_response_error <= 0;
        end else begin
            cfg_response_valid <= 0;
            case (transfer)
                IDLE: begin
                    discarded <= 0;
                    transfer_word <= 0;
                    if (cfg_valid) begin
                        cfg_transfer <= 1;
                        memory_program_write <= cfg_write;
                        memory_program_address <= cfg_address;
                        cfg_payload <= cfg_write_data;
                        request_instructions <= 1;
                        transfer <= REQUEST;
                    end else if (active && !restart && !instruction_error && remaining != 0 && occupancy <= 7'd60) begin
                        cfg_transfer <= 0;
                        memory_program_write <= 0;
                        memory_program_address <= next_address;
                        request_instructions <= batch;
                        next_address <= next_address + 16'(batch);
                        remaining <= remaining - 17'(batch);
                        transfer <= REQUEST;
                    end
                end
                REQUEST: if (memory_program_ready) transfer <= TRANSFER;
                TRANSFER: begin
                    if (memory_word) transfer_word <= transfer_word + 1'b1;
                    if (fill && transfer_word[1:0] == 3) tail <= tail + 1'b1;
                    if (cfg_transfer && memory_program_read_valid) begin
                        case (transfer_word[1:0])
                            0: cfg_payload[31:0] <= memory_program_read_data;
                            1: cfg_payload[63:32] <= memory_program_read_data;
                            2: cfg_payload[95:64] <= memory_program_read_data;
                            default: begin end
                        endcase
                    end
                    if (memory_program_response_valid) begin
                        transfer <= IDLE;
                        if (cfg_transfer) begin
                            cfg_response_valid <= 1;
                            cfg_response_error <= memory_program_response_error;
                        end else if (!discarded && memory_program_response_error)
                            instruction_error <= 1;
                    end
                end
                default: transfer <= IDLE;
            endcase
            if (restart) begin
                if (transfer != IDLE && !cfg_transfer) discarded <= 1;
                next_address <= restart_address;
                remaining <= restart_count;
                instruction_error <= 0;
                tail <= 0;
            end
        end
    end
endmodule
