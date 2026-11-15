# Standalone audio I/O

`audio_io.sv` and `audio_lane_ram.sv` preserve bank ownership, clock-domain crossing,
frame/publication cutoffs, underrun handling and reset behavior. Run:

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- audio-io
```

The tests supply explicit frame strobes and simulated core completion. The old
ARM control packet FIFO, prepared boot/demo loader and TP-backed full audio tops
are retired. Constraints remain reference material for later integration.
