module shared_axi #(parameter integer DATA_WIDTH = 32, parameter integer BURST_MEMORY = 1) (
    input wire clk, input wire reset,
    input wire [63:0] arena_base, input wire [63:0] arena_bytes, input wire [63:0] sample_bytes,
    input wire [31:0] physical_base,
    input wire memory_valid, output wire memory_ready, input wire memory_write,
    input wire [63:0] memory_address, input wire [31:0] memory_write_data,
    input wire [31:0] memory_count,
    input wire memory_write_valid, output wire memory_write_ready,
    output wire memory_read_valid, input wire memory_read_ready,
    output wire memory_response_valid, input wire memory_response_ready,
    output wire memory_response_error, output wire [31:0] memory_read_data,
    input wire memory_meta_valid, output wire memory_meta_ready, input wire memory_meta_write,
    input wire [63:0] memory_meta_address, input wire [31:0] memory_meta_write_data,
    input wire [31:0] memory_meta_count,
    input wire memory_meta_write_valid, output wire memory_meta_write_ready,
    output wire memory_meta_read_valid, input wire memory_meta_read_ready,
    output wire memory_meta_response_valid, input wire memory_meta_response_ready,
    output wire memory_meta_response_error, output wire [31:0] memory_meta_read_data,
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
    wire [1:0] client_command_valid;
    wire [1:0] client_command_ready;
    wire [1:0] client_command_write;
    wire [2*(32)-1:0] client_command_address;
    wire [2*(8)-1:0] client_command_length;
    wire [2*(3)-1:0] client_command_size;
    wire [1:0] client_write_valid;
    wire [1:0] client_write_ready;
    wire [2*(DATA_WIDTH)-1:0] client_write_data;
    wire [2*(DATA_WIDTH/8)-1:0] client_write_strobe;
    wire [1:0] client_read_valid;
    wire [1:0] client_read_ready;
    wire [2*(DATA_WIDTH)-1:0] client_read_data;
    wire [1:0] client_read_error;
    wire [1:0] client_read_last;
    wire [1:0] client_completion_valid;
    wire [1:0] client_completion_ready;
    wire [1:0] client_completion_error;
    generate if (BURST_MEMORY != 0) begin : stream_0
        chunk_dma #(.DATA_WIDTH(DATA_WIDTH)) transfer (
            .clk(clk),
            .reset(reset),
            .arena_base(arena_base),
            .arena_bytes(sample_bytes),
            .physical_base(physical_base),
            .memory_valid(memory_valid),
            .memory_ready(memory_ready),
            .memory_write(memory_write),
            .memory_address(memory_address),
            .memory_write_data(memory_write_data),
            .memory_count(memory_count),
            .memory_write_valid(memory_write_valid),
            .memory_write_ready(memory_write_ready),
            .memory_read_valid(memory_read_valid),
            .memory_read_ready(memory_read_ready),
            .memory_response_valid(memory_response_valid),
            .memory_response_ready(memory_response_ready),
            .memory_response_error(memory_response_error),
            .memory_read_data(memory_read_data),
            .command_valid(client_command_valid[0]),
            .command_ready(client_command_ready[0]),
            .command_write(client_command_write[0]),
            .command_address(client_command_address[0*(32)+:(32)]),
            .command_length(client_command_length[0*(8)+:(8)]),
            .command_size(client_command_size[0*(3)+:(3)]),
            .write_valid(client_write_valid[0]),
            .write_ready(client_write_ready[0]),
            .write_data(client_write_data[0*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .write_strobe(client_write_strobe[0*(DATA_WIDTH/8)+:(DATA_WIDTH/8)]),
            .read_valid(client_read_valid[0]),
            .read_ready(client_read_ready[0]),
            .read_data(client_read_data[0*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .read_error(client_read_error[0]),
            .read_last(client_read_last[0]),
            .completion_valid(client_completion_valid[0]),
            .completion_ready(client_completion_ready[0]),
            .completion_error(client_completion_error[0]));
    end else begin : scalar_0
        wire unused_stream = &{1'b0,memory_count,memory_write_valid,memory_read_ready};
        assign memory_write_ready = 0;
        assign memory_read_valid = 0;
        word_dma #(.DATA_WIDTH(DATA_WIDTH)) transfer (
            .clk(clk),
            .reset(reset),
            .arena_base(arena_base),
            .arena_bytes(sample_bytes),
            .physical_base(physical_base),
            .memory_valid(memory_valid),
            .memory_ready(memory_ready),
            .memory_write(memory_write),
            .memory_address(memory_address),
            .memory_write_data(memory_write_data),
            .memory_response_valid(memory_response_valid),
            .memory_response_ready(memory_response_ready),
            .memory_response_error(memory_response_error),
            .memory_read_data(memory_read_data),
            .command_valid(client_command_valid[0]),
            .command_ready(client_command_ready[0]),
            .command_write(client_command_write[0]),
            .command_address(client_command_address[0*(32)+:(32)]),
            .command_length(client_command_length[0*(8)+:(8)]),
            .command_size(client_command_size[0*(3)+:(3)]),
            .write_valid(client_write_valid[0]),
            .write_ready(client_write_ready[0]),
            .write_data(client_write_data[0*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .write_strobe(client_write_strobe[0*(DATA_WIDTH/8)+:(DATA_WIDTH/8)]),
            .read_valid(client_read_valid[0]),
            .read_ready(client_read_ready[0]),
            .read_data(client_read_data[0*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .read_error(client_read_error[0]),
            .read_last(client_read_last[0]),
            .completion_valid(client_completion_valid[0]),
            .completion_ready(client_completion_ready[0]),
            .completion_error(client_completion_error[0]));
    end endgenerate
    generate if (BURST_MEMORY != 0) begin : stream_1
        chunk_dma #(.DATA_WIDTH(DATA_WIDTH)) transfer (
            .clk(clk),
            .reset(reset),
            .arena_base(arena_base),
            .arena_bytes(arena_bytes),
            .physical_base(physical_base),
            .memory_valid(memory_meta_valid),
            .memory_ready(memory_meta_ready),
            .memory_write(memory_meta_write),
            .memory_address(memory_meta_address),
            .memory_write_data(memory_meta_write_data),
            .memory_count(memory_meta_count),
            .memory_write_valid(memory_meta_write_valid),
            .memory_write_ready(memory_meta_write_ready),
            .memory_read_valid(memory_meta_read_valid),
            .memory_read_ready(memory_meta_read_ready),
            .memory_response_valid(memory_meta_response_valid),
            .memory_response_ready(memory_meta_response_ready),
            .memory_response_error(memory_meta_response_error),
            .memory_read_data(memory_meta_read_data),
            .command_valid(client_command_valid[1]),
            .command_ready(client_command_ready[1]),
            .command_write(client_command_write[1]),
            .command_address(client_command_address[1*(32)+:(32)]),
            .command_length(client_command_length[1*(8)+:(8)]),
            .command_size(client_command_size[1*(3)+:(3)]),
            .write_valid(client_write_valid[1]),
            .write_ready(client_write_ready[1]),
            .write_data(client_write_data[1*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .write_strobe(client_write_strobe[1*(DATA_WIDTH/8)+:(DATA_WIDTH/8)]),
            .read_valid(client_read_valid[1]),
            .read_ready(client_read_ready[1]),
            .read_data(client_read_data[1*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .read_error(client_read_error[1]),
            .read_last(client_read_last[1]),
            .completion_valid(client_completion_valid[1]),
            .completion_ready(client_completion_ready[1]),
            .completion_error(client_completion_error[1]));
    end else begin : scalar_1
        wire unused_stream = &{1'b0,memory_meta_count,memory_meta_write_valid,memory_meta_read_ready};
        assign memory_meta_write_ready = 0;
        assign memory_meta_read_valid = 0;
        word_dma #(.DATA_WIDTH(DATA_WIDTH)) transfer (
            .clk(clk),
            .reset(reset),
            .arena_base(arena_base),
            .arena_bytes(arena_bytes),
            .physical_base(physical_base),
            .memory_valid(memory_meta_valid),
            .memory_ready(memory_meta_ready),
            .memory_write(memory_meta_write),
            .memory_address(memory_meta_address),
            .memory_write_data(memory_meta_write_data),
            .memory_response_valid(memory_meta_response_valid),
            .memory_response_ready(memory_meta_response_ready),
            .memory_response_error(memory_meta_response_error),
            .memory_read_data(memory_meta_read_data),
            .command_valid(client_command_valid[1]),
            .command_ready(client_command_ready[1]),
            .command_write(client_command_write[1]),
            .command_address(client_command_address[1*(32)+:(32)]),
            .command_length(client_command_length[1*(8)+:(8)]),
            .command_size(client_command_size[1*(3)+:(3)]),
            .write_valid(client_write_valid[1]),
            .write_ready(client_write_ready[1]),
            .write_data(client_write_data[1*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .write_strobe(client_write_strobe[1*(DATA_WIDTH/8)+:(DATA_WIDTH/8)]),
            .read_valid(client_read_valid[1]),
            .read_ready(client_read_ready[1]),
            .read_data(client_read_data[1*(DATA_WIDTH)+:(DATA_WIDTH)]),
            .read_error(client_read_error[1]),
            .read_last(client_read_last[1]),
            .completion_valid(client_completion_valid[1]),
            .completion_ready(client_completion_ready[1]),
            .completion_error(client_completion_error[1]));
    end endgenerate
    axi_burst_mux #(.DATA_WIDTH(DATA_WIDTH)) scheduler (.*);
endmodule
