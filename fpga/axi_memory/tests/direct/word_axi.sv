// Generated functional simulation top; do not edit by hand.
module word_axi #(
parameter integer DATA_WIDTH = 32,
    parameter integer SIM_WORDS = 8192,
    parameter [31:0] SIM_PHYSICAL_BASE = 32'h01000000
) (
    input wire clk,
    input wire reset,
    input wire [63:0] arena_base,
    input wire [63:0] arena_bytes,
    input wire [31:0] physical_base,
    input wire memory_valid,
    output wire memory_ready,
    input wire memory_write,
    input wire [63:0] memory_address,
    input wire [31:0] memory_write_data,
    output wire memory_response_valid,
    input wire memory_response_ready,
    output reg memory_response_error,
    output reg [31:0] memory_read_data,
    output wire m_axi_awid,
    output wire [31:0] m_axi_awaddr,
    output wire [7:0] m_axi_awlen,
    output wire [2:0] m_axi_awsize,
    output wire [1:0] m_axi_awburst,
    output wire m_axi_awvalid,
    output wire m_axi_awready,
    output wire [DATA_WIDTH-1:0] m_axi_wdata,
    output wire [DATA_WIDTH/8-1:0] m_axi_wstrb,
    output wire m_axi_wlast,
    output wire m_axi_wvalid,
    output wire m_axi_wready,
    output wire m_axi_bid,
    output wire [1:0] m_axi_bresp,
    output wire m_axi_bvalid,
    output wire m_axi_bready,
    output wire m_axi_arid,
    output wire [31:0] m_axi_araddr,
    output wire [7:0] m_axi_arlen,
    output wire [2:0] m_axi_arsize,
    output wire [1:0] m_axi_arburst,
    output wire m_axi_arvalid,
    output wire m_axi_arready,
    output wire m_axi_rid,
    output wire [DATA_WIDTH-1:0] m_axi_rdata,
    output wire [1:0] m_axi_rresp,
    output wire m_axi_rlast,
    output wire m_axi_rvalid,
    output wire m_axi_rready,
    input wire [1:0] sim_image_request,
    output wire sim_image_done,
    input wire [4:0] sim_control,
    input wire [31:0] sim_fail_token,
    output wire [31:0] sim_fail_seen,
    output wire sim_busy, output wire sim_active, output wire sim_error, output wire sim_write,
    output wire [31:0] sim_index,
    output wire [63:0] sim_reads, output wire [63:0] sim_writes,
    output wire [31:0] sim_capacity
);
    localparam integer LANE_BITS = $clog2(DATA_WIDTH/8);
    localparam [2:0] IDLE = 0, COMMAND = 1, WRITE = 2, READ = 3, ACK = 4, RESPONSE = 5;
    reg [2:0] phase;
    reg command_write;
    reg [31:0] command_address, payload;
    reg [LANE_BITS-1:0] lane;
    wire [63:0] offset = memory_address-arena_base;
    wire [64:0] mapped = {33'b0,physical_base}+{1'b0,offset};
    wire bad_address = memory_address < arena_base || arena_bytes < 4 || offset > arena_bytes-4
        || mapped[64:32] != 0 || mapped[31:0] > 32'hfffffffc || memory_address[1:0] != 0
        || mapped[1:0] != 0;
    wire command_valid = phase == COMMAND;
    wire command_ready;
    wire [7:0] command_length = 0;
    wire [2:0] command_size = 2;
    wire write_valid = phase == WRITE;
    wire write_ready;
    wire [DATA_WIDTH-1:0] write_data = DATA_WIDTH'(payload) << (lane*8);
    wire [DATA_WIDTH/8-1:0] write_strobe = (DATA_WIDTH/8)'(4'b1111) << lane;
    wire read_valid, read_error, read_last;
    wire read_ready = phase == READ;
    wire [DATA_WIDTH-1:0] read_data;
    wire completion_valid, completion_error;
    wire completion_ready = phase == ACK;
    wire unused_last = &{1'b0,read_last};
    assign memory_ready = phase == IDLE && !reset;
    assign memory_response_valid = phase == RESPONSE && !reset;

    axi_master #(.DATA_WIDTH(DATA_WIDTH)) endpoint (.*);

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE; command_write <= 0; command_address <= 0; payload <= 0; lane <= 0;
            memory_response_error <= 0; memory_read_data <= 0;
        end else begin
            if (memory_valid && memory_ready) begin
                command_write <= memory_write; command_address <= mapped[31:0];
                payload <= memory_write_data; lane <= mapped[LANE_BITS-1:0];
                memory_response_error <= bad_address; memory_read_data <= 0;
                phase <= bad_address ? RESPONSE : COMMAND;
            end
            if (command_valid && command_ready) phase <= command_write ? WRITE : READ;
            if (write_valid && write_ready) phase <= ACK;
            if (read_valid && read_ready) begin
                memory_read_data <= 32'(read_data >> (lane*8)); memory_response_error <= read_error;
                phase <= RESPONSE;
            end
            if (completion_valid && completion_ready) begin
                memory_response_error <= completion_error; phase <= RESPONSE;
            end
            if (memory_response_valid && memory_response_ready) phase <= IDLE;
        end
    end
    axi_direct_memory #(.DATA_WIDTH(DATA_WIDTH),.WORDS(SIM_WORDS),.BASE(SIM_PHYSICAL_BASE)) sim_memory (.*);
endmodule
