# Shared SP/JP MIDI measurements

This evidence supplements, and does not replace, the [previous study](../midi-vivado-7020/README.md).
The accompanying [report](../../midi-shared-jobs-vivado-7020.md) explains results
and limits in plain language.

Measured shared RTL: commit `c2d6e6ff`. The audio-only control uses the original
`147c2457f46ec921f5dad5c99b5f673eda13ef99` baseline with its recorded Vivado 2021.1
syntax-compatibility patch. No MIDI modules are included in the audio-only source
set. The shared build is one connected pipeline; its source set also retains
historical modules that are not instantiated.

Reproduce the compiled capacity profile and prepare the shared source snapshots:

```sh
cargo run -p rt_fpga_testbench --features sim --example midi_profile_64 > /tmp/midi-profile.json
python3 fpga/synthesis/midi_shared_jobs.py --revision c2d6e6ff --mhz 100 --profile /tmp/midi-profile.json --output /tmp/shared-100
python3 fpga/synthesis/midi_shared_jobs.py --revision c2d6e6ff --mhz 150 --profile /tmp/midi-profile.json --output /tmp/shared-150
python3 fpga/synthesis/midi_session.py before --mhz 100 --output /tmp/audio-100
python3 fpga/synthesis/midi_session.py before --mhz 150 --output /tmp/audio-150
```

Run `vivado -mode batch -source run.tcl` in each output directory with the pinned
public Docker-sourced Vivado 2021.1 payload/runtime in `tool-provenance.json`.
The original vendor files remain unmodified and read-only. Container bind mounts
must make the output directories accessible. The tool scripts require version
2021.1, Zynq part `xc7z020clg484-1`, two tool threads, and 0.2 ns clock uncertainty.

Each run records its committed revision, configuration, exact generated context,
constraints, Tcl, compiled-profile words (shared builds), source SHA-256 manifest,
artifact SHA-256 manifest, console, implementation reports and terminal status.
Raw timing summaries distinguish estimated, placed and routed timing. Checkpoints,
tool binaries and container files are deliberately not checked in. Sources can be
reconstructed from the pinned commit and compatibility patch.

The registered measurement boundary retains public inputs/outputs without PS7,
codec or board I/O. It consumes fabric resources. It does not load/play the
compiled session during synthesis: the profile demonstrates capacity and valid
lowering; functional tests separately execute compiled images. The external DDR
controller must arbitrate the two AXI3 ports in an embedding.
