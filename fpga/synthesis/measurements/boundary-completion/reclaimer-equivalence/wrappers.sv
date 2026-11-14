module old_boundary #(
    parameter integer GUARD_CYCLES = 1024
) (
    input wire clk, input wire reset, input wire enable, input wire drain,
    input wire grant, input wire [31:0] grant_cycles, input wire [31:0] grant_blocks,
    input wire manual_grant, input wire [31:0] manual_cycles, input wire [31:0] manual_blocks,
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
reclaimer_reference #(.GUARD_CYCLES(GUARD_CYCLES)) core (.clk(clk),.reset(reset),.enable(enable),.drain(drain),.grant(grant || manual_grant),.grant_cycles(manual_grant && manual_cycles != 0 ? manual_cycles : grant_cycles),.grant_blocks(manual_grant && manual_blocks != 0 ? manual_blocks : grant_blocks),.pending_tables(pending_tables),.free_count(free_count),.allocated_count(allocated_count),.lock_request(lock_request),.lock_grant(lock_grant),.lock_held(lock_held),.request_valid(request_valid),.request_ready(request_ready),.request_opcode(request_opcode),.request_index(request_index),.request_data(request_data),.response_valid(response_valid),.response_data(response_data),.response_error(response_error),.retire_valid(retire_valid),.retire_ready(retire_ready),.active_block(active_block),.retire_done(retire_done),.retire_error(retire_error),.freed(freed),.released(released),.active(active),.fault(fault),.released_blocks(released_blocks),.freed_blocks(freed_blocks),.maintenance_cycles(maintenance_cycles));
endmodule

module new_boundary #(
    parameter integer GUARD_CYCLES = 1024
) (
    input wire clk, input wire reset, input wire enable, input wire drain,
    input wire grant, input wire [31:0] grant_cycles, input wire [31:0] grant_blocks,
    input wire manual_grant, input wire [31:0] manual_cycles, input wire [31:0] manual_blocks,
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
media_reclaimer #(.GUARD_CYCLES(GUARD_CYCLES)) core (.clk(clk),.reset(reset),.enable(enable),.drain(drain),.grant(grant),.grant_cycles(grant_cycles),.grant_blocks(grant_blocks),.manual_grant(manual_grant),.manual_cycles(manual_cycles),.manual_blocks(manual_blocks),.pending_tables(pending_tables),.free_count(free_count),.allocated_count(allocated_count),.lock_request(lock_request),.lock_grant(lock_grant),.lock_held(lock_held),.request_valid(request_valid),.request_ready(request_ready),.request_opcode(request_opcode),.request_index(request_index),.request_data(request_data),.response_valid(response_valid),.response_data(response_data),.response_error(response_error),.retire_valid(retire_valid),.retire_ready(retire_ready),.active_block(active_block),.retire_done(retire_done),.retire_error(retire_error),.freed(freed),.released(released),.active(active),.fault(fault),.released_blocks(released_blocks),.freed_blocks(freed_blocks),.maintenance_cycles(maintenance_cycles));
endmodule
