# Event-only MIDI RTL evidence

See the [report](../../midi-data-jobs.md) for interpretation and scope. Measurements
use the 64 stereo-plus-MIDI loop configuration, 32-frame audio chunks/quanta, and
Vivado 2021.1 for `xc7z020clg484-1`. They measure resources and clock timing, not
quantum throughput or physical DDR arbitration.

- `summary.json`: resource and timing values, configuration, and test totals.
- `compiled-profile.json`: the new shared SP program/job budget for the session.
- `before-100/`, `before-150/`: preserved pre-port post-optimization reports and
  provenance from `9c75aa49`. The 100 MHz baseline also has post-placement evidence.
  Earlier baseline routing was stopped; no baseline routed result is claimed.
- `after-100/`, `after-150/`: new post-optimization, placement, and routing reports
  from RTL revision `0bc04a54`. `outcome.txt` records tool-flow completion; a zero
  flow status does **not** mean timing passed. Read the timing reports too.
- `baseline-verification.json`: all baseline source hashes checked against
  `3c0d4f11`, confirming that the Rust rewrite had left that RTL unchanged.
- `tool-provenance.json`: public Docker-image digest, verified extraction layers,
  runtime identity, vendor-file provenance, and exact tool version.
- `artifact-hashes.json`: SHA-256 hashes for the selected evidence in this folder.
- `plot_resources.py`, `resource-impact.svg`: figure source and exported chart.
  Run `python3 plot_resources.py` with Matplotlib to regenerate it from the summary.

These are selected text reports, not full build directories. Large checkpoints,
tool binaries, temporary synthesis databases, and source copies are omitted.
The original manifests describe the complete build inputs, including some files
not duplicated here. The committed RTL and the synthesis harness reproduce them.
The 100 MHz post-synthesis checkpoint hash and reason for resuming are recorded
in its provenance; `checkpoint-save-error.txt` preserves the failed save outcome.

The new configuration holds 64 sequences, 64 readers, 256 list descriptions,
256-event blocks, 8,192 events per view, 2,432 event blocks, and 131,072 scratch
records. The previous 256-entry input buffer is removed. Counts and payloads may
cross block boundaries; finite DDR storage remains configurable and explicit.

The functional validation is 139 selected passing tests: 103 Rust reference/
compiler/RTL-package tests, 15 MIDI simulations, and 21 shared-pipeline simulations.
The passing shared-pipeline total includes a rerun after a Verilator compilation
ran out of disk space. Full workspace/browser/device suites were not run. Historical
MIDI benchmarks and state-aware fixtures do not measure or validate this design.
