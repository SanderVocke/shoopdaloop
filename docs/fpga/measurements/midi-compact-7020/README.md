# Compact MIDI execution: reproducible measurements

The [report](../../midi-compact-vivado-7020.md) explains the result. The previous
[shared-MIDI](../midi-shared-jobs-7020/README.md) and audio-only reports remain the
before measurements. Compare `post_opt` resources: the old design failed
placement, so there is no old routed result to compare with the new routed result.

The final RTL is pinned by each run's `provenance.json` and source manifest.
Regenerate the source snapshots with that revision:

```sh
python3 fpga/synthesis/midi_shared_jobs.py --revision ad08d55e --mhz 100 --profile docs/fpga/measurements/midi-shared-jobs-7020/shared-100/compiled-profile.json --output /tmp/compact-pipelined-100
python3 fpga/synthesis/midi_shared_jobs.py --revision ad08d55e --mhz 150 --profile docs/fpga/measurements/midi-shared-jobs-7020/shared-100/compiled-profile.json --output /tmp/compact-pipelined-150
```

Run `vivado -mode batch -source run.tcl` in each directory, with the pinned public
Docker-sourced Vivado 2021.1 payload/runtime documented in
[tool provenance](../midi-shared-jobs-7020/tool-provenance.json). Vendor files remain
unmodified and mounted read-only. The flow uses `xc7z020clg484-1`, two tool threads,
0.2 ns clock uncertainty and registered public boundaries. PS7 and board DDR
arbitration remain outside this fabric-only build.

`collect.py /path/to/run-parent` retains text evidence and extracts `summary.json`.
Checkpoints and tool binaries stay out of Git. `plot.py` regenerates the vector
resource figure with Matplotlib. Additional MIDI-domain reports use the checked-in
`fpga/synthesis/midi_domain_timing.tcl`, including both the MIDI executor and its
DDR adapter. Service-only timing reports in the original run exclude that adapter;
they are not sufficient to establish timing for all MIDI hardware.

The `domain` directory retains those additional reports. `checkpoints.json`
records each analyzed checkpoint's source revision, stage and SHA-256 hash.
The complete original reports are compressed as `.rpt.gz`: their headers expand
thousands of cell names. Matching `.txt` files extract the readable timing
sections; `gzip -dc REPORT.rpt.gz` recovers each complete original report.
`banked-100/150` are the intermediate `c4d96817` implementation before the
instruction register and explicit status updates; `compact-*` use the final
`ad08d55e` implementation. The original studies' checkpoints supply `before-*`.
For example, analyze two reconstructed final checkpoints with:

```sh
vivado -mode batch -source fpga/synthesis/midi_domain_timing.tcl -tclargs /tmp/compact-pipelined-100/post_route.dcp /tmp/midi-100 /tmp/compact-pipelined-150/post_route.dcp /tmp/midi-150
```

Validation logs and cycle measurements are retained in `validation`. Reproduce:

```sh
cargo test --locked -p rt_fpga_reference --all-features -p rt_fpga_compiler
cargo test -p rt_fpga_testbench --features sim --test midi -- --test-threads=4
cargo test -p rt_fpga_testbench --features sim --test axi --test core --test audio --test graph_compiler --test loading --test targets -- --test-threads=2
cargo run -p rt_fpga_testbench --features sim --example midi_cycles_64 -- --active-loops 1
cargo run -p rt_fpga_testbench --features sim --example midi_cycles_64 -- --active-loops 64 --events-per-quantum 16
```

Cycle measurements compare packets with the Rust oracle. They cover MIDI service
jobs with deterministic delayed word memory; they exclude SP/JP dispatch, audio,
AXI/DDR-controller latency and board I/O. Clock-to-time conversions are hypothetical
at the requested frequency, not proof that the routed image meets that clock.
