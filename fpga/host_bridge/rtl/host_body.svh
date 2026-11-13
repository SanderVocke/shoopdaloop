    // CPU host bridge: AXI-lite register windows over the core's valid/ready ports.
    // Each window runs one transaction at a time; host traffic belongs between quanta.
    localparam [1:0] IDLE = 2'd0, REQUEST = 2'd1, RESPONSE = 2'd2;
    localparam [1:0] RESET_IDLE = 2'd0, RESET_DRAIN = 2'd1, RESET_HOLD = 2'd2;
    localparam integer FIFO_DEPTH = 64;
    localparam integer CAPACITY_BITS = $clog2(HOST_CAPACITY_WORDS), RUN_BITS = $clog2(HOST_RUN_WORDS);
    localparam integer TP_DATA_BITS = $clog2(HOST_TP_DATA_WORDS), TP_RESPONSE_BITS = $clog2(HOST_TP_RESPONSE_WORDS);
    localparam integer CFG_BITS = $clog2(HOST_CFG_WRITE_WORDS), STORE_CFG_BITS = $clog2(HOST_STORE_CFG_WRITE_WORDS);

    function automatic in_window(input [11:0] address, input [11:0] base, input integer count);
        in_window = address >= base && {20'b0, address} < {20'b0, base} + 32'(4*count);
    endfunction
    function automatic [11:0] word_index(input [11:0] address, input [11:0] base);
        word_index = (address - base) >> 2;
    endfunction

    // Requests are captured first and take effect a cycle later, so the CPU-side
    // AXI wires only reach capture registers.
    reg bvalid, rvalid, write_fire, read_fire;
    reg [31:0] rdata, write_data;
    reg [11:0] write_address, read_address;
    wire write_accept = s_axi_awvalid && s_axi_wvalid && !write_fire && !bvalid;
    wire read_accept = s_axi_arvalid && !read_fire && !rvalid;
    assign s_axi_awready = write_accept;
    assign s_axi_wready = write_accept;
    assign s_axi_bvalid = bvalid;
    assign s_axi_bresp = 2'b00;
    assign s_axi_arready = read_accept;
    assign s_axi_rvalid = rvalid;
    assign s_axi_rresp = 2'b00;
    assign s_axi_rdata = rdata;

    reg [1:0] reset_phase;
    reg [1:0] reset_count;
    reg [1:0] run_phase, tp_phase, cfg_phase, audio_phase, store_cfg_phase, store_phase;
    reg done_flag;
    reg [63:0] quanta;
    reg [31:0] run_cycles, last_cycles, output_words;
    reg [63:0] digest;
    reg [31:0] run_words [0:HOST_RUN_WORDS-1];
    reg [7:0] tp_opcode;
    reg [31:0] tp_index;
    reg [31:0] tp_data [0:HOST_TP_DATA_WORDS-1];
    reg [31:0] tp_response [0:HOST_TP_RESPONSE_WORDS-1];
    reg [3:0] tp_error;
    reg [1:0] cfg_control;
    reg [15:0] cfg_address_word;
    reg [31:0] cfg_write_words [0:HOST_CFG_WRITE_WORDS-1];
    reg [31:0] cfg_read_words [0:HOST_CFG_READ_WORDS-1];
    reg cfg_error;
    reg [2:0] audio_control;
    reg [31:0] audio_address, audio_write, audio_read;
    reg audio_error;
    reg [3:0] store_cfg_control;
    reg [31:0] store_cfg_address_word;
    reg [31:0] store_cfg_write_words [0:HOST_STORE_CFG_WRITE_WORDS-1];
    reg [31:0] store_cfg_read_words [0:HOST_STORE_CFG_READ_WORDS-1];
    reg store_cfg_error;
    reg [3:0] store_opcode;
    reg [31:0] store_view, store_count, store_result_view_word, store_written;
    reg [63:0] store_start, store_length;
    reg [7:0] store_code;
    reg store_empty;
    reg [31:0] tx_fifo [0:FIFO_DEPTH-1];
    reg [31:0] rx_fifo [0:FIFO_DEPTH-1];
    reg [5:0] tx_head, tx_tail, rx_head, rx_tail;
    reg [6:0] tx_count, rx_count;

    reg read_outstanding, write_outstanding, ar_exposed, aw_exposed;
    wire isolate = reset_phase == RESET_DRAIN;
    wire memory_quiescent = !read_outstanding && !write_outstanding && !ar_exposed && !aw_exposed;
    assign m_axi_arvalid = core_m_axi_arvalid && (!isolate || ar_exposed);
    assign core_m_axi_arready = m_axi_arready && (!isolate || ar_exposed);
    assign m_axi_awvalid = core_m_axi_awvalid && (!isolate || aw_exposed);
    assign core_m_axi_awready = m_axi_awready && (!isolate || aw_exposed);
    assign m_axi_wvalid = core_m_axi_wvalid && write_outstanding;
    assign core_m_axi_wready = m_axi_wready && write_outstanding;
    assign m_axi_awid = core_m_axi_awid;
    assign m_axi_awaddr = core_m_axi_awaddr;
    assign m_axi_awlen = core_m_axi_awlen;
    assign m_axi_awsize = core_m_axi_awsize;
    assign m_axi_awburst = core_m_axi_awburst;
    assign m_axi_wid = core_m_axi_wid;
    assign m_axi_wdata = core_m_axi_wdata;
    assign m_axi_wstrb = core_m_axi_wstrb;
    assign m_axi_wlast = core_m_axi_wlast;
    assign core_m_axi_bid = m_axi_bid;
    assign core_m_axi_bresp = m_axi_bresp;
    assign core_m_axi_bvalid = m_axi_bvalid;
    assign m_axi_bready = core_m_axi_bready;
    assign m_axi_arid = core_m_axi_arid;
    assign m_axi_araddr = core_m_axi_araddr;
    assign m_axi_arlen = core_m_axi_arlen;
    assign m_axi_arsize = core_m_axi_arsize;
    assign m_axi_arburst = core_m_axi_arburst;
    assign core_m_axi_rid = m_axi_rid;
    assign core_m_axi_rdata = m_axi_rdata;
    assign core_m_axi_rresp = m_axi_rresp;
    assign core_m_axi_rlast = m_axi_rlast;
    assign core_m_axi_rvalid = m_axi_rvalid;
    assign m_axi_rready = core_m_axi_rready;

    always @(posedge clk) begin
        if (reset) begin
            read_outstanding <= 0; write_outstanding <= 0; ar_exposed <= 0; aw_exposed <= 0;
        end else begin
            if (m_axi_arvalid) ar_exposed <= !m_axi_arready;
            if (m_axi_awvalid) aw_exposed <= !m_axi_awready;
            if (m_axi_arvalid && m_axi_arready) read_outstanding <= 1;
            if (m_axi_rvalid && m_axi_rready && m_axi_rlast) read_outstanding <= 0;
            if (m_axi_awvalid && m_axi_awready) write_outstanding <= 1;
            if (m_axi_bvalid && m_axi_bready) write_outstanding <= 0;
        end
    end

    wire [32*HOST_TP_DATA_WORDS-1:0] tp_data_packed;
    wire [32*HOST_CFG_WRITE_WORDS-1:0] cfg_write_packed;
    wire [32*HOST_STORE_CFG_WRITE_WORDS-1:0] store_cfg_write_packed;
    wire [32*HOST_CFG_READ_WORDS-1:0] cfg_read_padded =
        {{(32*HOST_CFG_READ_WORDS-`SHOOP_SP_STATE_BITS){1'b0}}, cfg_read_data};
    genvar host_word;
    generate
        for (host_word = 0; host_word < HOST_TP_DATA_WORDS; host_word = host_word + 1) begin : host_tp_pack
            assign tp_data_packed[32*host_word +: 32] = tp_data[host_word];
        end
        for (host_word = 0; host_word < HOST_CFG_WRITE_WORDS; host_word = host_word + 1) begin : host_cfg_pack
            assign cfg_write_packed[32*host_word +: 32] = cfg_write_words[host_word];
        end
        for (host_word = 0; host_word < HOST_STORE_CFG_WRITE_WORDS; host_word = host_word + 1) begin : host_store_cfg_pack
            assign store_cfg_write_packed[32*host_word +: 32] = store_cfg_write_words[host_word];
        end
    endgenerate

    // One flop drives the core's reset so the tools can replicate it across the die.
    (* max_fanout = 128 *) reg core_reset_hold;
    always @(posedge clk) core_reset_hold <= reset || reset_phase == RESET_HOLD;
    assign core_reset = core_reset_hold;
    assign maintenance_enable = 1'b1;
    assign job_enable = 1'b1;
    assign drain_ready = 1'b0;
    assign arena_base = 64'(PHYSICAL_BASE);
    assign irq = done_flag;
    assign run_valid = run_phase == REQUEST;
    assign run_instruction_count = run_words[0];
    assign run_state_count = run_words[1];
    assign run_quantum = run_words[2];
    assign run_chunk_frames = run_words[3];
    assign run_job_budget = run_words[4];
    assign run_buffers = run_words[5];
    assign run_inputs = run_words[6];
    assign run_outputs = run_words[7];
    assign command_valid = tp_phase == REQUEST;
    assign command_opcode = tp_opcode;
    assign command_index = tp_index;
    assign command_data = tp_data_packed;
    assign response_ready = tp_phase == RESPONSE && response_valid;
    assign cfg_valid = cfg_phase == REQUEST;
    assign cfg_state = cfg_control[0];
    assign cfg_write = cfg_control[1];
    assign cfg_address = cfg_address_word;
    assign cfg_write_data = cfg_write_packed[`SHOOP_SP_STATE_BITS-1:0];
    assign audio_cfg_valid = audio_phase == REQUEST;
    assign audio_cfg_space = audio_control[1:0];
    assign audio_cfg_write = audio_control[2];
    assign audio_cfg_address = audio_address;
    assign audio_cfg_write_data = audio_write;
    assign store_cfg_valid = store_cfg_phase == REQUEST;
    assign store_cfg_space = store_cfg_control[2:0];
    assign store_cfg_write = store_cfg_control[3];
    assign store_cfg_address = store_cfg_address_word;
    assign store_cfg_write_data = store_cfg_write_packed;
    assign store_cmd_valid = store_phase == REQUEST;
    assign store_cmd_opcode = store_opcode;
    assign store_cmd_view = {32'b0, store_view};
    assign store_cmd_start = store_start;
    assign store_cmd_length = store_length;
    assign store_cmd_count = store_count;
    wire appending = store_opcode == 4'd5;
    // The streaming direction is latched when the command is accepted and the FIFO
    // levels are registered, so the store handshakes leave the bridge from flops.
    reg store_reading, store_appending, tx_ready, rx_space;
    assign store_write_valid = store_appending && tx_ready;
    assign store_write_data = tx_fifo[tx_head];
    assign store_read_ready = store_reading && rx_space;

    reg [31:0] generated_sample;
    assign sample_input_data = generated_sample;

    wire [31:0] status_word =
        (32'(faulted) << HOST_STATUS_FAULTED) | (32'(error_stage) << HOST_STATUS_ERROR_STAGE)
        | (32'(error_code) << HOST_STATUS_ERROR_CODE) | (32'(run_phase != IDLE) << HOST_STATUS_RUN_BUSY)
        | (32'(tp_phase != IDLE) << HOST_STATUS_TP_BUSY) | (32'(cfg_phase != IDLE) << HOST_STATUS_CFG_BUSY)
        | (32'(audio_phase != IDLE) << HOST_STATUS_AUDIO_BUSY)
        | (32'(store_cfg_phase != IDLE) << HOST_STATUS_STORE_CFG_BUSY)
        | (32'(store_phase != IDLE) << HOST_STATUS_STORE_BUSY)
        | (32'(reset_phase != RESET_IDLE) << HOST_STATUS_RESET_BUSY)
        | (32'(memory_quiescent) << HOST_STATUS_QUIESCENT) | (32'(done_flag) << HOST_STATUS_DONE)
        | (32'(memory_fault) << HOST_STATUS_MEMORY_FAULT);

    reg [31:0] read_value;
    always @(*) begin
        read_value = 0;
        if (read_address == HOST_ID) read_value = HOST_IDENTITY;
        else if (read_address == HOST_VERSION) read_value = HOST_VERSION_NUMBER;
        else if (read_address == HOST_STATUS) read_value = status_word;
        else if (read_address == HOST_QUANTA) read_value = quanta[31:0];
        else if (read_address == HOST_QUANTA + 12'd4) read_value = quanta[63:32];
        else if (read_address == HOST_LAST_CYCLES) read_value = last_cycles;
        else if (read_address == HOST_OUTPUT_WORDS) read_value = output_words;
        else if (read_address == HOST_DIGEST) read_value = digest[31:0];
        else if (read_address == HOST_DIGEST + 12'd4) read_value = digest[63:32];
        else if (in_window(read_address, HOST_CAPACITY, HOST_CAPACITY_WORDS))
            read_value = host_capacity[CAPACITY_BITS'(word_index(read_address, HOST_CAPACITY))];
        else if (in_window(read_address, HOST_RUN, HOST_RUN_WORDS))
            read_value = run_words[RUN_BITS'(word_index(read_address, HOST_RUN))];
        else if (read_address == HOST_TP_OPCODE) read_value = {24'b0, tp_opcode};
        else if (read_address == HOST_TP_INDEX) read_value = tp_index;
        else if (read_address == HOST_TP_ERROR) read_value = {28'b0, tp_error};
        else if (in_window(read_address, HOST_TP_DATA, HOST_TP_DATA_WORDS))
            read_value = tp_data[TP_DATA_BITS'(word_index(read_address, HOST_TP_DATA))];
        else if (in_window(read_address, HOST_TP_RESPONSE, HOST_TP_RESPONSE_WORDS))
            read_value = tp_response[TP_RESPONSE_BITS'(word_index(read_address, HOST_TP_RESPONSE))];
        else if (read_address == HOST_CFG_CONTROL) read_value = {30'b0, cfg_control};
        else if (read_address == HOST_CFG_ADDRESS) read_value = {16'b0, cfg_address_word};
        else if (read_address == HOST_CFG_ERROR) read_value = {31'b0, cfg_error};
        else if (in_window(read_address, HOST_CFG_WRITE, HOST_CFG_WRITE_WORDS))
            read_value = cfg_write_words[CFG_BITS'(word_index(read_address, HOST_CFG_WRITE))];
        else if (in_window(read_address, HOST_CFG_READ, HOST_CFG_READ_WORDS))
            read_value = cfg_read_words[CFG_BITS'(word_index(read_address, HOST_CFG_READ))];
        else if (read_address == HOST_AUDIO_CONTROL) read_value = {29'b0, audio_control};
        else if (read_address == HOST_AUDIO_ADDRESS) read_value = audio_address;
        else if (read_address == HOST_AUDIO_WRITE) read_value = audio_write;
        else if (read_address == HOST_AUDIO_READ) read_value = audio_read;
        else if (read_address == HOST_AUDIO_ERROR) read_value = {31'b0, audio_error};
        else if (read_address == HOST_STORE_CFG_CONTROL) read_value = {28'b0, store_cfg_control};
        else if (read_address == HOST_STORE_CFG_ADDRESS) read_value = store_cfg_address_word;
        else if (read_address == HOST_STORE_CFG_ERROR) read_value = {31'b0, store_cfg_error};
        else if (in_window(read_address, HOST_STORE_CFG_WRITE, HOST_STORE_CFG_WRITE_WORDS))
            read_value = store_cfg_write_words[STORE_CFG_BITS'(word_index(read_address, HOST_STORE_CFG_WRITE))];
        else if (in_window(read_address, HOST_STORE_CFG_READ, HOST_STORE_CFG_READ_WORDS))
            read_value = store_cfg_read_words[STORE_CFG_BITS'(word_index(read_address, HOST_STORE_CFG_READ))];
        else if (read_address == HOST_STORE_OPCODE) read_value = {28'b0, store_opcode};
        else if (read_address == HOST_STORE_VIEW) read_value = store_view;
        else if (read_address == HOST_STORE_START) read_value = store_start[31:0];
        else if (read_address == HOST_STORE_START + 12'd4) read_value = store_start[63:32];
        else if (read_address == HOST_STORE_LENGTH) read_value = store_length[31:0];
        else if (read_address == HOST_STORE_LENGTH + 12'd4) read_value = store_length[63:32];
        else if (read_address == HOST_STORE_COUNT) read_value = store_count;
        else if (read_address == HOST_STORE_RESULT) read_value = {23'b0, store_empty, store_code};
        else if (read_address == HOST_STORE_RESULT_VIEW) read_value = store_result_view_word;
        else if (read_address == HOST_STORE_TX_SPACE) read_value = 32'(FIFO_DEPTH) - {25'b0, tx_count};
        else if (read_address == HOST_STORE_RX) read_value = rx_fifo[rx_head];
        else if (read_address == HOST_STORE_RX_LEVEL) read_value = {25'b0, rx_count};
        else if (read_address == HOST_STORE_WRITTEN) read_value = store_written;
    end

    wire [11:0] run_write_word = word_index(write_address, HOST_RUN);
    wire [11:0] tp_write_word = word_index(write_address, HOST_TP_DATA);
    wire [11:0] cfg_write_word = word_index(write_address, HOST_CFG_WRITE);
    wire [11:0] store_cfg_write_word = word_index(write_address, HOST_STORE_CFG_WRITE);
    wire host_idle = reset_phase == RESET_IDLE;
    wire tx_push = write_fire && write_address == HOST_STORE_TX && tx_count != 7'(FIFO_DEPTH);
    wire tx_pop = store_write_valid && store_write_ready;
    wire rx_push = store_read_valid && store_read_ready;
    wire rx_pop = read_fire && read_address == HOST_STORE_RX && rx_count != 0;
    wire [6:0] tx_count_next = tx_push == tx_pop ? tx_count : tx_push ? tx_count + 7'd1 : tx_count - 7'd1;
    wire [6:0] rx_count_next = rx_push == rx_pop ? rx_count : rx_push ? rx_count + 7'd1 : rx_count - 7'd1;
    integer i;
    always @(posedge clk) begin
        if (reset) begin
            bvalid <= 0; rvalid <= 0; rdata <= 0; write_fire <= 0; read_fire <= 0;
            write_data <= 0; write_address <= 0; read_address <= 0;
            reset_phase <= RESET_IDLE; reset_count <= 0;
            cfg_control <= 0; cfg_address_word <= 0; audio_control <= 0; audio_address <= 0;
            audio_write <= 0; store_cfg_control <= 0; store_cfg_address_word <= 0;
            tp_opcode <= 0; tp_index <= 0; store_opcode <= 0; store_view <= 0; store_count <= 0;
            store_start <= 0; store_length <= 0;
            for (i = 0; i < HOST_RUN_WORDS; i = i + 1) run_words[i] <= 0;
            for (i = 0; i < HOST_TP_DATA_WORDS; i = i + 1) tp_data[i] <= 0;
            for (i = 0; i < HOST_CFG_WRITE_WORDS; i = i + 1) cfg_write_words[i] <= 0;
            for (i = 0; i < HOST_STORE_CFG_WRITE_WORDS; i = i + 1) store_cfg_write_words[i] <= 0;
        end else begin
            if (bvalid && s_axi_bready) bvalid <= 0;
            if (rvalid && s_axi_rready) rvalid <= 0;
            write_fire <= write_accept;
            read_fire <= read_accept;
            if (write_accept) begin
                write_address <= {s_axi_awaddr[11:2], 2'b00};
                write_data <= s_axi_wdata;
            end
            if (read_accept) read_address <= {s_axi_araddr[11:2], 2'b00};
            if (read_fire) begin
                rvalid <= 1;
                rdata <= read_value;
            end
            if (write_fire) begin
                bvalid <= 1;
                if (write_address == HOST_CONTROL && write_data[0]) reset_phase <= RESET_DRAIN;
                else if (in_window(write_address, HOST_RUN, HOST_RUN_WORDS))
                    run_words[RUN_BITS'(run_write_word)] <= write_data;
                else if (write_address == HOST_TP_OPCODE) tp_opcode <= write_data[7:0];
                else if (write_address == HOST_TP_INDEX) tp_index <= write_data;
                else if (in_window(write_address, HOST_TP_DATA, HOST_TP_DATA_WORDS))
                    tp_data[TP_DATA_BITS'(tp_write_word)] <= write_data;
                else if (write_address == HOST_CFG_CONTROL) cfg_control <= write_data[1:0];
                else if (write_address == HOST_CFG_ADDRESS) cfg_address_word <= write_data[15:0];
                else if (in_window(write_address, HOST_CFG_WRITE, HOST_CFG_WRITE_WORDS))
                    cfg_write_words[CFG_BITS'(cfg_write_word)] <= write_data;
                else if (write_address == HOST_AUDIO_CONTROL) audio_control <= write_data[2:0];
                else if (write_address == HOST_AUDIO_ADDRESS) audio_address <= write_data;
                else if (write_address == HOST_AUDIO_WRITE) audio_write <= write_data;
                else if (write_address == HOST_STORE_CFG_CONTROL) store_cfg_control <= write_data[3:0];
                else if (write_address == HOST_STORE_CFG_ADDRESS) store_cfg_address_word <= write_data;
                else if (in_window(write_address, HOST_STORE_CFG_WRITE, HOST_STORE_CFG_WRITE_WORDS))
                    store_cfg_write_words[STORE_CFG_BITS'(store_cfg_write_word)] <= write_data;
                else if (write_address == HOST_STORE_OPCODE) store_opcode <= write_data[3:0];
                else if (write_address == HOST_STORE_VIEW) store_view <= write_data;
                else if (write_address == HOST_STORE_START) store_start[31:0] <= write_data;
                else if (write_address == HOST_STORE_START + 12'd4) store_start[63:32] <= write_data;
                else if (write_address == HOST_STORE_LENGTH) store_length[31:0] <= write_data;
                else if (write_address == HOST_STORE_LENGTH + 12'd4) store_length[63:32] <= write_data;
                else if (write_address == HOST_STORE_COUNT) store_count <= write_data;
            end
            case (reset_phase)
                RESET_DRAIN: if (memory_quiescent) begin reset_phase <= RESET_HOLD; reset_count <= 0; end
                RESET_HOLD: begin
                    reset_count <= reset_count + 2'd1;
                    if (reset_count == 2'd3) reset_phase <= RESET_IDLE;
                end
                default: begin end
            endcase
        end
    end

    always @(posedge clk) begin
        if (core_reset) begin
            run_phase <= IDLE; tp_phase <= IDLE; cfg_phase <= IDLE; audio_phase <= IDLE;
            store_cfg_phase <= IDLE; store_phase <= IDLE;
            store_reading <= 0; store_appending <= 0; tx_ready <= 0; rx_space <= 1;
            done_flag <= 0; quanta <= 0; run_cycles <= 0; last_cycles <= 0;
            output_words <= 0; digest <= 0; generated_sample <= 0;
            tp_error <= 0; cfg_error <= 0; audio_error <= 0; store_cfg_error <= 0;
            audio_read <= 0; store_code <= 0; store_empty <= 0; store_result_view_word <= 0; store_written <= 0;
            tx_head <= 0; tx_tail <= 0; tx_count <= 0; rx_head <= 0; rx_tail <= 0; rx_count <= 0;
            for (i = 0; i < HOST_TP_RESPONSE_WORDS; i = i + 1) tp_response[i] <= 0;
            for (i = 0; i < HOST_CFG_READ_WORDS; i = i + 1) cfg_read_words[i] <= 0;
            for (i = 0; i < HOST_STORE_CFG_READ_WORDS; i = i + 1) store_cfg_read_words[i] <= 0;
        end else if (!host_idle) begin
            run_phase <= IDLE; tp_phase <= IDLE; cfg_phase <= IDLE; audio_phase <= IDLE;
            store_cfg_phase <= IDLE; store_phase <= IDLE;
            store_reading <= 0; store_appending <= 0;
        end else begin
            if (sample_input_read) generated_sample <= (sample_input_address + 32'd1)*32'd37 ^ quanta[31:0];
            if (sample_output_write) begin
                output_words <= output_words + 32'd1;
                digest <= {digest[62:0], digest[63]} ^ {sample_output_address, sample_output_data};
            end
            if (write_fire && write_address == HOST_CONTROL && write_data[1]) done_flag <= 0;
            case (run_phase)
                IDLE: if (write_fire && write_address == HOST_RUN_GO) begin run_phase <= REQUEST; done_flag <= 0; end
                REQUEST: if (run_ready) begin run_phase <= RESPONSE; run_cycles <= 0; end
                default: begin
                    run_cycles <= run_cycles + 32'd1;
                    if (done) begin
                        run_phase <= IDLE; done_flag <= 1; quanta <= quanta + 64'd1;
                        last_cycles <= run_cycles + 32'd1;
                    end
                end
            endcase
            case (tp_phase)
                IDLE: if (write_fire && write_address == HOST_TP_GO) tp_phase <= REQUEST;
                REQUEST: if (command_ready) tp_phase <= RESPONSE;
                default: if (response_valid) begin
                    tp_phase <= IDLE; tp_error <= response_error;
                    for (i = 0; i < HOST_TP_RESPONSE_WORDS; i = i + 1) tp_response[i] <= response_data[32*i +: 32];
                end
            endcase
            case (cfg_phase)
                IDLE: if (write_fire && write_address == HOST_CFG_GO) cfg_phase <= REQUEST;
                REQUEST: if (cfg_ready) cfg_phase <= RESPONSE;
                default: if (cfg_response_valid) begin
                    cfg_phase <= IDLE; cfg_error <= cfg_response_error;
                    for (i = 0; i < HOST_CFG_READ_WORDS; i = i + 1) cfg_read_words[i] <= cfg_read_padded[32*i +: 32];
                end
            endcase
            case (audio_phase)
                IDLE: if (write_fire && write_address == HOST_AUDIO_GO) audio_phase <= REQUEST;
                REQUEST: if (audio_cfg_ready) audio_phase <= RESPONSE;
                default: if (audio_cfg_response_valid) begin
                    audio_phase <= IDLE; audio_error <= audio_cfg_response_error; audio_read <= audio_cfg_read_data;
                end
            endcase
            case (store_cfg_phase)
                IDLE: if (write_fire && write_address == HOST_STORE_CFG_GO) store_cfg_phase <= REQUEST;
                REQUEST: if (store_cfg_ready) store_cfg_phase <= RESPONSE;
                default: if (store_cfg_response_valid) begin
                    store_cfg_phase <= IDLE; store_cfg_error <= store_cfg_response_error;
                    for (i = 0; i < HOST_STORE_CFG_READ_WORDS; i = i + 1)
                        store_cfg_read_words[i] <= store_cfg_read_data[32*i +: 32];
                end
            endcase
            case (store_phase)
                IDLE: if (write_fire && write_address == HOST_STORE_GO) begin store_phase <= REQUEST; store_written <= 0; end
                REQUEST: if (store_cmd_ready) begin
                    store_phase <= RESPONSE; store_reading <= !appending; store_appending <= appending;
                end
                default: if (store_done) begin
                    store_phase <= IDLE; store_code <= store_error_code;
                    store_reading <= 0; store_appending <= 0;
                    store_result_view_word <= store_result_view; store_empty <= store_result_empty;
                end
            endcase
            if (tx_pop) store_written <= store_written + 32'd1;
            if (tx_push) tx_fifo[tx_tail] <= write_data;
            if (store_phase == RESPONSE && store_done) begin
                tx_head <= 0; tx_tail <= 0; tx_count <= 0; tx_ready <= 0;
            end else begin
                if (tx_push) tx_tail <= tx_tail + 6'd1;
                if (tx_pop) tx_head <= tx_head + 6'd1;
                tx_count <= tx_count_next; tx_ready <= tx_count_next != 0;
            end
            if (rx_push) begin rx_fifo[rx_tail] <= store_read_data; rx_tail <= rx_tail + 6'd1; end
            if (rx_pop) rx_head <= rx_head + 6'd1;
            rx_count <= rx_count_next; rx_space <= rx_count_next != 7'(FIFO_DEPTH);
        end
    end
