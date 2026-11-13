module axi_master #(
    parameter integer DATA_WIDTH = 32
) (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready,
    input wire command_write, input wire [31:0] command_address,
    input wire [7:0] command_length, input wire [2:0] command_size,
    input wire write_valid, output wire write_ready,
    input wire [DATA_WIDTH-1:0] write_data, input wire [DATA_WIDTH/8-1:0] write_strobe,
    output wire read_valid, input wire read_ready,
    output wire [DATA_WIDTH-1:0] read_data, output wire read_error, output wire read_last,
    output wire completion_valid, input wire completion_ready, output wire completion_error,
    output wire m_axi_awid, output wire [31:0] m_axi_awaddr,
    output wire [3:0] m_axi_awlen, output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst, output wire m_axi_awvalid, input wire m_axi_awready,
    output wire [DATA_WIDTH-1:0] m_axi_wdata, output wire [DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wid, output wire m_axi_wlast, output wire m_axi_wvalid, input wire m_axi_wready,
    input wire m_axi_bid, input wire [1:0] m_axi_bresp,
    input wire m_axi_bvalid, output wire m_axi_bready,
    output wire m_axi_arid, output wire [31:0] m_axi_araddr,
    output wire [3:0] m_axi_arlen, output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst, output wire m_axi_arvalid, input wire m_axi_arready,
    input wire m_axi_rid, input wire [DATA_WIDTH-1:0] m_axi_rdata,
    input wire [1:0] m_axi_rresp, input wire m_axi_rlast,
    input wire m_axi_rvalid, output wire m_axi_rready
);
    localparam integer BUS_BITS = $clog2(DATA_WIDTH/8);
    localparam [2:0] IDLE = 0, WRITING = 1, READING = 2, BAD_WRITE = 3, BAD_READ = 4, CHECK = 5;
    reg [2:0] phase;
    reg [31:0] address;
    reg [7:0] length, index;
    reg [2:0] size;
    reg address_sent, data_sent, writing;
    wire [12:0] page_end = {1'b0,address[11:0]} + (({5'b0,length}+13'd1) << size)-13'd1;
    wire unused_end = &{1'b0,page_end[11:0]};
    wire invalid_command = length > 15 || size > 3'(BUS_BITS)
        || (8'(address[2:0]) & ((8'd1 << size)-8'd1)) != 8'b0 || page_end[12];

    assign command_ready = phase == IDLE && !reset;
    assign m_axi_awid = 0;
    assign m_axi_awaddr = address;
    assign m_axi_awlen = length[3:0];
    assign m_axi_awsize = size;
    assign m_axi_awburst = 1;
    assign m_axi_awvalid = phase == WRITING && !address_sent && !reset;
    assign m_axi_wid = 0;
    assign m_axi_wdata = write_data;
    assign m_axi_wstrb = write_strobe;
    assign m_axi_wlast = index == length;
    assign m_axi_wvalid = phase == WRITING && !data_sent && write_valid && !reset;
    assign write_ready = phase == WRITING && !data_sent && m_axi_wready && !reset;
    assign completion_valid = !reset && (phase == BAD_WRITE
        || (phase == WRITING && address_sent && data_sent && m_axi_bvalid));
    assign completion_error = phase == BAD_WRITE || m_axi_bresp != 0 || m_axi_bid != 0;
    assign m_axi_bready = phase == WRITING && address_sent && data_sent && completion_ready && !reset;
    assign m_axi_arid = 0;
    assign m_axi_araddr = address;
    assign m_axi_arlen = length[3:0];
    assign m_axi_arsize = size;
    assign m_axi_arburst = 1;
    assign m_axi_arvalid = phase == READING && !address_sent && !reset;
    assign read_valid = !reset && (phase == BAD_READ || (phase == READING && address_sent && m_axi_rvalid));
    assign read_data = phase == BAD_READ ? {DATA_WIDTH{1'b0}} : m_axi_rdata;
    assign read_error = phase == BAD_READ || m_axi_rresp != 0 || m_axi_rid != 0
        || (m_axi_rlast != (index == length));
    assign read_last = phase == BAD_READ || m_axi_rlast;
    assign m_axi_rready = phase == READING && address_sent && read_ready && !reset;

    initial begin
        if (DATA_WIDTH != 32 && DATA_WIDTH != 64) $fatal(1,"AXI data width must be 32 or 64");
    end

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; address <= 0; length <= 0; size <= 0; index <= 0;
            address_sent <= 0; data_sent <= 0; writing <= 0;
        end else begin
            if (command_valid && command_ready) begin
                address <= command_address; length <= command_length; size <= command_size;
                index <= 0; address_sent <= 0; data_sent <= 0;
                writing <= command_write; phase <= CHECK;
            end
            if (phase == CHECK) begin
                if (invalid_command) phase <= writing ? BAD_WRITE : BAD_READ;
                else phase <= writing ? WRITING : READING;
            end
            if (m_axi_awvalid && m_axi_awready) address_sent <= 1;
            if (m_axi_arvalid && m_axi_arready) address_sent <= 1;
            if (m_axi_wvalid && m_axi_wready) begin
                if (index == length) data_sent <= 1;
                else index <= index+1;
            end
            if (read_valid && read_ready) begin
                if (read_last) phase <= IDLE;
                else index <= index+1;
            end
            if (completion_valid && completion_ready) phase <= IDLE;
        end
    end
endmodule
