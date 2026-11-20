// I²S master with 32-bit slots and 24-bit PCM, clocked at 256 times the sample rate.
module i2s_duplex (
    input wire audio_clk,
    input wire reset,
    input wire adc_data,
    output wire bit_clk,
    output reg word_clk,
    output reg dac_data,
    output reg frame_prepare,
    output reg frame_tick,
    output reg [63:0] rx_samples,
    input wire [63:0] tx_samples,
    input wire tx_valid
);
    reg [1:0] phase;
    reg [4:0] slot_bit;
    reg [22:0] rx_shift;
    reg [23:0] tx_pending_left, tx_pending_right;
    reg [23:0] tx_left, tx_right;

    assign bit_clk = phase[1];

    function automatic [23:0] pcm24(input signed [31:0] sample);
        if (sample >= 32'sh01000000) pcm24 = 24'h7fffff;
        else if (sample < -32'sh01000000) pcm24 = 24'h800000;
        else pcm24 = sample[24:1];
    endfunction

    wire [23:0] rx_word = {rx_shift, adc_data};
    wire [31:0] rx_fixed = {{7{rx_word[23]}}, rx_word, 1'b0};

    always @(posedge audio_clk) begin
        if (reset) begin
            phase <= 0;
            slot_bit <= 0;
            word_clk <= 0;
            dac_data <= 0;
            frame_prepare <= 0;
            frame_tick <= 0;
            rx_shift <= 0;
            rx_samples <= 0;
            tx_pending_left <= 0;
            tx_pending_right <= 0;
            tx_left <= 0;
            tx_right <= 0;
        end else begin
            phase <= phase+1'b1;
            frame_prepare <= phase == 1 && word_clk && slot_bit == 27;
            frame_tick <= phase == 1 && word_clk && slot_bit == 30;
            if (tx_valid) begin
                tx_pending_left <= pcm24(tx_samples[31:0]);
                tx_pending_right <= pcm24(tx_samples[63:32]);
            end

            // Rising SCLK samples the wire after the one-bit I²S delay.
            if (phase == 1 && slot_bit >= 1 && slot_bit <= 24) begin
                rx_shift <= rx_word[22:0];
                if (slot_bit == 24) begin
                    if (word_clk) rx_samples[63:32] <= rx_fixed;
                    else rx_samples[31:0] <= rx_fixed;
                end
            end

            // Falling SCLK changes LRCLK and drives the next bit in the slot.
            if (phase == 3) begin
                slot_bit <= slot_bit+1'b1;
                if (slot_bit == 31) begin
                    word_clk <= !word_clk;
                    if (word_clk) begin
                        tx_left <= tx_pending_left;
                        tx_right <= tx_pending_right;
                    end
                end
                if (slot_bit < 24)
                    dac_data <= word_clk ? tx_right[23-slot_bit] : tx_left[23-slot_bit];
                else dac_data <= 0;
            end
        end
    end
endmodule
