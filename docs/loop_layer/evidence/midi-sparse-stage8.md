# Measured MIDI/preparation costs: native/Wasm-verified candidate

Verification base: `3b33803b`, with the implementation/test/example changes in the
accompanying patch uncommitted during these gates. Stage7 remains **7/8**; this
partial Stage8 optimization/model milestone is not a completed performance audit,
interpreter recovery, remote CI result or delivery claim.

## Diagnostic evidence, not passing coverage

Job `20261028T102117-ca19c81152` interpreted immutable `3574d8fc` with the unchanged
full dense restart/stall test, seed0, assertions, guards and900s watchdog plus the
sole measureme flag. A live-profile copy could not decode because its string index
had not flushed. After checking the interpreter's process identity/ancestry and
isolated build arguments, SIGINT deliberately interrupted that interpreter to
finalize diagnostic data before hard termination. The first path assertion refused
to signal; correcting the check to the actual build path preceded the signal.

The supervisor recorded **failed/exit1**, ending2026-10-28T10:34:59.550132Z after
821.2866128640017s, **zero completed executions**. All864 frozen source entries and
the copied supervisor still match; passing collection refuses the failed job.
The earlier two900s failures are not superseded by this interruption. No partial
trace, native dispatch or profile is a passing interpreter result.

A finalized copy outside reports decoded with pinned `summarize v11.0.1`, measureme
revision `9aeaf4d0cfaafb13379f0d8f3c3b5e8c13071afc`. It contains14,530 function labels
and816.845415594s of completed recorded events. Selected **inclusive** costs:

| Function | Calls | Inclusive seconds |
| --- | ---: | ---: |
| MIDI restore |257|231.276777159|
| MIDI cleanup, render instantiation |254|72.345708586|
| MIDI cleanup, discontinuity instantiation |2|0.568866998|
| Channel note lookup |1,576,960|96.103946956|
| Journal construction |2|98.056687148|
| MIDI-buffer construction |22|120.301924781|
| Ordered MIDI insertion |50,826|108.453974085|
| Its partition-point lookup |50,826|66.720217786|

Nested costs overlap: do not add these rows as independent time. This is an
interrupted interpreter profile, not completed replay coverage, native callback
performance, a cold-cache comparison or a deadline/margin measurement.

The original3,232,339,355-byte profile remains at
`target/loop-layer-miri-jobs/20261028T102117-ca19c81152/reports/measureme/loop_engine_host_fixture-3725316.mm_profdata`.
SHA256: `c1e1a5fdf46b5a8232e8d5ba2aa63cc96cf1214cbcb64e82a8d21e8caa316268`.
The archived analyzer JSON retains all labels/timings; the multi-gigabyte raw
profile is retained locally, not duplicated in Git.

## Implementation and invariants

- `ChannelState` maintains two fixed128-bit indexes alongside its existing arrays.
  Public `projected_notes()` and `known_controllers()` borrow ascending iterators.
  Known zero pressure/control values, sounding/held/sostenuto keys and opposed-mode
  clearing remain represented. Dense getters and opaque copied channels remain
  available; `size_of::<MidiState>()` includes the fixed indexes/layout padding.
- Loop cleanup/restoration uses those indexes without changing emitted packets,
  per-channel ordering, sixteen-channel neutral baselines, refusal accounting or
  conservative2096/8449 emission bounds. Fully populated state still visits every
  required key/control. No musical quantum, subdivision cap or dropped semantics.
- Prepared MIDI/event/journal storage still has every slot/byte explicitly written,
  now through safe bulk fill over black-boxed slices rather than per-byte iterator
  bookkeeping. Capacities and off-render allocation/preparation remain unchanged.
- Ordered MIDI insertion appends when the final event is not later; otherwise it
  retains stable partition-point insertion and movement. Equal-time order, payload
  validation, capacity refusal and reserved reset behavior remain unchanged.

The new heap-guarded sparse-index model compares iteration with every dense getter
across all keys/channels, known zero values,4,096 seeded updates, resets, pedals,
opposed modes, invalid atomic refusal and copied-channel reassembly. The new
64-round insertion model compares ascending/reverse/equal/seeded offsets against
an off-render scalar stable sort through capacity refusal and repeated buffer reuse.
Existing independent pedal/voice models and all musical workloads remain intact.

Runtime chapter12 and `midi_seeking` now explain/exercise the new iterators; the
loop restoration guide explains exact sparse traversal and worst-case limits.
Both affected public examples (`midi_seeking`, `coordinated_loop`) run in both
profiles, alongside warning-denied rustdoc/doctests.

## Fresh verification

All Cargo/Python payloads ran inside Nix with project Rust/rustdoc warnings denied.

- Runtime190/loop153 native tests per profile; standalone fixture59 per profile.
- Package Wasm: runtime186/loop153 in each of Node22.22.2 and Chromium147.0.7727.137.
- Both native-only profiles pass17 commands; their **complete native report data**
  equals the archived preceding CI-host reports, excluding only wrapper source
  metadata, not musical content, bytes, timing, receipts or recovery.
- Freshly rebuilt debug/release Wasm passes20 binding groups in each Node/Chromium,
  sixteen actual offline/live contexts and six exact Rust-origin failure gates per
  profile, fixed post-prepare memory/heap guards and full content/owner recovery.
- Sixty Python tooling/parser/orchestration tests; boundary, Wasm dependency,
  tracing207-module and test-attribute policies pass. Root/fixture formatting,
  locked warning-denied workspace build and2143 workspace tests/four existing
  skips pass. Workspace34.317s is warm test execution, not callback performance.

The first host attempt used a mistyped chromedriver PATH and failed before browser
validation; it is preserved, not counted. The first package-Chromium invocation
was interrupted by an insufficient240s outer tool allowance. Its own generated
webdriver configuration was identified, preserved and removed only after its
processes ended; the successful full retry retained the existing600s per-package/
3600s suite guards. No Miri900s watchdog, stack, workload or safety check changed.

The current interpreter inventory is145 commands/298 executions:
135runtime+100loop+63host. All prior filters/seeds/order remain; `midi_state` grows
6→7 and the full runtime `midi` target adds six executions. Host batch10/63 is
unchanged. The repeated concrete timeout and these measured-cost changes justify
one unprofiled retry of that same whole exact test/seed0, not a routine full run.
No interpreter speedup or successful recovery is established by these gates.
Consolidated current/final coverage, remaining Stage8 models/audit/benchmark,
remote CI artifacts and final-source review/delivery remain mandatory.

Raw logs, current reports, diagnostic state/analysis, inventory comparison and
implementation patch are in [`midi-sparse-stage8-logs.tar.gz`](midi-sparse-stage8-logs.tar.gz),
5,240,431 bytes, SHA256
`8d9967f340beafd70294f5fdfa16c8c5a59639177fa485125a6e2680bdd2d0d8`.
Local root: `target/loop-layer-midi-sparse-stage8/`. Generic-runtime-host replay/
actual-worklet regression was not freshly rerun in this scoped milestone and
remains part of the complete final gates; loop-host evidence is not its substitute.
