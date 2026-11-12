# Block allocation and ownership

Metadata-cache baseline: `676093aa`, preserved in PR #971 onto
`proto/fpga-rust-reference`. Follow-up branch: `proto/fpga-block-allocation`.

- [x] Preserve/push the metadata-cache implementation and publish its PR unchanged.
- [x] Add configurable power-of-two allocation blocks to the Rust logical store.
  View tables, ownership counters and the free stack operate on blocks; sample
  access remains chunk-granular. Preserve one-chunk mode for existing fixtures.
- [x] Add block mapping/ownership and whole-block cache retirement to RTL; update
  layout arithmetic, prefetch, configuration and generated integration wrappers.
- [x] Update snapshot packing/restore and add native/DRAMsim3 Rust comparisons for
  partial blocks, sharing, nested ranges, append, clear/retake, reclaim and reuse.
  Cover both AXI widths, backpressure, in-flight retirement, faults and reset.
- [x] Run targeted Rust tests, formatting, warning-denying build, schema checks,
  warning-clean RTL lint and generic/ECP5 synthesis.
- [x] Rerun original, streaming and mixed 64-loop DRAMsim3 benchmarks with 32-chunk
  allocation blocks, including the 32-reference-line comparison. Preserve graph,
  audio, quantum, buses, buffers, clock and DDR profile. Report required physical
  capacity/placement changes and completion, metadata/DDR and resource metrics.
- [x] Publish comparison reports, commit the follow-up, and report results. Leave
  the metadata PR separate; no autonomous merge or CI/review follow-up is required.

## Contract

Blocks contain 32 physical chunks by default in the new RTL configuration; one
chunk mode remains selectable. SLOTS continues to mean physical sample chunks.
Metadata capacities use floor(SLOTS/BLOCK_CHUNKS) allocation units. Sample-cache
buffers and DDR transactions remain one chunk. Reclamation stays synchronous.

Each view retains one reference per block entry. A writable view owns a dense
prefix; ranges may begin/end inside blocks and retain complete boundary blocks.
Block IDs expand to physical chunk IDs as `block * BLOCK_CHUNKS + within`.
Only a last-reference release retires the complete block and publishes reuse.
No zeroing of a whole block is required on allocation.

Snapshots retain expanded physical chunk IDs for logical views and free blocks;
an optional `block_chunks` field distinguishes blocked images (legacy default 1).
Configuration uses block IDs/counts and a frame offset within the first block.
For blocked snapshots, unpublished sample bytes are represented canonically as
zero; playback-visible samples always come from actual storage. This permits
lazy initialization without making unused DDR bytes part of the audio contract.
Legacy one-chunk snapshots retain their exact existing representation.

The original 64-view fixture uses half-full 32-chunk blocks and therefore needs
2,048 physical chunks instead of 1,024. Streaming needs 4,096 as before. Mixed
needs 2,304 to accommodate 64 active views plus eight retained old backing blocks.
These storage costs are explicit; benchmark audio topology and lengths stay fixed.

Keep this file updated and commit meaningful verified milestones. Cache behavior
remains a hardware concern; Rust models block ownership/allocation, not caching.

Implementation evidence: Rust targeted tests and 19 wire-contract tests pass.
The warning-denying workspace build passes. The full workspace nextest run
passes 3,264/3,265 tests; a LAN-profile publication timeout passes on an isolated
rerun (the full run used four test workers alongside simulators). Blocked native
AXI32 connected tests pass all profiles, including recording across block
boundaries; realistic AXI64 standalone and AXI32 audio tests pass all profiles,
and realistic AXI64 odd/stereo connected tests pass. Native AXI64 and realistic
AXI32 block-retirement tests cover dirty drain, paused/in-flight fills, held
responses, invalidation, faults and reset/reuse. Odd scalar-AXI tests pass with
three table lines, one reference/stack line, one sample read/write buffer and
physical base +8. One-chunk regressions remain available.

Both-width lint and integrated generic/ECP5 synthesis pass. The new default
AXI32 profile has 29,433 LUT4, 16,141 FF, 94 DP16KD and four DSPs. Its 1,024
physical sample chunks provide 32 allocation blocks; this is distinct from
the larger 64-loop timing profile. No whole-block sample zeroing is introduced.

The first mixed DRAMsim3 replay passes all 24 quanta and the unchanged-audio/
state/jobs equivalence check against chunk allocation. Its retake is 23,433
clocks (234.33 us at 100 MHz), with 32 reference reads and zero metadata DDR
transfers. Ordinary recording is approximately 18,251 clocks. Published clean
revision measurements and full snapshot replays follow this implementation
milestone. Generated build/incremental caches were cleared after debug-heavy
workspace builds exhausted disk; reduced-debug full tests then completed.

All benchmark/verification runs at clean implementation revision `d1a67219`
are complete. Reports, cases, allocation-equivalence checks, per-quantum counters,
processor completion times, capacities, resource counts and replay commands are
in `fpga/media_store/measurements/blocks32/`. All 52 primary measured quanta
and the additional 24-quanta reference-cache variant match Rust; separate native
replays match complete intermediate media for all 52 primary quanta. Audio, jobs,
state, readiness and transitions also match the original chunk-allocation cases.

At 100 MHz the mixed retake takes 234.33 us with either four or 32 reference
lines, versus 738.05/430.68 us before. BeginCapture work drops to 26.24 us,
reference reads to 32 from 384, and metadata DDR traffic to zero in that quantum.
Final ordinary recording is 182.51 us, so the retake is 1.28 times that cost.
The largest measured mixed quantum is startup at 262.31 us (39.3% of budget).
Warm original playback is 131.76 us versus 131.74; cold playback is 172.67 us
versus 166.53 because foreground metadata wait increases despite fewer fills.
These short-take tests do not establish a bound for long-take reclamation, which
remains synchronous and proportional to block count. No background workers or
other ownership schemes were added.

Default resource use is 29,433 LUT4 / 16,141 FF / 94 DP16KD / four DSPs. The
metadata baseline is 29,265 / 16,142 / 94 / four; block reservation increases
external sample capacity for short takes without increasing cache/descriptor BRAM.
The original and mixed timing arenas explicitly grow to 2,048 and 2,304 physical
chunks; streaming stays at 4,096. Blocked snapshots canonically omit unpublished
bytes, and the post-run Reclaim unit changes from one chunk to one block.

Implementation and publication commits are pushed on `proto/fpga-block-allocation`.
PR #971 stays separate and ready on `proto/fpga-metadata-cache` at `676093aa`,
targeting `proto/fpga-rust-reference`. CI/review follow-up is left for later,
following the user's earlier instruction; no autonomous merge is performed.
