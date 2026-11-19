function automatic [31:0] initial_cell(input integer field);
    if (field < 128) initial_cell = 32'h00800000;
    else if (field < 256)
        initial_cell = (field == 192 || field == 194 || field == 197) ? 0 : 128;
    else if (field == 259) initial_cell = 8192;
    else if (field <= 260) initial_cell = 128;
    else initial_cell = 0;
endfunction
function automatic [2:0] packet_error(input [23:0] packet, input [1:0] length);
    reg [7:0] status, a, b;
    begin
        status = packet[7:0]; a = packet[15:8]; b = packet[23:16];
        packet_error = 0;
        if (status >= 8'hf0 && status != 8'hff) packet_error = 2;
        else if (status < 8'h80 || length == 0) packet_error = 1;
        else if (status == 8'hff) begin
            if (length != 1) packet_error = 1;
        end else if (a[7] || (length == 3 && b[7]) ||
            length != ((status[7:4] == 12 || status[7:4] == 13) ? 2 : 3))
            packet_error = 1;
        else if (status[7:4] == 11 && (a == 6 || a == 38 || (a >= 96 && a <= 101)))
            packet_error = 2;
    end
endfunction
function automatic bit cell_diff(input integer field, input [31:0] target,
                                 input [31:0] current, input bit program_known);
    if (field < 128)
        cell_diff = target[15:0] != current[15:0] ||
            (target[23:16] < 128 && target[23:16] != current[23:16]);
    else if (field < 256 || field == 260) cell_diff = target < 128 && target != current;
    else if (field <= 258) cell_diff = program_known && target < 128 && target != current;
    else if (field == 259) cell_diff = target != current;
    else cell_diff = 0;
endfunction
