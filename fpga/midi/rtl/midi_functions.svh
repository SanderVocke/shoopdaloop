function automatic [2:0] packet_error(input [23:0] packet, input [1:0] length);
    reg [7:0] status;
    reg [1:0] expected;
    begin
        status=packet[7:0]; expected=0; packet_error=0;
        if(status>=8'h80 && status<=8'hef) expected=(status[7:4]==12 || status[7:4]==13) ? 2 : 3;
        else case(status)
            8'hf1,8'hf3: expected=2;
            8'hf2: expected=3;
            8'hf6,8'hf8,8'hfa,8'hfb,8'hfc,8'hfe,8'hff: expected=1;
            default: packet_error=status>=8'hf0 ? 2 : 1;
        endcase
        if(packet_error==0 && (length!=expected || (length>=2 && packet[15]) || (length==3 && packet[23]))) packet_error=1;
    end
endfunction
