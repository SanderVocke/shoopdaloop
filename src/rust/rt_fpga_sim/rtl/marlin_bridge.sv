// Generated Marlin top for the CPU host bridge; do not edit by hand.
// The shared core profile behind the AXI-lite register map, with a functional DDR model.
module marlin_bridge #(parameter integer EXTERNAL_IO = 0) (
    input wire clk, input wire reset, output wire irq,
    input wire [11:0] s_axi_awaddr, input wire s_axi_awvalid, output wire s_axi_awready,
    input wire [31:0] s_axi_wdata, input wire s_axi_wvalid, output wire s_axi_wready,
    output wire s_axi_bvalid, input wire s_axi_bready,
    input wire [11:0] s_axi_araddr, input wire s_axi_arvalid, output wire s_axi_arready,
    output wire [31:0] s_axi_rdata, output wire s_axi_rvalid, input wire s_axi_rready
);
    wire [1:0] s_axi_bresp, s_axi_rresp;
    wire m_axi_awid, m_axi_wid, m_axi_bid, m_axi_arid, m_axi_rid;
    wire [31:0] m_axi_awaddr, m_axi_araddr;
    wire [3:0] m_axi_awlen, m_axi_arlen;
    wire [2:0] m_axi_awsize, m_axi_arsize;
    wire [1:0] m_axi_awburst, m_axi_arburst, m_axi_bresp, m_axi_rresp;
    wire m_axi_awvalid, m_axi_awready, m_axi_wlast, m_axi_wvalid, m_axi_wready, m_axi_bvalid, m_axi_bready;
    wire m_axi_arvalid, m_axi_arready, m_axi_rlast, m_axi_rvalid, m_axi_rready;
    wire [63:0] m_axi_wdata, m_axi_rdata;
    wire [7:0] m_axi_wstrb;
    host_core #(.REUSE_VIEWS(1), .LOOP_DEPTH(4), .REQUEST_DEPTH(32), .EVENT_DEPTH(64), .PROGRAM_DEPTH(256), .STATE_DEPTH(256), .JOB_DEPTH(512), .MAX_FRAMES(32), .BUFFERS(128), .INPUT_PORTS(2), .OUTPUT_PORTS(2), .VIEWS(32), .SLOTS(32768), .CHUNK_FRAMES(32), .BLOCK_CHUNKS(1), .RETIRED_TABLES(256), .AXI_DATA_WIDTH(64), .TABLE_PAGES(4642), .EXTERNAL_IO(EXTERNAL_IO), .PHYSICAL_BASE(32'h01000000)) bridge (
        .clk, .reset, .irq, .s_axi_awaddr, .s_axi_awvalid, .s_axi_awready, .s_axi_wdata,
        .s_axi_wstrb(4'hf), .s_axi_wvalid, .s_axi_wready, .s_axi_bresp, .s_axi_bvalid, .s_axi_bready,
        .s_axi_araddr, .s_axi_arvalid, .s_axi_arready, .s_axi_rdata, .s_axi_rresp, .s_axi_rvalid,
        .s_axi_rready, .m_axi_awid, .m_axi_awaddr, .m_axi_awlen, .m_axi_awsize, .m_axi_awburst,
        .m_axi_awvalid, .m_axi_awready, .m_axi_wdata, .m_axi_wstrb, .m_axi_wid, .m_axi_wlast,
        .m_axi_wvalid, .m_axi_wready, .m_axi_bid, .m_axi_bresp, .m_axi_bvalid, .m_axi_bready,
        .m_axi_arid, .m_axi_araddr, .m_axi_arlen, .m_axi_arsize, .m_axi_arburst, .m_axi_arvalid,
        .m_axi_arready, .m_axi_rid, .m_axi_rdata, .m_axi_rresp, .m_axi_rlast, .m_axi_rvalid,
        .m_axi_rready);
    axi_direct_memory #(.DATA_WIDTH(64), .WORDS(1192480), .BASE(32'h01000000)) memory (
        .clk, .reset, .sim_image_request(2'b0), .sim_image_done(), .sim_control(5'b0),
        .sim_fail_token(32'b0), .sim_fail_seen(), .sim_busy(), .sim_active(), .sim_error(), .sim_write(),
        .sim_index(), .sim_reads(), .sim_writes(), .sim_capacity(), .m_axi_awid, .m_axi_awaddr,
        .m_axi_awlen, .m_axi_awsize, .m_axi_awburst, .m_axi_awvalid, .m_axi_awready, .m_axi_wdata,
        .m_axi_wstrb, .m_axi_wid, .m_axi_wlast, .m_axi_wvalid, .m_axi_wready, .m_axi_bid, .m_axi_bresp,
        .m_axi_bvalid, .m_axi_bready, .m_axi_arid, .m_axi_araddr, .m_axi_arlen, .m_axi_arsize,
        .m_axi_arburst, .m_axi_arvalid, .m_axi_arready, .m_axi_rid, .m_axi_rdata, .m_axi_rresp,
        .m_axi_rlast, .m_axi_rvalid, .m_axi_rready);
endmodule

module marlin_bridge_external (
    input wire clk, input wire reset, output wire irq,
    input wire [11:0] s_axi_awaddr, input wire s_axi_awvalid, output wire s_axi_awready,
    input wire [31:0] s_axi_wdata, input wire s_axi_wvalid, output wire s_axi_wready,
    output wire s_axi_bvalid, input wire s_axi_bready,
    input wire [11:0] s_axi_araddr, input wire s_axi_arvalid, output wire s_axi_arready,
    output wire [31:0] s_axi_rdata, output wire s_axi_rvalid, input wire s_axi_rready
);
    marlin_bridge #(.EXTERNAL_IO(1)) top (.*);
endmodule
