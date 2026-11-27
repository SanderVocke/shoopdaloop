// Pmod I2S2 in slave mode. Its two MCLK inputs share the external 12.288 MHz reference.
module host_i2s_audio #(
    parameter [31:0] CORE_CLOCK_HZ = 32'd100000000,
    parameter integer QUANTUM = 32,
    parameter integer WATCHDOG_CYCLES = (CORE_CLOCK_HZ/48000)*QUANTUM+1
) (
    input wire clk,
    input wire reset,
    input wire enable,
    input wire [31:0] epoch,
    (* X_INTERFACE_INFO = "xilinx.com:signal:clock:1.0 audio_ref CLK" *)
    (* X_INTERFACE_PARAMETER = "FREQ_HZ 12288000" *)
    input wire audio_ref,
    input wire i2s_adc_data,
    output wire i2s_bclk,
    output wire i2s_lrclk,
    output wire i2s_dac_data,
    output wire quantum_valid,
    input wire quantum_ready,
    input wire quantum_done,
    input wire process_fault,
    output wire audio_fault,
    output wire watchdog_fault,
    input wire sample_input_read,
    input wire [31:0] sample_input_address,
    output wire [31:0] sample_input_data,
    input wire sample_output_write,
    input wire [31:0] sample_output_address,
    input wire [31:0] sample_output_data,
    output wire [31:0] missed_deadlines
);
    // Keep the serializer, audio banks and counter on the same synchronized reset.
    wire audio_reset;
    wire frame_prepare, frame_tick, tx_valid;
    wire [63:0] rx_samples, tx_samples;
    wire [31:0] audio_missed;
    wire [3:0] recovery_reason;
    assign watchdog_fault = audio_fault && recovery_reason == 2;
    reg [31:0] missed_gray;
    (* async_reg = "true" *) reg [31:0] missed_meta, missed_sync;
    always @(posedge audio_ref)
        missed_gray <= audio_reset ? 0 : audio_missed ^ (audio_missed >> 1);
    always @(posedge clk) begin
        if (reset) begin
            missed_meta <= 0;
            missed_sync <= 0;
        end else begin
            missed_meta <= missed_gray;
            missed_sync <= missed_meta;
        end
    end
    generate for (genvar bit_number = 0; bit_number < 32; bit_number = bit_number + 1) begin : decode
        assign missed_deadlines[bit_number] = ^missed_sync[31:bit_number];
    end endgenerate
    i2s_duplex serial (
        .audio_clk(audio_ref), .reset(audio_reset),
        .adc_data(i2s_adc_data), .bit_clk(i2s_bclk), .word_clk(i2s_lrclk), .dac_data(i2s_dac_data),
        .frame_prepare(frame_prepare), .frame_tick(frame_tick), .rx_samples(rx_samples),
        .tx_samples(tx_samples), .tx_valid(tx_valid)
    );

    // The current graph compiler exposes one mono audio input and two outputs.
    audio_io #(.Q(QUANTUM), .INPUTS(1), .OUTPUTS(2), .WATCHDOG_CYCLES(WATCHDOG_CYCLES)) io (
        .clk(clk), .audio_clk(audio_ref), .reset(reset), .session_reset(1'b0), .enable(enable), .epoch(epoch),
        .frame_tick(frame_tick), .frame_prepare(frame_prepare), .rx_samples(rx_samples[31:0]),
        .tx_samples(tx_samples), .tx_valid(tx_valid),
        .process_valid(quantum_valid), .process_ready(quantum_ready), .process_done(quantum_done),
        .process_fault(process_fault),
        .input_read(sample_input_read), .input_address(sample_input_address), .input_data(sample_input_data),
        .output_write(sample_output_write), .output_address(sample_output_address), .output_data(sample_output_data),
        .recovery_requested(audio_fault),
        .audio_frame(), .session_frame(), .process_sequence(), .process_epoch(), .recovery_reason(recovery_reason),
        .failure_sequence(), .processed_quanta(), .played_quanta(), .missed_deadlines(audio_missed), .discarded_quanta(),
        .input_losses(), .rx_high_water(), .tx_high_water(), .busy(), .audio_reset(audio_reset)
    );
endmodule
