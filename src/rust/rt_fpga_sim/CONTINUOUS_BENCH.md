# Continuous audio regression

The continuous bench lives in the shared
[`rt_fpga_testbench`](../rt_fpga_testbench/README.md) crate. It uses the same
clock scheduler, AXI agents and production reference as the component suites.

The coincident-clock, phase-offset/stalled and deliberately late-first-quantum
cases check 216 stereo frames per case against a frozen baseline, together with
final loop states, logical media, processed/missed/discarded counts and bank
high-water marks. Each uses Q16/chunk8/P8, reverse routing with input monitoring,
three audio banks, a three-entry control FIFO and 64-bit AXI with seven-cycle
fixed timing.

```sh
cargo test --locked -p rt_fpga_testbench --features sim --test continuous
```

The frozen baseline records its revision, scenario hash and historical tool
provenance. The Rust regression reads it directly; no comparison runner is
required. Production Rust state/media/audio comparisons also execute on every
run, independently of the frozen fixture.

Routine runs create no VCDs. Use `--waves` on a focused native CLI run when a
waveform is needed. The shared suite documents other geometries, 16-channel
operation, prepared-loader recovery, native DRAM timing and the coverage mapping.
