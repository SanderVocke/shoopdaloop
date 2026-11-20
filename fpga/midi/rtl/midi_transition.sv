module midi_transition (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready,
    input wire [63:0] command_target, input wire [63:0] command_current,
    input wire [63:0] command_map, input wire [15:0] command_channels,
    input wire command_cleanup,
    output wire response_valid, input wire response_ready, output reg [2:0] response_error,
    output wire packet_valid, input wire packet_ready, input wire packet_refused,
    output reg [23:0] packet_data, output reg [1:0] packet_length,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    localparam [3:0] IDLE=0, LOAD=1, LOAD_WAIT=2, SELECT=3, EXECUTE=4, PACKET=5, RESPONSE=6, FETCH_TARGET=7, FETCH_TARGET_WAIT=8, FETCH_CURRENT=9, FETCH_CURRENT_WAIT=10, PREPARE=11;
    reg [3:0] phase;
    reg [63:0] target_base, current_base, map_base;
    reg [15:0] channels;
    reg cleanup, rebase, has_diff, notes_changed, active;
    reg [4:0] channel, step;
    reg [9:0] cursor, load_cursor;
    reg [31:0] target_scalar [0:9];
    reg [31:0] target_value, current_value;
    reg [31:0] current_scalar [0:9];
    reg [31:0] differences [0:8];
    wire target_holds = (target_scalar[2] >= 64 && target_scalar[2] < 128) ||
        (target_scalar[3] >= 64 && target_scalar[3] < 128) || (target_scalar[4] >= 64 && target_scalar[4] < 128);
    wire current_holds = (current_scalar[2] >= 64 && current_scalar[2] < 128) ||
        (current_scalar[3] >= 64 && current_scalar[3] < 128) || (current_scalar[4] >= 64 && current_scalar[4] < 128);
    wire program_dirty = target_scalar[5] < 128 && (target_scalar[5] != current_scalar[5] ||
        (target_scalar[6] < 128 && target_scalar[6] != current_scalar[6]) ||
        (target_scalar[7] < 128 && target_scalar[7] != current_scalar[7]));
    assign command_ready = phase == IDLE && !reset;
    assign response_valid = phase == RESPONSE && !reset;
    assign packet_valid = phase == PACKET && !reset;
    function automatic integer scalar_field(input integer index);
        case(index)
            0:scalar_field=128;1:scalar_field=160;2:scalar_field=192;3:scalar_field=194;4:scalar_field=197;
            5:scalar_field=256;6:scalar_field=257;7:scalar_field=258;8:scalar_field=259;default:scalar_field=260;
        endcase
    endfunction
    wire [63:0] requested_field = step == 6 ? 64'(cursor)+128 : 64'(cursor);
    assign memory_valid = ((phase == LOAD && channels[channel[3:0]]) || phase == FETCH_TARGET || phase == FETCH_CURRENT) && !reset;
    assign memory_write = 0;
    assign memory_write_data = 0;
    assign memory_address = phase == FETCH_TARGET || phase == FETCH_TARGET_WAIT ?
        target_base+(64'(channel)*264+requested_field)*4 :
        phase == FETCH_CURRENT || phase == FETCH_CURRENT_WAIT ? current_base+(64'(channel)*264+requested_field)*4 :
        load_cursor < 10 ? target_base+(64'(channel)*264+64'(scalar_field(integer'(load_cursor))))*4 :
        load_cursor < 20 ? current_base+(64'(channel)*264+64'(scalar_field(integer'(load_cursor)-10)))*4 :
        map_base+(64'(channel)*8+64'(channel >> 2)+64'(load_cursor)-20)*4;
    assign memory_response_ready = phase == LOAD_WAIT || phase == FETCH_TARGET_WAIT || phase == FETCH_CURRENT_WAIT;
    function automatic bit dirty(input integer field);
        integer cell_index;
        begin
            cell_index = integer'(channel[1:0])*8+field;
            dirty = differences[cell_index/32][cell_index%32];
        end
    endfunction
    task automatic emit(input [7:0] status, input [7:0] a, input [7:0] b, input [1:0] length);
        begin packet_data <= {b,a,status}; packet_length <= length; phase <= PACKET; end
    endtask
    task automatic control(input integer number, input [31:0] value);
        emit(8'hb0|8'(channel),8'(number),8'(value),3);
    endtask
    task automatic next_step;
        begin step <= step+1; cursor <= 0; phase <= PREPARE; end
    endtask
    always @(posedge clk) begin : execution
        integer n, field;
        reg [31:0] a, b;
        reg sounding, changed, candidate;
        if (reset) begin
            phase <= IDLE; target_base <= 0; current_base <= 0; map_base <= 0;
            target_value <= 0; current_value <= 0; channels <= 0; cleanup <= 0; rebase <= 0; has_diff <= 0; notes_changed <= 0; active <= 0;
            channel <= 0; step <= 0; cursor <= 0; load_cursor <= 0; response_error <= 0; packet_data <= 0; packet_length <= 0;
        end else case (phase)
            IDLE: if (command_valid) begin
                target_base <= command_target; current_base <= command_current; map_base <= command_map;
                channels <= command_channels; cleanup <= command_cleanup; channel <= 0;
                cursor <= 0; load_cursor <= 0; step <= 0; response_error <= 0; phase <= LOAD;
                has_diff <= 0; notes_changed <= 0; active <= 0; rebase <= 0;
            end
            LOAD: if (!channels[channel[3:0]]) begin
                if (channel == 15) phase <= RESPONSE; else channel <= channel+1;
            end else if (memory_ready) phase <= LOAD_WAIT;
            LOAD_WAIT: if (memory_response_valid) begin
                if (memory_response_error) begin response_error <= 7; phase <= RESPONSE; end
                else begin
                    if (load_cursor < 10) target_scalar[load_cursor[3:0]] <= memory_read_data;
                    else if (load_cursor < 20) current_scalar[4'(load_cursor-10)] <= memory_read_data;
                    else differences[load_cursor-20] <= memory_read_data;
                    if (load_cursor == 28) begin load_cursor <= 0; phase <= PREPARE; end
                    else begin load_cursor <= load_cursor+1; phase <= LOAD; end
                end
            end
            PREPARE: if (cursor < 128 && (
                (step == 0 && (cleanup || dirty(integer'(cursor)))) ||
                (step == 6 && dirty(128+integer'(cursor))) ||
                (step == 10 && rebase) ||
                ((step == 14 || step == 17) && rebase) ||
                (step == 16 && (rebase || dirty(integer'(cursor)))) ||
                (step == 18 && dirty(integer'(cursor)))))
                phase <= cleanup ? FETCH_CURRENT : FETCH_TARGET;
                else phase <= step == 0 ? SELECT : EXECUTE;
            FETCH_TARGET: if (memory_ready) phase <= FETCH_TARGET_WAIT;
            FETCH_TARGET_WAIT: if (memory_response_valid) begin
                if (memory_response_error) begin response_error <= 7; phase <= RESPONSE; end
                else begin target_value <= memory_read_data; phase <= FETCH_CURRENT; end
            end
            FETCH_CURRENT: if (memory_ready) phase <= FETCH_CURRENT_WAIT;
            FETCH_CURRENT_WAIT: if (memory_response_valid) begin
                if (memory_response_error) begin response_error <= 7; phase <= RESPONSE; end
                else begin current_value <= memory_read_data; phase <= step == 0 ? SELECT : EXECUTE; end
            end
            SELECT: begin
                if (cursor == 264) begin
                    cursor <= 0;
                    if (channels[channel[3:0]] && (cleanup ? active || current_holds : has_diff)) begin
                        rebase <= cleanup || rebase || (notes_changed && (target_holds || current_holds));
                        step <= cleanup ? 9 : 1; phase <= EXECUTE;
                    end else if (channel == 15) phase <= RESPONSE;
                    else begin
                        channel <= channel+1; phase <= LOAD; has_diff <= 0;
                        notes_changed <= 0; active <= 0; rebase <= 0;
                    end
                end else begin
                    field = integer'(cursor);
                    if (dirty(field)) has_diff <= 1;
                    if (field < 128) begin
                        if (current_value[8] || current_value[7:0] != 0) active <= 1;
                        if (dirty(field) && target_value[15:0] != current_value[15:0]) notes_changed <= 1;
                    end
                    if ((field == 192 || field == 194 || field == 197 || field >= 252 && field <= 255)
                        && dirty(field)) rebase <= 1;
                    cursor <= cursor+1; phase <= PREPARE;
                end
            end
            EXECUTE: begin
                phase <= PREPARE;
                case (step)
                    1,2: begin
                        next_step(); n = step == 1 ? 0 : 32;
                        a = target_scalar[step == 1 ? 6 : 7];
                        if (program_dirty && a < 128 && current_scalar[n == 0 ? 0 : 1] != a) control(n,a);
                    end
                    3: begin
                        next_step(); if (program_dirty) emit(8'hc0|8'(channel),8'(target_scalar[5]),0,2);
                    end
                    4,5: begin
                        next_step(); n = step == 4 ? 0 : 32;
                        if (target_scalar[n == 0 ? 0 : 1] < 128 && current_scalar[n == 0 ? 0 : 1] != target_scalar[n == 0 ? 0 : 1]) control(n,target_scalar[n == 0 ? 0 : 1]);
                    end
                    6: begin
                        if (cursor == 128) next_step();
                        else begin
                            n = integer'(cursor); cursor <= cursor+1;
                            if (dirty(128+n) && n != 0 && n != 32 && n != 64 && n != 66 && n != 69 && target_value < 128)
                                control(n,target_value);
                        end
                    end
                    7: begin
                        next_step();
                        if (target_scalar[8] != current_scalar[8])
                            emit(8'he0|8'(channel),{1'b0,target_scalar[8][6:0]},{1'b0,target_scalar[8][13:7]},3);
                    end
                    8: begin
                        next_step();
                        if (target_scalar[9] < 128 && target_scalar[9] != current_scalar[9])
                            emit(8'hd0|8'(channel),8'(target_scalar[9]),0,2);
                    end
                    9: begin
                        if (!rebase || cursor == 3) next_step();
                        else begin
                            n = cursor == 0 ? 64 : cursor == 1 ? 66 : 69; cursor <= cursor+1;
                            if (current_scalar[n == 64 ? 2 : n == 66 ? 3 : 4] >= 64 && current_scalar[n == 64 ? 2 : n == 66 ? 3 : 4] < 128) control(n,0);
                        end
                    end
                    10: begin
                        if (!rebase || cursor == 128) begin step <= cleanup ? 19 : 11; cursor <= 0; end
                        else begin
                            n = integer'(cursor); cursor <= cursor+1;
                            if (current_value[8] || current_value[7:0] != 0) emit(8'h80|8'(channel),8'(n),0,3);
                        end
                    end
                    11,12: begin
                        next_step(); n = step == 11 ? 64 : 69;
                        if (target_scalar[n == 64 ? 2 : 4] < 128 && target_scalar[n == 64 ? 2 : 4] != current_scalar[n == 64 ? 2 : 4]) control(n,target_scalar[n == 64 ? 2 : 4]);
                    end
                    13: begin
                        next_step(); a = target_scalar[3] < 64 ? target_scalar[3] : 0;
                        if (rebase && current_scalar[3] != a) control(66,a);
                    end
                    14,16,17,18: begin
                        if (cursor == 128 || (!rebase && step != 16 && step != 18)) next_step();
                        else begin
                            n = integer'(cursor); a = target_value; b = current_value; cursor <= cursor+1;
                            changed = a[15:0] != b[15:0];
                            sounding = target_scalar[3] >= 64 && target_scalar[3] < 128;
                            if (step == 16 && !rebase) begin
                                if (dirty(n) && changed) begin
                                    if (a[7:0] != 0 && b[7:0] != 0) begin
                                        cursor <= cursor; emit(8'h80|8'(channel),8'(n),0,3);
                                    end else if (a[7:0] != 0) emit(8'h90|8'(channel),8'(n),a[7:0],3);
                                    else if (b[7:0] != 0 || b[8]) emit(8'h80|8'(channel),8'(n),0,3);
                                end
                            end else begin
                                candidate = step == 14 ? rebase && sounding && a[9] && a[7:0] != 0 :
                                    step == 16 ? rebase && a[7:0] != 0 && !(sounding && a[9]) :
                                    step == 17 ? rebase && a[7:0] != 0 && !a[8] :
                                    dirty(n) && a[23:16] < 128 && a[23:16] != b[23:16];
                                if (candidate) begin
                                    if (step == 17) emit(8'h80|8'(channel),8'(n),0,3);
                                    else if (step == 18) emit(8'ha0|8'(channel),8'(n),a[23:16],3);
                                    else emit(8'h90|8'(channel),8'(n),a[7:0],3);
                                end
                            end
                        end
                    end
                    15: begin
                        next_step(); if (target_scalar[3] < 128 && target_scalar[3] != current_scalar[3]) control(66,target_scalar[3]);
                    end
                    19: begin
                        if (channel == 15) phase <= RESPONSE;
                        else begin
                            channel <= channel+1; step <= 0; cursor <= 0; phase <= LOAD;
                            has_diff <= 0; notes_changed <= 0; active <= 0; rebase <= 0;
                        end
                    end
                    default: begin response_error <= 4; phase <= RESPONSE; end
                endcase
            end
            PACKET: if (packet_ready) begin
                if (packet_refused) begin response_error <= 5; phase <= RESPONSE; end
                else begin load_cursor <= 0; phase <= LOAD; end
            end
            RESPONSE: if (response_ready) phase <= IDLE;
            default: phase <= IDLE;
        endcase
    end
endmodule
