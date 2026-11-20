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
