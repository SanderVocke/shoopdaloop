module axi_direct_memory #(
    parameter integer DATA_WIDTH = 32,
    parameter integer WORDS = 8192,
    parameter [31:0] BASE = 32'h01000000
) (
    input wire clk, input wire reset,
    input wire [1:0] sim_image_request, output reg sim_image_done,
    input wire [4:0] sim_control,
    input wire [31:0] sim_fail_token, output reg [31:0] sim_fail_seen,
    output wire sim_busy, output wire sim_active, output wire sim_error, output wire sim_write,
    output wire [31:0] sim_index, output reg [63:0] sim_reads, output reg [63:0] sim_writes,
    output wire [31:0] sim_capacity,
    input wire m_axi_awid, input wire [31:0] m_axi_awaddr,
    input wire [3:0] m_axi_awlen, input wire [2:0] m_axi_awsize,
    input wire [1:0] m_axi_awburst, input wire m_axi_awvalid, output wire m_axi_awready,
    input wire [DATA_WIDTH-1:0] m_axi_wdata, input wire [DATA_WIDTH/8-1:0] m_axi_wstrb,
    input wire m_axi_wid, input wire m_axi_wlast, input wire m_axi_wvalid, output wire m_axi_wready,
    output wire m_axi_bid, output wire [1:0] m_axi_bresp,
    output wire m_axi_bvalid, input wire m_axi_bready,
    input wire m_axi_arid, input wire [31:0] m_axi_araddr,
    input wire [3:0] m_axi_arlen, input wire [2:0] m_axi_arsize,
    input wire [1:0] m_axi_arburst, input wire m_axi_arvalid, output wire m_axi_arready,
    output wire m_axi_rid, output wire [DATA_WIDTH-1:0] m_axi_rdata,
    output wire [1:0] m_axi_rresp, output wire m_axi_rlast,
    output wire m_axi_rvalid, input wire m_axi_rready
);
    localparam integer BYTES = DATA_WIDTH/8;
    reg [31:0] samples [0:WORDS-1];
    reg [DATA_WIDTH-1:0] write_data [0:15];
    reg [BYTES-1:0] write_strobes [0:15];
    reg write_lasts [0:15];
    reg writing, reading, written, response_write, response_read, error;
    reg [31:0] address;
    reg [7:0] length, read_index;
    reg [2:0] size;
    reg [4:0] write_count;
    reg [DATA_WIDTH-1:0] held_read;
    reg held_last;
    integer image_file;
    wire paused = sim_control[0];
    wire pause_response = sim_control[1];
    wire [7:0] next_read_index = read_index+8'(response_read);
    wire [31:0] beat_address = address+(32'(next_read_index) << size);
    assign sim_active = writing || reading;
    assign sim_busy = sim_active || write_count != 0;
    assign sim_error = error;
    assign sim_write = writing;
    assign sim_index = 32'(read_index);
    assign sim_capacity = 32'(WORDS);
    assign m_axi_awready = !reset && !paused && !sim_control[2] && !sim_active;
    assign m_axi_wready = !reset && !paused && !sim_control[3] && !reading && !written && write_count < 16;
    assign m_axi_arready = !reset && !paused && !sim_control[4] && !sim_active
        && write_count == 0 && !m_axi_awvalid && !m_axi_wvalid;
    assign m_axi_bid = 0;
    assign m_axi_bresp = error ? 2'd2 : 2'd0;
    assign m_axi_bvalid = response_write && !reset;
    assign m_axi_rid = 0;
    assign m_axi_rdata = held_read;
    assign m_axi_rresp = error ? 2'd2 : 2'd0;
    assign m_axi_rlast = held_last;
    assign m_axi_rvalid = response_read && !reset;

    function automatic outside(input [31:0] transfer_start, input [7:0] len, input [2:0] bytes_log);
        reg [63:0] transfer_end;
        begin
            transfer_end = {32'b0,transfer_start}+((64'(len)+1) << bytes_log);
            outside = transfer_start < BASE || transfer_end > {32'b0,BASE}+64'(WORDS)*4;
        end
    endfunction

    task automatic check_shape(input id, input [31:0] transfer_start, input [7:0] len,
                               input [2:0] bytes_log, input [1:0] burst);
        reg [63:0] transfer_end;
        begin
            transfer_end = {32'b0,transfer_start}+((64'(len)+1) << bytes_log)-1;
            if (id != 0 || burst != 1 || len > 15 || (32'd1 << bytes_log) > BYTES
                || (transfer_start & ((32'd1 << bytes_log)-1)) != 0
                || {32'b0,transfer_start >> 12} != (transfer_end >> 12))
                $fatal(1,"Unsupported direct-memory AXI transfer");
        end
    endtask

    function automatic [DATA_WIDTH-1:0] read_bus(input [31:0] transfer_start);
        reg signed [63:0] offset;
        begin
            read_bus = 0;
            for (integer lane=0; lane<BYTES; lane=lane+1) begin
                offset = ({32'b0,transfer_start} & ~(64'(BYTES)-64'd1))-{32'b0,BASE}+64'(lane);
                if (offset >= 0 && offset < 64'(WORDS)*4)
                    read_bus[lane*8+:8] = samples[32'(offset >> 2)][32'(offset & 3)*8+:8];
            end
        end
    endfunction

    initial begin
        if ((DATA_WIDTH != 32 && DATA_WIDTH != 64) || WORDS < 1) $fatal(1,"Invalid direct-memory dimensions");
        sim_reads = 0; sim_writes = 0; sim_image_done = 0;
        writing = 0; reading = 0; written = 0; response_write = 0; response_read = 0;
        error = 0; address = 0; length = 0; size = 0; read_index = 0; write_count = 0;
        held_read = 0; held_last = 0; sim_fail_seen = 0; image_file = 0;
    end

    // Image administration runs only between transfers; sample contents survive reset.
    /* verilator lint_off BLKSEQ */
    always @(posedge clk) begin
        if (sim_image_request == 0) sim_image_done <= 0;
        else if (!sim_image_done) begin
            if (sim_busy && !reset) $fatal(1,"Image operation during an AXI transfer");
            if (sim_image_request == 1) $readmemh("sim-memory.hex", samples);
            else if (sim_image_request == 2) begin
                image_file = $fopen("sim-memory.hex", "w");
                if (image_file == 0) $fatal(1,"Cannot export direct-memory image");
                for (integer word_index=0; word_index<WORDS; word_index=word_index+1)
                    $fdisplay(image_file,"%08x",samples[word_index]);
                $fclose(image_file);
            end else $fatal(1,"Invalid image operation");
            sim_image_done <= 1;
        end
        if (reset) begin
            writing <= 0; reading <= 0; written <= 0; write_count <= 0;
            response_write <= 0; response_read <= 0; error <= 0;
            read_index <= 0; sim_fail_seen <= sim_fail_token;
        end else begin
            if (m_axi_awvalid && m_axi_awready) begin
                check_shape(m_axi_awid,m_axi_awaddr,8'(m_axi_awlen),m_axi_awsize,m_axi_awburst);
                writing <= 1; address <= m_axi_awaddr; length <= 8'(m_axi_awlen); size <= m_axi_awsize;
                error <= outside(m_axi_awaddr,8'(m_axi_awlen),m_axi_awsize) || sim_fail_token != sim_fail_seen;
                sim_fail_seen <= sim_fail_token; sim_writes <= sim_writes+1;
            end
            if (m_axi_arvalid && m_axi_arready) begin
                check_shape(m_axi_arid,m_axi_araddr,8'(m_axi_arlen),m_axi_arsize,m_axi_arburst);
                reading <= 1; address <= m_axi_araddr; length <= 8'(m_axi_arlen); size <= m_axi_arsize;
                error <= outside(m_axi_araddr,8'(m_axi_arlen),m_axi_arsize) || sim_fail_token != sim_fail_seen;
                sim_fail_seen <= sim_fail_token; sim_reads <= sim_reads+1; read_index <= 0;
            end
            if (m_axi_wvalid && m_axi_wready) begin
                if (m_axi_wid != 0) $fatal(1,"Unexpected AXI3 WID");
                write_data[write_count[3:0]] <= m_axi_wdata;
                write_strobes[write_count[3:0]] <= m_axi_wstrb;
                write_lasts[write_count[3:0]] <= m_axi_wlast;
                write_count <= write_count+1;
            end
            if (writing && !written && write_count == {1'b0,length[3:0]}+1) begin
                for (integer beat=0; beat<16; beat=beat+1) if (beat <= 32'(length)) begin
                    if (write_lasts[beat] != (beat == 32'(length))) $fatal(1,"Invalid WLAST");
                    for (integer lane=0; lane<BYTES; lane=lane+1) if (write_strobes[beat][lane]) begin
                        if (lane < ((address+(32'(beat) << size)) & (32'(BYTES)-32'd1))
                            || lane >= (((address+(32'(beat) << size)) & (32'(BYTES)-32'd1))+(32'd1 << size)))
                            $fatal(1,"WSTRB outside beat lanes");
                        if (!error) begin
                            samples[((((address+(32'(beat) << size)) & ~(32'(BYTES)-32'd1))-BASE) >> 2)+32'(lane/4)]
                                [lane*8 % 32+:8] = write_data[beat][lane*8+:8];
                        end
                    end
                end
                written <= 1;
            end
            if (writing && written && !response_write && !paused && !pause_response) response_write <= 1;
            if (m_axi_bvalid && m_axi_bready) begin
                response_write <= 0; writing <= 0; written <= 0; write_count <= 0;
            end
            if (reading && (!response_read || m_axi_rready) && !paused && !pause_response) begin
                held_read <= error ? {DATA_WIDTH{1'b0}} : read_bus(beat_address);
                held_last <= next_read_index == length;
                response_read <= 1;
            end else if (m_axi_rvalid && m_axi_rready) response_read <= 0;
            if (m_axi_rvalid && m_axi_rready) begin
                if (held_last) begin reading <= 0; response_read <= 0; end
                else read_index <= read_index+1;
            end
        end
    end
    /* verilator lint_on BLKSEQ */
endmodule
