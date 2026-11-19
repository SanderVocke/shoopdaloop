module midi_state_service (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready, input wire [2:0] command_op,
    input wire [63:0] command_current, input wire [63:0] command_target,
    input wire [63:0] command_map, input wire [63:0] command_directory,
    input wire command_track_activity, input wire [63:0] command_activity,
    input wire command_track_changes, input wire [63:0] command_changes,
    input wire [31:0] command_pairs, input wire [12:0] command_first, input wire [12:0] command_count,
    input wire [15:0] command_channels, input wire [23:0] command_packet, input wire [1:0] command_length,
    output wire response_valid, input wire response_ready, output reg [2:0] response_error,
    output wire packet_valid, input wire packet_ready, input wire packet_refused,
    output wire [23:0] packet_data, output wire [1:0] packet_length,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    `include "midi_functions.svh"
    localparam [4:0] IDLE=0, STATE_START=1, STATE_WAIT=2, DIRECTORY=3, DIRECTORY_WAIT=4,
        SNAPSHOT=5, SNAPSHOT_WAIT=6, DIFF_START=7, DIFF_WAIT=8, CHANGE_ACK=9,
        TRANSITION_START=10, TRANSITION_WAIT=11, TRANSITION_ACK=12, RESPONSE=13, MASK=14, MASK_WAIT=15, MASK_WRITE=16, MASK_ACK=17;
    reg [4:0] phase;
    reg [2:0] op;
    reg [63:0] current_base, target_base, map_base, directory_base, activity_base;
    reg track_activity, updating_activity, track_changes;
    reg [63:0] changes_base;
    reg [31:0] changes_word;
    wire effective_changes = track_changes && !updating_activity;
    wire mask_memory = phase == MASK || phase == MASK_WAIT || phase == MASK_WRITE || phase == MASK_ACK;
    wire [63:0] state_base = updating_activity ? activity_base : current_base;
    wire [31:0] effective_pairs = updating_activity ? 0 : pairs;
    reg [31:0] pairs, pair_index;
    reg [12:0] first_cell, cell_count, changed_index;
    reg [15:0] channels;
    reg [23:0] packet;
    reg [1:0] length;
    reg [3:0] cursor;
    reg [31:0] entry [0:11];
    reg [31:0] snapshot_generation;
    reg [1:0] snapshot_word;
    reg returning_to_transition, refused;
    wire state_change_valid, state_change_ready;
    wire [12:0] state_change_cell;
    wire [31:0] state_change_data;
    wire transition_packet_valid, transition_packet_ready;
    wire [23:0] transition_packet;
    wire [1:0] transition_length;
    wire state_response_valid, diff_response_valid, transition_response_valid;
    wire [2:0] state_error, transition_error;
    wire diff_error;
    wire state_command_ready, diff_command_ready, transition_command_ready;
    wire state_memory_valid, state_memory_ready, state_memory_write;
    wire [63:0] state_memory_address;
    wire [31:0] state_memory_write_data;
    wire state_memory_response_valid, state_memory_response_ready;
    wire diff_memory_valid, diff_memory_ready, diff_memory_write;
    wire [63:0] diff_memory_address;
    wire [31:0] diff_memory_write_data;
    wire diff_memory_response_valid, diff_memory_response_ready;
    wire transition_memory_valid, transition_memory_ready, transition_memory_write;
    wire [63:0] transition_memory_address;
    wire [31:0] transition_memory_write_data;
    wire transition_memory_response_valid, transition_memory_response_ready;
    wire own_memory = phase == DIRECTORY || phase == DIRECTORY_WAIT || phase == SNAPSHOT || phase == SNAPSHOT_WAIT || mask_memory;
    wire select_diff = phase == DIFF_START || phase == DIFF_WAIT;
    wire select_transition = phase == TRANSITION_START || phase == TRANSITION_WAIT || phase == TRANSITION_ACK;
    wire [63:0] own_address = mask_memory ? changes_base+64'(changed_index >> 5)*4 : phase == SNAPSHOT || phase == SNAPSHOT_WAIT ?
        {entry[9],entry[8]} + 64'(snapshot_word)*4 : directory_base+64'(pair_index)*48+64'(cursor)*4;
    assign memory_valid = own_memory ? (phase == DIRECTORY || phase == SNAPSHOT || phase == MASK || phase == MASK_WRITE) :
        select_diff ? diff_memory_valid : select_transition ? transition_memory_valid : state_memory_valid;
    assign memory_write = own_memory ? phase == MASK_WRITE : select_diff ? diff_memory_write :
        select_transition ? transition_memory_write : state_memory_write;
    assign memory_address = own_memory ? own_address : select_diff ? diff_memory_address :
        select_transition ? transition_memory_address : state_memory_address;
    assign memory_write_data = own_memory ? changes_word | (32'b1 << changed_index[4:0]) : select_diff ? diff_memory_write_data :
        select_transition ? transition_memory_write_data : state_memory_write_data;
    assign memory_response_ready = own_memory ? (phase == DIRECTORY_WAIT || phase == SNAPSHOT_WAIT || phase == MASK_WAIT || phase == MASK_ACK) :
        select_diff ? diff_memory_response_ready : select_transition ? transition_memory_response_ready : state_memory_response_ready;
    assign state_memory_ready = (!own_memory && !select_diff && !select_transition) && memory_ready;
    assign state_memory_response_valid = (!own_memory && !select_diff && !select_transition) && memory_response_valid;
    assign diff_memory_ready = (select_diff) && memory_ready;
    assign diff_memory_response_valid = (select_diff) && memory_response_valid;
    assign transition_memory_ready = (select_transition) && memory_ready;
    assign transition_memory_response_valid = (select_transition) && memory_response_valid;
    assign command_ready = phase == IDLE && !reset;
    assign response_valid = phase == RESPONSE && !reset;
    assign packet_valid = phase == TRANSITION_WAIT && (op == 5 || transition_packet_valid) && !reset;
    assign packet_data = op == 5 ? packet : transition_packet;
    assign packet_length = op == 5 ? length : transition_length;
    assign state_change_ready = phase == CHANGE_ACK || (phase == STATE_WAIT && effective_pairs == 0 && !effective_changes);
    assign transition_packet_ready = phase == TRANSITION_ACK;
    wire unit_reset = reset || (phase == RESPONSE && response_error != 0);
    midi_state state_engine (
        .clk(clk), .reset(unit_reset), .command_valid(phase == STATE_START), .command_ready(state_command_ready),
        .command_initialize(op == 0), .command_base(state_base),
        .command_packet(packet), .command_length(length),
        .response_valid(state_response_valid), .response_ready(phase == STATE_WAIT), .response_error(state_error),
        .changed_valid(state_change_valid), .changed_ready(state_change_ready),
        .changed_cell(state_change_cell), .changed_data(state_change_data),
        .memory_valid(state_memory_valid),
        .memory_ready(state_memory_ready),
        .memory_write(state_memory_write),
        .memory_address(state_memory_address),
        .memory_write_data(state_memory_write_data),
        .memory_response_valid(state_memory_response_valid),
        .memory_response_ready(state_memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data)
    );
    midi_difference difference_engine (
        .clk(clk), .reset(unit_reset), .command_valid(phase == DIFF_START), .command_ready(diff_command_ready),
        .command_target(op == 2 ? target_base : {entry[3],entry[2]}), .command_current(current_base),
        .command_map(op == 2 ? map_base : {entry[7],entry[6]}),
        .command_first(op == 2 ? first_cell : changed_index), .command_count(op == 2 ? cell_count : 13'd1),
        .response_valid(diff_response_valid), .response_ready(phase == DIFF_WAIT), .response_error(diff_error),
        .memory_valid(diff_memory_valid),
        .memory_ready(diff_memory_ready),
        .memory_write(diff_memory_write),
        .memory_address(diff_memory_address),
        .memory_write_data(diff_memory_write_data),
        .memory_response_valid(diff_memory_response_valid),
        .memory_response_ready(diff_memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data)
    );
    midi_transition transition_engine (
        .clk(clk), .reset(unit_reset), .command_valid(phase == TRANSITION_START), .command_ready(transition_command_ready),
        .command_target(target_base), .command_current(op == 4 && track_activity ? activity_base : current_base), .command_map(map_base),
        .command_channels(channels), .command_cleanup(op == 4),
        .response_valid(transition_response_valid), .response_ready(phase == TRANSITION_WAIT), .response_error(transition_error),
        .packet_valid(transition_packet_valid), .packet_ready(transition_packet_ready), .packet_refused(refused),
        .packet_data(transition_packet), .packet_length(transition_length),
        .memory_valid(transition_memory_valid),
        .memory_ready(transition_memory_ready),
        .memory_write(transition_memory_write),
        .memory_address(transition_memory_address),
        .memory_write_data(transition_memory_write_data),
        .memory_response_valid(transition_memory_response_valid),
        .memory_response_ready(transition_memory_response_ready),
        .memory_response_error(memory_response_error), .memory_read_data(memory_read_data)
    );
    task automatic next_pair;
        begin
            if (pair_index+1 == pairs) phase <= effective_changes ? MASK : CHANGE_ACK;
            else begin pair_index <= pair_index+1; cursor <= 0; phase <= DIRECTORY; end
        end
    endtask
    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; op <= 0; current_base <= 0; target_base <= 0; map_base <= 0;
            directory_base <= 0; changes_base <= 0; changes_word <= 0; track_changes <= 0; activity_base <= 0; track_activity <= 0; updating_activity <= 0; pairs <= 0; pair_index <= 0; first_cell <= 0; cell_count <= 0;
            changed_index <= 0; channels <= 0; packet <= 0; length <= 0; cursor <= 0;
            snapshot_generation <= 0; snapshot_word <= 0; returning_to_transition <= 0; refused <= 0;
            response_error <= 0;
        end else case (phase)
            IDLE: if (command_valid) begin
                op <= command_op; current_base <= command_current; target_base <= command_target;
                map_base <= command_map; directory_base <= command_directory; pairs <= command_pairs;
                activity_base <= command_activity; track_activity <= command_track_activity; updating_activity <= 0;
                track_changes <= command_track_changes; changes_base <= command_changes;
                first_cell <= command_first; cell_count <= command_count; channels <= command_channels;
                packet <= command_packet; length <= command_length; response_error <= 0;
                returning_to_transition <= 0; refused <= 0;
                if (command_op <= 1) phase <= STATE_START;
                else if (command_op == 2) phase <= DIFF_START;
                else if (command_op <= 4) phase <= TRANSITION_START;
                else if (command_op == 5) begin
                    if (packet_error(command_packet,command_length) != 0) begin
                        response_error <= packet_error(command_packet,command_length); phase <= RESPONSE;
                    end else phase <= TRANSITION_WAIT;
                end
                else begin response_error <= 4; phase <= RESPONSE; end
            end
            STATE_START: if (state_command_ready) phase <= STATE_WAIT;
            STATE_WAIT: begin
                if (state_response_valid) begin
                    if (state_error != 0) begin response_error <= state_error; phase <= RESPONSE; end
                    else if (returning_to_transition && track_activity && !updating_activity) begin
                        updating_activity <= 1; phase <= STATE_START;
                    end else begin updating_activity <= 0; phase <= returning_to_transition && op != 5 ? TRANSITION_ACK : RESPONSE; end
                end else if (state_change_valid && (effective_pairs != 0 || effective_changes)) begin
                    changed_index <= state_change_cell; pair_index <= 0; cursor <= 0; phase <= effective_pairs == 0 ? MASK : DIRECTORY;
                end
            end
            DIRECTORY: if (memory_ready) phase <= DIRECTORY_WAIT;
            DIRECTORY_WAIT: if (memory_response_valid) begin
                if (memory_response_error) begin response_error <= 7; phase <= RESPONSE; end
                else begin
                    entry[cursor] <= memory_read_data;
                    if (cursor == 0 && memory_read_data == 0) next_pair();
                    else if (cursor == 11) begin
                        if (entry[0] != 0 && {entry[5],entry[4]} == current_base && memory_read_data > {19'b0,changed_index}) begin
                            snapshot_word <= 0; phase <= SNAPSHOT;
                        end else next_pair();
                    end else begin cursor <= cursor+1; phase <= DIRECTORY; end
                end
            end
            SNAPSHOT: if (memory_ready) phase <= SNAPSHOT_WAIT;
            SNAPSHOT_WAIT: if (memory_response_valid) begin
                if (memory_response_error) begin response_error <= 7; phase <= RESPONSE; end
                else if (snapshot_word == 0) begin snapshot_generation <= memory_read_data; snapshot_word <= 1; phase <= SNAPSHOT; end
                else if (memory_read_data != 0 && snapshot_generation == entry[10]) phase <= DIFF_START;
                else next_pair();
            end
            DIFF_START: if (diff_command_ready) phase <= DIFF_WAIT;
            DIFF_WAIT: if (diff_response_valid) begin
                if (diff_error) begin response_error <= 7; phase <= RESPONSE; end
                else if (op == 2) phase <= RESPONSE;
                else next_pair();
            end
            MASK: if (memory_ready) phase <= MASK_WAIT;
            MASK_WAIT: if (memory_response_valid) begin
                if (memory_response_error) begin response_error <= 7; phase <= RESPONSE; end
                else begin changes_word <= memory_read_data; phase <= MASK_WRITE; end
            end
            MASK_WRITE: if (memory_ready) phase <= MASK_ACK;
            MASK_ACK: if (memory_response_valid) begin
                if (memory_response_error) begin response_error <= 7; phase <= RESPONSE; end
                else phase <= CHANGE_ACK;
            end
            CHANGE_ACK: phase <= STATE_WAIT;
            TRANSITION_START: if (transition_command_ready) phase <= TRANSITION_WAIT;
            TRANSITION_WAIT: if (op != 5 && transition_response_valid) begin response_error <= transition_error; phase <= RESPONSE; end
                else if ((op == 5 || transition_packet_valid) && packet_ready) begin
                    refused <= packet_refused;
                    if (packet_refused) begin
                        if (op == 5) begin response_error <= 5; phase <= RESPONSE; end
                        else phase <= TRANSITION_ACK;
                    end
                    else begin
                        if (op != 5) begin packet <= transition_packet; length <= transition_length; end
                        returning_to_transition <= 1; phase <= STATE_START;
                    end
                end
            TRANSITION_ACK: phase <= TRANSITION_WAIT;
            RESPONSE: if (response_ready) phase <= IDLE;
            default: phase <= IDLE;
        endcase
    end
endmodule
