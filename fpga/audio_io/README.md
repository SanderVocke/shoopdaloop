# Standalone audio I/O

`audio_io.sv` and `audio_lane_ram.sv` preserve bank ownership, clock-domain crossing,
frame/publication cutoffs, underrun handling and reset behavior. Run:

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- audio-io
```

The tests supply explicit frame strobes and simulated core completion. The old
ARM control packet FIFO, prepared boot/demo loader and TP-backed full audio tops
are retired. Constraints remain reference material for later integration.

`i2s_duplex.sv` uses a 12.288 MHz audio clock for 3.072 MHz SCLK and 48 kHz LRCLK,
with 24-bit samples in 32-bit slots. It converts input PCM to Q8.24 and clips
output to the signed PCM range. The serial test checks both lanes, the one-bit
I²S delay and the capture/publication strobes. The board wrapper supplies the
audio clock, codec pins and timing constraints.
