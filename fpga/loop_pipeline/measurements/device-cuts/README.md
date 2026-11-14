# Autonomous media-cut prerequisite checkpoint

This is partial M5 work. It does not claim application callback separation,
retained public queries, final timing or physical qualification. The new capture
profile and the general kernel share the existing TP resolver and media store.
[The contract](../../MEDIA_CUTS.md) describes the remaining connections.

In instant, stalled fixed-latency and DRAMsim3 memory, the real integrated sample
path processes 1,050 quanta with recording, playback and a retake, without host
TP/media/observation service after initial admission. Each mode compares all
33,600 stereo sample frames and 2,100 coherent pre/post observations with
production Rust, including modes, positions, lengths, capture counts, revisions,
cycles, repeat flags and pulse state. `samples-*.json.gz` retains every output
sample and full cut row. This is stepped fabric simulation; it does not prove
ARM scheduling or replace the still-unmigrated near-capacity application test.

Other tests cover historical export/empty identities over repeated retakes,
stale IDs, error precedence, exhaustion, reset, and bounded finalization of a
240,000-frame take against a stalled view allocator. Retained-record RAM tests
fill 3 and 256 cells, pause for 1,024 simulated quanta, then verify every word,
generation/epoch-qualified ACK, reuse and reset during a partial copy. That RAM
is a component prerequisite, not yet the public query transport. The independent
TP observer and resident command port operate with a held host reply; execution
and SP reads retain priority. All five TP capacities with 20 seeds pass the
observer checkpoint. Later private-command tests verify authoritative reads and
collection without consuming or modifying the held reply.

Combined media/demo/board tests pass (4 + 1 + 1), including existing three-memory
recovery/runner cases. Strict lint passes general and stereo capture profiles;
Q32 generic synthesis, warning-denying workspace build, formatting and test policy
pass. Logs retain the initial harness failures (an unavailable JSON helper and
incorrectly treating a generated artifact-directory name as a stable path).
Corrected runs pass; initial failures remain visible.

`source.tar.gz` is an exact 197-file source overlay over the recorded base commit;
`source-manifest.json` gives SHA-256 hashes. Cargo.lock pins dependencies. Rust
1.100.0, Verilator 5.032, Marlin 0.21.1 and DRAMsim3 are used. Replay with:

```sh
cargo test --locked --release -p rt_fpga_testbench --features sim \
  --test media_cuts --test demo --test zedboard -- --nocapture
```
