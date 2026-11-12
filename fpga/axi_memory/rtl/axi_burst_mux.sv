module axi_burst_mux #(parameter integer DATA_WIDTH = 32) (
    input wire clk, input wire reset,
    input wire [1:0] client_command_valid,
    output wire [1:0] client_command_ready,
    input wire [1:0] client_command_write,
    input wire [2*(32)-1:0] client_command_address,
    input wire [2*(8)-1:0] client_command_length,
    input wire [2*(3)-1:0] client_command_size,
    input wire [1:0] client_write_valid,
    output wire [1:0] client_write_ready,
    input wire [2*(DATA_WIDTH)-1:0] client_write_data,
    input wire [2*(DATA_WIDTH/8)-1:0] client_write_strobe,
    output wire [1:0] client_read_valid,
    input wire [1:0] client_read_ready,
    output wire [2*(DATA_WIDTH)-1:0] client_read_data,
    output wire [1:0] client_read_error,
    output wire [1:0] client_read_last,
    output wire [1:0] client_completion_valid,
    input wire [1:0] client_completion_ready,
    output wire [1:0] client_completion_error,
    output wire m_axi_awid, output wire [31:0] m_axi_awaddr,
    output wire [7:0] m_axi_awlen, output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst, output wire m_axi_awvalid, input wire m_axi_awready,
    output wire [DATA_WIDTH-1:0] m_axi_wdata, output wire [DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wlast, output wire m_axi_wvalid, input wire m_axi_wready,
    input wire m_axi_bid, input wire [1:0] m_axi_bresp,
    input wire m_axi_bvalid, output wire m_axi_bready,
    output wire m_axi_arid, output wire [31:0] m_axi_araddr,
    output wire [7:0] m_axi_arlen, output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst, output wire m_axi_arvalid, input wire m_axi_arready,
    input wire m_axi_rid, input wire [DATA_WIDTH-1:0] m_axi_rdata,
    input wire [1:0] m_axi_rresp, input wire m_axi_rlast,
    input wire m_axi_rvalid, output wire m_axi_rready

);
    reg active, owner, turn;
    wire selected = client_command_valid[turn] ? turn : !turn;
    wire command_valid;
    assign command_valid = !active && client_command_valid[selected];
    wire command_ready;
    assign client_command_ready = !active ? (2'(command_ready) << selected) : 0;
    wire command_write;
    assign command_write = client_command_write[selected];
    wire [32-1:0] command_address;
    assign command_address = client_command_address[selected*(32)+:(32)];
    wire [8-1:0] command_length;
    assign command_length = client_command_length[selected*(8)+:(8)];
    wire [3-1:0] command_size;
    assign command_size = client_command_size[selected*(3)+:(3)];
    wire write_valid;
    assign write_valid = client_write_valid[owner];
    wire write_ready;
    assign client_write_ready = active ? (2'(write_ready) << owner) : 0;
    wire [DATA_WIDTH-1:0] write_data;
    assign write_data = client_write_data[owner*(DATA_WIDTH)+:(DATA_WIDTH)];
    wire [DATA_WIDTH/8-1:0] write_strobe;
    assign write_strobe = client_write_strobe[owner*(DATA_WIDTH/8)+:(DATA_WIDTH/8)];
    wire read_valid;
    assign client_read_valid = active ? (2'(read_valid) << owner) : 0;
    wire read_ready;
    assign read_ready = client_read_ready[owner];
    wire [DATA_WIDTH-1:0] read_data;
    assign client_read_data = {read_data,read_data};
    wire read_error;
    assign client_read_error = {read_error,read_error};
    wire read_last;
    assign client_read_last = {read_last,read_last};
    wire completion_valid;
    assign client_completion_valid = active ? (2'(completion_valid) << owner) : 0;
    wire completion_ready;
    assign completion_ready = client_completion_ready[owner];
    wire completion_error;
    assign client_completion_error = {completion_error,completion_error};
    reg [63:0] reads [0:1];
    reg [63:0] writes [0:1];
    reg [63:0] bytes [0:1];
    reg [63:0] wait_cycles [0:1];
    axi_master #(.DATA_WIDTH(DATA_WIDTH)) endpoint (.*);
    always @(posedge clk) begin
        if (reset) begin
            active <= 0; owner <= 0; turn <= 0;
            for (integer i=0; i<2; i=i+1) begin
                reads[i] <= 0; writes[i] <= 0; bytes[i] <= 0; wait_cycles[i] <= 0;
            end
        end else begin
            for (integer i=0; i<2; i=i+1)
                if (client_command_valid[i] && !client_command_ready[i]) wait_cycles[i] <= wait_cycles[i]+1;
            if (command_valid && command_ready) begin
                active <= 1; owner <= selected;
                if (command_write) writes[selected] <= writes[selected]+1;
                else reads[selected] <= reads[selected]+1;
                bytes[selected] <= bytes[selected]+((64'(command_length)+1) << command_size);
            end
            if (active && ((read_valid && read_ready && read_last) || (completion_valid && completion_ready))) begin
                active <= 0; turn <= !owner;
            end
        end
    end
endmodule
