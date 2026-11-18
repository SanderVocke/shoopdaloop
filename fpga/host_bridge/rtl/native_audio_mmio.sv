module native_audio_mmio #(
    parameter [31:0] CORE_CLOCK_HZ = 32'd100000000
) (
    (* X_INTERFACE_INFO = "xilinx.com:signal:clock:1.0 clk CLK", X_INTERFACE_PARAMETER = "ASSOCIATED_BUSIF s_axi, ASSOCIATED_RESET reset" *)
    input wire clk,
    (* X_INTERFACE_INFO = "xilinx.com:signal:reset:1.0 reset RST", X_INTERFACE_PARAMETER = "POLARITY ACTIVE_HIGH" *)
    input wire reset,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi AWADDR", X_INTERFACE_PARAMETER = "PROTOCOL AXI4LITE" *)
    input wire [11:0] s_axi_awaddr,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi AWVALID" *)
    input wire s_axi_awvalid,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi AWREADY" *)
    output wire s_axi_awready,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi WDATA" *)
    input wire [31:0] s_axi_wdata,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi WSTRB" *)
    input wire [3:0] s_axi_wstrb,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi WVALID" *)
    input wire s_axi_wvalid,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi WREADY" *)
    output wire s_axi_wready,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi BRESP" *)
    output wire [1:0] s_axi_bresp,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi BVALID" *)
    output reg s_axi_bvalid,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi BREADY" *)
    input wire s_axi_bready,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi ARADDR" *)
    input wire [11:0] s_axi_araddr,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi ARVALID" *)
    input wire s_axi_arvalid,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi ARREADY" *)
    output wire s_axi_arready,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi RDATA" *)
    output reg [31:0] s_axi_rdata,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi RRESP" *)
    output wire [1:0] s_axi_rresp,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi RVALID" *)
    output reg s_axi_rvalid,
    (* X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 s_axi RREADY" *)
    input wire s_axi_rready,
    (* X_INTERFACE_INFO = "xilinx.com:signal:interrupt:1.0 irq INTERRUPT", X_INTERFACE_PARAMETER = "SENSITIVITY LEVEL_HIGH" *)
    output wire irq,
    output reg audio_enable,
    input wire audio_fault,
    input wire [31:0] audio_xruns,
    input wire quantum_valid,
    output wire quantum_ready,
    output wire quantum_done,
    output wire sample_input_read,
    output wire [31:0] sample_input_address,
    input wire [31:0] sample_input_data,
    output wire sample_output_write,
    output wire [31:0] sample_output_address,
    output wire [31:0] sample_output_data
);
    import native_audio_registers::*;

    localparam [11:0] RX_END = REG_RX_BASE + 12'(4*AUDIO_QUANTUM);
    localparam [11:0] TX_END = REG_TX_BASE + 12'(8*AUDIO_QUANTUM);
    localparam integer TX_INDEX_BITS = $clog2(2*AUDIO_QUANTUM);
    reg enabled_once, pending, read_pcm;
    reg aw_pending, w_pending;
    reg [11:0] write_address;
    reg [31:0] write_data;
    reg [3:0] write_strobes;
    reg [2*AUDIO_QUANTUM-1:0] output_written;
    reg [31:0] sequence_number, elapsed, peak_cycles;

    wire active = pending && !audio_fault;
    wire write_word = aw_pending && w_pending && !s_axi_bvalid && &write_strobes;
    wire stop_audio = write_word && write_address == REG_ENABLE && write_data == 0;
    wire [TX_INDEX_BITS-1:0] tx_word = TX_INDEX_BITS'((write_address - REG_TX_BASE) >> 2);
    assign quantum_ready = audio_enable && !pending && !audio_fault && !stop_audio;
    assign quantum_done = active && write_word && write_address == REG_COMPLETE
        && write_data == sequence_number && (&output_written);
    assign irq = audio_enable && (active || audio_fault);
    assign s_axi_awready = !reset && !aw_pending && !s_axi_bvalid;
    assign s_axi_wready = !reset && !w_pending && !s_axi_bvalid;
    assign s_axi_arready = !reset && !read_pcm && !s_axi_rvalid;
    assign s_axi_bresp = 2'b00;
    assign s_axi_rresp = 2'b00;
    assign sample_input_read = s_axi_arready && s_axi_arvalid && active && s_axi_araddr[1:0] == 0
        && s_axi_araddr >= REG_RX_BASE && s_axi_araddr < RX_END;
    assign sample_input_address = (32'(s_axi_araddr) - 32'(REG_RX_BASE)) >> 2;
    assign sample_output_write = active && write_word && write_address[1:0] == 0
        && write_address >= REG_TX_BASE && write_address < TX_END;
    assign sample_output_address = 32'(tx_word);
    assign sample_output_data = write_data;

    always @(posedge clk) begin
        if (reset) begin
            audio_enable <= 0;
            enabled_once <= 0;
            pending <= 0;
            sequence_number <= 0;
            elapsed <= 0;
            peak_cycles <= 0;
            output_written <= 0;
            aw_pending <= 0;
            w_pending <= 0;
            write_address <= 0;
            write_data <= 0;
            write_strobes <= 0;
            s_axi_bvalid <= 0;
            s_axi_rvalid <= 0;
            s_axi_rdata <= 0;
            read_pcm <= 0;
        end else begin
            if (s_axi_awready && s_axi_awvalid) begin
                aw_pending <= 1;
                write_address <= s_axi_awaddr;
            end
            if (s_axi_wready && s_axi_wvalid) begin
                w_pending <= 1;
                write_data <= s_axi_wdata;
                write_strobes <= s_axi_wstrb;
            end
            if (s_axi_bready) s_axi_bvalid <= 0;
            if (aw_pending && w_pending && !s_axi_bvalid) begin
                aw_pending <= 0;
                w_pending <= 0;
                s_axi_bvalid <= 1;
            end

            if (write_word && write_address == REG_ENABLE && write_data == 1 && !enabled_once) begin
                audio_enable <= 1;
                enabled_once <= 1;
            end
            if (stop_audio) audio_enable <= 0;
            if (quantum_valid && quantum_ready) begin
                pending <= 1;
                sequence_number <= sequence_number + 1;
                elapsed <= 0;
                output_written <= 0;
            end else if (pending) begin
                if (elapsed != 32'hffffffff) elapsed <= elapsed + 1;
                if (quantum_done || audio_fault || stop_audio) begin
                    pending <= 0;
                    if (elapsed > peak_cycles) peak_cycles <= elapsed;
                end
                if (sample_output_write) output_written[tx_word] <= 1;
            end

            if (s_axi_rready) s_axi_rvalid <= 0;
            if (read_pcm) begin
                s_axi_rdata <= sample_input_data;
                s_axi_rvalid <= 1;
                read_pcm <= 0;
            end else if (s_axi_arready && s_axi_arvalid) begin
                if (sample_input_read) read_pcm <= 1;
                else begin
                    s_axi_rvalid <= 1;
                    case (s_axi_araddr)
                        REG_ID: s_axi_rdata <= AUDIO_ID;
                        REG_VERSION: s_axi_rdata <= AUDIO_VERSION;
                        REG_QUANTUM: s_axi_rdata <= AUDIO_QUANTUM;
                        REG_SAMPLE_RATE: s_axi_rdata <= AUDIO_SAMPLE_RATE;
                        REG_CLOCK_HZ: s_axi_rdata <= CORE_CLOCK_HZ;
                        REG_STATUS: s_axi_rdata <= (32'(active) << STATUS_PENDING) | (32'(audio_fault) << STATUS_FAULT);
                        REG_SEQUENCE: s_axi_rdata <= sequence_number;
                        REG_XRUNS: s_axi_rdata <= audio_xruns;
                        REG_PEAK_CYCLES: s_axi_rdata <= peak_cycles;
                        default: s_axi_rdata <= 0;
                    endcase
                end
            end
        end
    end
endmodule
