# Engine IPC verification records

The acceptance mapping and limits are in
[VALIDATION.md](../../../VALIDATION.md#native-engine-ipc-evidence).

- [local.json](local.json) pins the implementation and Rust source tree for the
  warnings-denied build, retained native suite and boundary checks. Compressed
  logs retain their original uncompressed SHA256 digests.
- [miri/index.json](miri/index.json) indexes 16 selected commands and 26 passing
  interpreted executions. Reports and logs are included. Rust-source/Cargo
  manifest hashes establish that the tested dependencies did not change between
  their original pins and the implementation checkpoint. The engine run spans
  an interrupted batch and three exact recovery shards; it is not a whole-suite
  or uninterrupted-batch claim.
- [browser.json](browser.json) records the new-engine Chrome workflow and both
  worklet artifact contracts. It also retains the additional legacy debug synth
  smoke failure and its reproduction on the base branch's CI bundle.

The canonical Miri runner is `scripts/run_rt_runtime_miri.py`, with
`--package shoop_rt_protocol`, `--package shoop_rt_transport`, and
`--package shoop_rt_engine`. Recovery used the last package with `--shard 9/11`,
`--shard 10/11` and `--shard 11/11`; seed zero and the 900-second watchdog were
unchanged. Shared-memory mappings and cross-process publication are checked by
native subprocess/model tests, outside Miri.

Documentation-only closure commits can have newer SHAs than the implementation
records. Their PR checks must still pass before delivery. Historical backend and
loop-library limitations remain in their own validation sections.
