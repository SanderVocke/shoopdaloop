# Live signal-image checkpoint

This is an M4 functional checkpoint, before the later registered admission,
resident-action pipeline and metadata-cache timing changes. It is not M5,
physical qualification or final timing closure.

The full continuous test uploads 512 instructions, 512 initial values and 512
live-copy descriptors while stereo audio runs. The shared ARM owner admits
activation at frame 2,048 and two independent channel ramp changes at frame
2,560. Service then stops for 1,040 quanta, retaining all 128 transport replies
and an incomplete incoming batch. After service resumes, retained records carry
the exact cuts. All 34,340 stereo frames match fixed-point arithmetic; the maximum
independent floating error is 5.959e-8. No quantum is missed or discarded.
The smallest publication margin is 118.755 microseconds, 54.07% of the measured
last-input-to-prefetch window, at activation. These are accelerated simulated
frame clocks (13 ns audio clock, 512 clocks per frame), not physical 48 kHz.

`live-image-continuous.json.gz` retains every sample and publication measurement.
`source.tar.gz` and `source-manifest.json` freeze the exact 167 source files used
by model `audio_loop_pipeline-e54f6283c75fca12`, over commit
`a134d1bb92704e1c9fd066ca9e5df48818bc3336`. The archive is a source overlay;
restore it over that revision to replay the checkpoint. Cargo.lock at that
revision pins dependencies. Tools are Rust 1.100.0 and Verilator 5.032.

Replay the functional gate with:

```sh
cargo test --locked --release -p rt_fpga_testbench --features sim \
  --test image_control --test audio_io -- --nocapture
```

The component tests also cover copy permutations, full numeric ramp ranges,
private/partial/unsorted publication, moving-cut publication retry, exact late
frames, cancellation, credit exhaustion, generation reuse, counter exhaustion
and uncertain ownership. No mock resolves loop decisions.

Logs preserve the first failures: an overflow-injection test needed a reset
between independent nonce/version exhaustion cases, and moving the pure signal
encoder into the shared adapter required a Result error conversion in the
testbench. Corrected reruns pass; those initial failures are not relabelled.
`continuous-actions-test.log.gz` is earlier regression evidence, including the
three memory models; its inputs precede this exact source overlay.

`combined/` freezes the subsequent combined M4 functional gate: 173 source files,
12 passing continuous/demo/image/board tests, strict Q16/Q32/Q64 lint/generic
mapping and ECP5 mapping. It includes registered admission, pipelined resident
ordering, DMA selection and metadata miss phases. The large-image test again
matches all 34,340 stereo frames; maximum independent float error is
5.9582816902548075e-8. Its minimum simulated publication margin is 118.742 us.
The separate 160-quantum flood fills both ingress and all 128 result credits;
backpressure affects the producer while audio and resident transitions continue.
Raw combined logs and both complete result reports are retained alongside the
source overlay. Final Vivado timing remains a separate, open gate.
