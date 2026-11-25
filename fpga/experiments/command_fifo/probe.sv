module fifo_edit_probe #(
    parameter integer LOOPS=128,
    parameter integer STATES=512,
    parameter integer LOOP_ADDRESS_BITS=$clog2(LOOPS*16),
    parameter integer STATE_ADDRESS_BITS=$clog2(STATES)
) (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready,
    input wire [2:0] command_kind,
    input wire [15:0] command_slot, input wire [31:0] command_generation,
    input wire command_self_sync, input wire [63:0] command_frame,
    input wire [159:0] command_args, input wire [15:0] command_gain_address,
    input wire allow_memory,
    input wire cfg_valid, input wire cfg_write, input wire cfg_state,
    input wire [15:0] cfg_address, input wire [128:0] cfg_write_data,
    output reg cfg_response_valid, output reg [128:0] cfg_read_data,
    output wire result_valid, input wire result_ready,
    output wire [3:0] result_code, output wire write_fault,
    output wire [31:0] command_cycles
);
    (* ram_style="block" *) reg [63:0] loop_memory [0:LOOPS*16-1];
    (* ram_style="block" *) reg [128:0] state_memory [0:STATES-1];
    wire memory_valid, memory_write, memory_state;
    wire [15:0] memory_address;
    wire [128:0] memory_write_data;
    reg memory_response_valid, memory_response_error;
    reg [128:0] memory_read_data;
    wire memory_ready=allow_memory;
    wire accept=memory_valid && memory_ready;
    wire in_range=memory_state ? 32'(memory_address)<32'(STATES) : 32'(memory_address)<32'(LOOPS*16);
    wire external_access=cfg_valid && command_ready && !command_valid;
    wire access_valid=(accept && in_range) || external_access;
    wire access_state=accept ? memory_state : cfg_state;
    wire access_write=accept ? memory_write : cfg_write;
    wire [15:0] access_address=accept ? memory_address : cfg_address;
    wire [128:0] access_data=accept ? memory_write_data : cfg_write_data;
    reg [63:0] loop_read;
    reg [128:0] state_read;
    reg response_state;

    always @(posedge clk) begin
        if (access_valid && !access_state) begin
            loop_read<=loop_memory[access_address[LOOP_ADDRESS_BITS-1:0]];
            if (access_write) loop_memory[access_address[LOOP_ADDRESS_BITS-1:0]]<=access_data[63:0];
        end
        if (access_valid && access_state) begin
            state_read<=state_memory[access_address[STATE_ADDRESS_BITS-1:0]];
            if (access_write) state_memory[access_address[STATE_ADDRESS_BITS-1:0]]<=access_data;
        end
        if (reset) begin
            memory_response_valid<=0; memory_response_error<=0;
            cfg_response_valid<=0; response_state<=0;
        end else begin
            memory_response_valid<=accept;
            memory_response_error<=accept && !in_range;
            cfg_response_valid<=external_access;
            if (access_valid) response_state<=access_state;
        end
    end
    always @* begin
        memory_read_data=response_state ? state_read : {65'd0,loop_read};
        cfg_read_data=memory_read_data;
    end
    fifo_edit_kernel #(.LOOPS(LOOPS)) kernel (.*);
endmodule
