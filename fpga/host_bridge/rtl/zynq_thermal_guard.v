module zynq_thermal_guard(input wire clk);
    // Autonomous OT protection must survive unused fabric outputs.
    (* DONT_TOUCH = "TRUE" *)
    XADC #(
        .INIT_40(16'h0000),
        .INIT_41(16'h0000),
        .INIT_42(16'h0800),
        .INIT_53(16'hB0D3),
        .INIT_57(16'hABC4),
        .SIM_DEVICE("ZYNQ")
    ) monitor (
        .DCLK(clk),
        .DADDR(7'b0), .DEN(1'b0), .DI(16'b0), .DWE(1'b0),
        .CONVST(1'b0), .CONVSTCLK(1'b0), .RESET(1'b0),
        .VAUXP(16'b0), .VAUXN(16'b0), .VP(1'b0), .VN(1'b0),
        .ALM(), .OT(), .BUSY(), .CHANNEL(), .DO(), .DRDY(),
        .EOC(), .EOS(), .JTAGBUSY(), .JTAGLOCKED(),
        .JTAGMODIFIED(), .MUXADDR()
    );
endmodule
