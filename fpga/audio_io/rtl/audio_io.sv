module audio_io #(
    parameter integer Q = 32,
    parameter integer INPUTS = 16,
    parameter integer OUTPUTS = 16,
    parameter integer RX_BANKS = 3,
    parameter integer TX_BANKS = 3,
    parameter integer P = Q,
    parameter integer WATCHDOG_CYCLES = 1000000,
    parameter integer RX_BITS = (RX_BANKS > 1) ? $clog2(RX_BANKS) : 1,
    parameter integer TX_BITS = (TX_BANKS > 1) ? $clog2(TX_BANKS) : 1,
    parameter integer RX_ADDR_BITS = (RX_BANKS*Q > 1) ? $clog2(RX_BANKS*Q) : 1,
    parameter integer TX_ADDR_BITS = (TX_BANKS*Q > 1) ? $clog2(TX_BANKS*Q) : 1
) (
    input wire clk,
    input wire audio_clk,
    input wire reset,
    input wire session_reset,
    input wire enable,
    input wire [31:0] epoch,
    input wire frame_tick,
    input wire frame_prepare,
    input wire [INPUTS*32-1:0] rx_samples,
    output reg [OUTPUTS*32-1:0] tx_samples,
    output reg tx_valid,
    output reg [63:0] audio_frame,
    output reg [63:0] session_frame,
    output wire process_valid,
    input wire process_ready,
    output reg [63:0] process_sequence,
    output wire [31:0] process_epoch,
    input wire process_done,
    input wire process_fault,
    input wire input_read,
    input wire [31:0] input_address,
    output wire [31:0] input_data,
    input wire output_write,
    input wire [31:0] output_address,
    input wire [31:0] output_data,
    output wire recovery_requested,
    output reg [3:0] recovery_reason,
    output reg [63:0] failure_sequence,
    output reg [31:0] processed_quanta,
    output reg [31:0] played_quanta,
    output reg [31:0] missed_deadlines,
    output reg [31:0] discarded_quanta,
    output reg [31:0] input_losses,
    output reg [31:0] rx_high_water,
    output reg [31:0] tx_high_water,
    output reg busy
);
    localparam integer LATENCY = Q+P;
    reg [RX_BANKS-1:0] rx_request, rx_ack;
    reg [TX_BANKS-1:0] tx_request, tx_ack;
    (* async_reg = "true" *) reg [RX_BANKS-1:0] rx_request_meta, rx_request_sync;
    (* async_reg = "true" *) reg [RX_BANKS-1:0] rx_ack_meta, rx_ack_sync;
    (* async_reg = "true" *) reg [TX_BANKS-1:0] tx_request_meta, tx_request_sync;
    (* async_reg = "true" *) reg [TX_BANKS-1:0] tx_ack_meta, tx_ack_sync;
    (* async_reg = "true" *) reg enable_meta, enable_sync, fault_meta, fault_sync;
    (* async_reg = "true" *) reg audio_fault_meta, audio_fault_sync;
    reg audio_fault;
    reg [3:0] audio_reason;
    reg [63:0] audio_failure_sequence;
    reg core_fault;
    reg [63:0] rx_sequence [0:RX_BANKS-1];
    reg [31:0] rx_epoch [0:RX_BANKS-1];
    reg [63:0] tx_sequence [0:TX_BANKS-1];
    reg [31:0] tx_epoch [0:TX_BANKS-1];
    reg [RX_BITS-1:0] capture_bank, process_bank;
    reg [TX_BITS-1:0] produce_bank;
    reg [31:0] capture_offset;
    reg [63:0] capture_sequence;
    reg running;
    reg [31:0] watchdog;
    reg selected_valid;
    reg [TX_BITS-1:0] selected_bank;
    reg [31:0] selected_offset;
    reg [63:0] playback_sequence;
    reg [31:0] playback_offset;
    reg playback_started;
    reg [31:0] input_channel;
    wire reset_session = reset || session_reset;
    (* async_reg = "true" *) reg [1:0] audio_reset_sync;
    always @(posedge audio_clk)
        audio_reset_sync<={audio_reset_sync[0],reset_session};
    wire audio_reset = audio_reset_sync[1];
    wire audio_halt = audio_fault || fault_sync;
    assign recovery_requested = core_fault || audio_fault_sync;
    assign process_epoch = epoch;
    wire rx_available = rx_request_sync[process_bank] != rx_ack[process_bank];
    wire tx_available = tx_request[produce_bank] == tx_ack_sync[produce_bank];
    assign process_valid = enable && !reset_session && !recovery_requested && !busy
        && rx_available && tx_available;
    wire start = process_valid && process_ready;
    wire capture_free = rx_request[capture_bank] == rx_ack_sync[capture_bank];
    wire capture_write = frame_tick && enable_sync && !audio_reset && !audio_halt && capture_free;
    wire [RX_ADDR_BITS-1:0] capture_address = RX_ADDR_BITS'(32'(capture_bank)*Q+capture_offset);
    wire [RX_ADDR_BITS-1:0] core_read_address = RX_ADDR_BITS'(32'(process_bank)*Q+input_address%Q);
    wire [TX_ADDR_BITS-1:0] core_write_address = TX_ADDR_BITS'(32'(produce_bank)*Q+output_address%Q);
    wire [TX_ADDR_BITS-1:0] playback_address = TX_ADDR_BITS'(32'(selected_bank)*Q+selected_offset);
    wire [INPUTS*32-1:0] core_input_lanes;
    wire [OUTPUTS*32-1:0] audio_output_lanes;
    assign input_data = core_input_lanes[input_channel*32 +: 32];

    function automatic [31:0] increment(input [31:0] value);
        increment = value == 32'hffffffff ? value : value+1;
    endfunction
    initial begin
        if (Q < 1 || INPUTS < 1 || INPUTS > 16 || OUTPUTS < 1 || OUTPUTS > 16
            || RX_BANKS < 2 || TX_BANKS < 2 || P < 1 || P > Q || WATCHDOG_CYCLES < 1)
            $fatal(1,"Invalid audio I/O configuration");
    end

    for (genvar lane=0; lane<INPUTS; lane=lane+1) begin : rx_lanes
        audio_lane_ram #(.DEPTH(RX_BANKS*Q),.ADDRESS_BITS(RX_ADDR_BITS)) samples (
            .write_clk(audio_clk),.write_enable(capture_write),.write_address(capture_address),
            .write_data(rx_samples[lane*32 +: 32]),.read_clk(clk),
            .read_enable(input_read && busy && !reset_session),.read_address(core_read_address),
            .read_data(core_input_lanes[lane*32 +: 32])
        );
    end
    for (genvar lane=0; lane<OUTPUTS; lane=lane+1) begin : tx_lanes
        audio_lane_ram #(.DEPTH(TX_BANKS*Q),.ADDRESS_BITS(TX_ADDR_BITS)) samples (
            .write_clk(clk),.write_enable(output_write && busy && !reset_session
                && output_address/Q == 32'(lane)),.write_address(core_write_address),
            .write_data(output_data),.read_clk(audio_clk),
            .read_enable(selected_valid && !audio_reset),.read_address(playback_address),
            .read_data(audio_output_lanes[lane*32 +: 32])
        );
    end

    always @(posedge clk) begin
        if (reset_session) begin
            rx_request_meta <= 0; rx_request_sync <= 0;
            tx_ack_meta <= 0; tx_ack_sync <= 0;
            audio_fault_meta <= 0; audio_fault_sync <= 0;
        end else begin
            rx_request_meta <= rx_request; rx_request_sync <= rx_request_meta;
            tx_ack_meta <= tx_ack; tx_ack_sync <= tx_ack_meta;
            audio_fault_meta <= audio_fault; audio_fault_sync <= audio_fault_meta;
        end
    end
    always @(posedge audio_clk) begin
        if (audio_reset) begin
            tx_request_meta <= 0; tx_request_sync <= 0;
            rx_ack_meta <= 0; rx_ack_sync <= 0;
            enable_meta <= 0; enable_sync <= 0; fault_meta <= 0; fault_sync <= 0;
        end else begin
            tx_request_meta <= tx_request; tx_request_sync <= tx_request_meta;
            rx_ack_meta <= rx_ack; rx_ack_sync <= rx_ack_meta;
            enable_meta <= enable; enable_sync <= enable_meta;
            fault_meta <= core_fault; fault_sync <= fault_meta;
        end
    end

    integer rx_occupied, tx_occupied;
    always @* begin
        rx_occupied=0; tx_occupied=0;
        for (integer b=0;b<RX_BANKS;b=b+1)
            if (rx_request_sync[b] != rx_ack[b]) rx_occupied=rx_occupied+1;
        for (integer b=0;b<TX_BANKS;b=b+1)
            if (tx_request[b] != tx_ack_sync[b]) tx_occupied=tx_occupied+1;
    end
    always @(posedge clk) begin
        if (reset_session) begin
            rx_ack <= 0; tx_request <= 0; process_bank <= 0; produce_bank <= 0;
            busy <= 0; process_sequence <= 0; watchdog <= 0; input_channel <= 0;
            core_fault <= 0; recovery_reason <= 0; failure_sequence <= 0;
            processed_quanta <= 0; rx_high_water <= 0; tx_high_water <= 0;
            for (integer b=0;b<TX_BANKS;b=b+1) begin tx_sequence[b]<=0; tx_epoch[b]<=0; end
        end else begin
            if (32'(rx_occupied)>rx_high_water) rx_high_water<=32'(rx_occupied);
            if (32'(tx_occupied)>tx_high_water) tx_high_water<=32'(tx_occupied);
            if (input_read) input_channel <= input_address/Q;
            if (audio_fault_sync && !core_fault) begin
                core_fault<=1; recovery_reason<=audio_reason; failure_sequence<=audio_failure_sequence;
            end
            if (process_fault && !core_fault) begin
                core_fault<=1; recovery_reason<=3; failure_sequence<=process_sequence;
            end
            if (start) begin
                if (rx_sequence[process_bank] != process_sequence || rx_epoch[process_bank] != epoch) begin
                    core_fault<=1; recovery_reason<=4; failure_sequence<=process_sequence;
                end else begin busy<=1; watchdog<=0; end
            end
            if (busy) begin
                if (process_done) begin
                    busy<=0;
                    if (!process_fault && !recovery_requested) begin
                        rx_ack[process_bank] <= rx_request_sync[process_bank];
                        tx_sequence[produce_bank]<=process_sequence; tx_epoch[produce_bank]<=epoch;
                        tx_request[produce_bank]<=!tx_request[produce_bank];
                        process_bank <= 32'(process_bank)+1 == RX_BANKS ? 0 : process_bank+1'b1;
                        produce_bank <= 32'(produce_bank)+1 == TX_BANKS ? 0 : produce_bank+1'b1;
                        process_sequence<=process_sequence+1;
                        processed_quanta<=increment(processed_quanta);
                    end
                end else if (watchdog == 32'(WATCHDOG_CYCLES-1) && !core_fault) begin
                    core_fault<=1; recovery_reason<=2; failure_sequence<=process_sequence;
                end else watchdog<=increment(watchdog);
            end
        end
    end

    integer match_bank, expired_count;
    always @* begin
        match_bank=-1; expired_count=0;
        for (integer b=0;b<TX_BANKS;b=b+1) begin
            if (tx_request_sync[b] != tx_ack[b] && tx_sequence[b] == playback_sequence
                && tx_epoch[b] == epoch) match_bank=b;
            if (tx_request_sync[b] != tx_ack[b] && tx_sequence[b] < playback_sequence
                && !(selected_valid && selected_bank == TX_BITS'(b))) expired_count=expired_count+1;
        end
    end
    always @(posedge audio_clk) begin
        if (reset) audio_frame<=0;
        else if (frame_tick) audio_frame<=audio_frame+1;
        if (audio_reset) begin
            rx_request<=0; tx_ack<=0; capture_bank<=0; capture_offset<=0; capture_sequence<=0;
            running<=0; session_frame<=0; audio_fault<=0; audio_reason<=0; audio_failure_sequence<=0;
            selected_valid<=0; selected_bank<=0; selected_offset<=0;
            playback_sequence<=0; playback_offset<=0; playback_started<=0;
            tx_samples<=0; tx_valid<=0;
            played_quanta<=0; missed_deadlines<=0; discarded_quanta<=0; input_losses<=0;
            for (integer b=0;b<RX_BANKS;b=b+1) begin rx_sequence[b]<=0; rx_epoch[b]<=0; end
        end else begin
            tx_valid<=0;
            if (frame_prepare) begin
                if (session_frame >= 64'(LATENCY) && !audio_halt && enable_sync) begin
                    if (!playback_started || playback_offset == 0) begin
                        selected_valid<=match_bank>=0;
                        if (match_bank>=0) begin
                            selected_bank<=TX_BITS'(match_bank);
                            played_quanta<=increment(played_quanta);
                        end else missed_deadlines<=increment(missed_deadlines);
                    end
                    selected_offset<=playback_offset;
                end else selected_valid<=0;
            end
            if (expired_count != 0)
                discarded_quanta <= 33'(discarded_quanta)+33'(expired_count) > 33'h0ffffffff
                    ? 32'hffffffff : discarded_quanta+32'(expired_count);
            // Reclaim only expired, unselected banks, including results arriving after a miss.
            for (integer b=0;b<TX_BANKS;b=b+1) begin
                if (tx_request_sync[b] != tx_ack[b] && tx_sequence[b] < playback_sequence
                    && !(selected_valid && selected_bank == TX_BITS'(b))) begin
                    tx_ack[b]<=tx_request_sync[b];

                end
            end
            if (frame_tick) begin
                tx_valid<=1;
                tx_samples<=selected_valid && !audio_halt && enable_sync ? audio_output_lanes : 0;
                if (enable_sync && !audio_halt) begin
                    running<=1;
                    session_frame<=running ? session_frame+1 : 1;
                    if (!capture_free) begin
                        audio_fault<=1; audio_reason<=1; audio_failure_sequence<=capture_sequence;
                        input_losses<=increment(input_losses);
                    end else if (capture_offset == Q-1) begin
                        rx_sequence[capture_bank]<=capture_sequence; rx_epoch[capture_bank]<=epoch;
                        rx_request[capture_bank]<=!rx_request[capture_bank];
                        capture_sequence<=capture_sequence+1; capture_offset<=0;
                        capture_bank<=32'(capture_bank)+1 == RX_BANKS ? 0 : capture_bank+1'b1;
                    end else capture_offset<=capture_offset+1;
                    if (session_frame >= 64'(LATENCY)) begin
                        playback_started<=1;
                        if (playback_offset == Q-1) begin
                            playback_offset<=0; playback_sequence<=playback_sequence+1;
                            if (selected_valid) tx_ack[selected_bank]<=tx_request_sync[selected_bank];
                            selected_valid<=0;
                        end else playback_offset<=playback_offset+1;
                    end
                end else begin
                    selected_valid<=0;
                    if (running) session_frame<=session_frame+1;
                end
            end
        end
    end
endmodule
