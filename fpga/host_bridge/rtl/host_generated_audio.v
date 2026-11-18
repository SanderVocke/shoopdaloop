// Self-contained 48 kHz test source in the fabric clock domain.
module host_generated_audio #(
    parameter [31:0] CORE_CLOCK_HZ = 32'd100000000,
    parameter integer QUANTUM = 32
) (
    input wire clk,
    input wire reset,
    input wire enable,
    input wire [31:0] epoch,
    output wire quantum_valid,
    input wire quantum_ready,
    input wire quantum_done,
    input wire process_fault,
    output wire audio_fault,
    input wire sample_input_read,
    input wire [31:0] sample_input_address,
    output wire [31:0] sample_input_data,
    input wire sample_output_write,
    input wire [31:0] sample_output_address,
    input wire [31:0] sample_output_data,
    output wire [63:0] tx_samples,
    output wire tx_valid,
    output wire [31:0] processed_quanta,
    output wire [31:0] missed_deadlines
);
    localparam integer RATE = 48000;
    reg [31:0] phase;
    reg [15:0] sample_index;
    wire [31:0] next_phase = phase+RATE;
    wire frame_tick = next_phase >= CORE_CLOCK_HZ;
    wire frame_prepare = next_phase+2*RATE >= CORE_CLOCK_HZ && next_phase+RATE < CORE_CLOCK_HZ;
    wire [31:0] generated_sample = {{8{sample_index[15]}}, sample_index, 8'b0};

    always @(posedge clk) begin
        if (reset) begin
            phase <= 0;
            sample_index <= 0;
        end else begin
            phase <= frame_tick ? next_phase-CORE_CLOCK_HZ : next_phase;
            if (!enable) sample_index <= 0;
            else if (frame_tick) sample_index <= sample_index+1'b1;
        end
    end

    wire [63:0] audio_frame, session_frame, process_sequence, failure_sequence;
    wire [31:0] process_epoch, played_quanta, discarded_quanta, input_losses, rx_high_water, tx_high_water;
    wire [3:0] recovery_reason;
    wire busy;
    wire unused_diagnostics = &{1'b0, audio_frame, session_frame, process_sequence, process_epoch, failure_sequence,
        recovery_reason, played_quanta, discarded_quanta, input_losses, rx_high_water, tx_high_water, busy};

    audio_io #(.Q(QUANTUM), .INPUTS(1), .OUTPUTS(2), .WATCHDOG_CYCLES((CORE_CLOCK_HZ/RATE)*QUANTUM+1)) io (
        .clk(clk), .audio_clk(clk), .reset(reset), .session_reset(1'b0), .enable(enable), .epoch(epoch),
        .frame_tick(frame_tick), .frame_prepare(frame_prepare), .rx_samples(generated_sample),
        .tx_samples(tx_samples), .tx_valid(tx_valid), .audio_frame(audio_frame), .session_frame(session_frame),
        .process_valid(quantum_valid), .process_ready(quantum_ready), .process_sequence(process_sequence),
        .process_epoch(process_epoch), .process_done(quantum_done), .process_fault(process_fault),
        .input_read(sample_input_read), .input_address(sample_input_address), .input_data(sample_input_data),
        .output_write(sample_output_write), .output_address(sample_output_address), .output_data(sample_output_data),
        .recovery_requested(audio_fault), .recovery_reason(recovery_reason), .failure_sequence(failure_sequence),
        .processed_quanta(processed_quanta), .played_quanta(played_quanta), .missed_deadlines(missed_deadlines),
        .discarded_quanta(discarded_quanta), .input_losses(input_losses), .rx_high_water(rx_high_water),
        .tx_high_water(tx_high_water), .busy(busy), .audio_reset()
    );
endmodule
