# Apply: integrated nested seek/stop/restart example and verification

Implementation parent: `9203bc43`. This is added integration coverage, not a new
musical policy or completion of the whole plan. Runtime/program-change guides were
previously read and their current hashes rechecked in `nested-lifecycle-baseline.json`.
The documentation worker should add a runnable nested-control example/cross-reference
without replacing the existing standalone semantics or historical evidence.

Use `tests/interloop.rs` and `tests/support/nested_lifecycle.rs` as the source of the
example. Preserve the older 137-page A→B→C/capture-exhaustion scenario. The new scenario
uses256 pages to test sufficient-capacity semantics; it does not raise the old test's
capacity or weaken its exhaustion checks.

## Required timeline explanation

A one-frame primitive B supplies actual clock pulses and starts C. The outer regular
program delays an explicit inner program, which controls D through Play/Record/Stop.
All four primitive playback→FX→capture chains have actual audio/MIDI ports.

- Select B's retained revision at0, then start B and the outer program at1. Selection
  and a later Play are distinct controls, not two competing direct transitions at0.
- D first plays at2. Seeking the outer program into its inner recording segment at3
  starts D's first take immediately. Its initial MIDI projection contains the note
  held before3; the renderer's cleanup at3 is an original captured event at offset0.
- Freeze at5, seek back at6 and play the captured content. The old prefix/full exports
  remain immutable. FX output while recording is1; playback of that recorded input
  through the same mix produces1.5, not an unprocessed copy of the input.
- At8 a direct D Stop beats the nested Record intent from an outer seek. It creates
  no new take/revision; continuation of the same inner segment does not invent a
  delayed start at9.
- A pending countdown requests an outer iteration change at10. A later-sequence Stop
  wins at11 and cancels it. A direct D Play at position1 still beats inherited child
  Stops. MIDI restoration at that position excludes the already-replayed note-off;
  the next wrap restores the captured initial state and its original offset-zero
  packet in the proper order.
- Restart the outer program at13, record D again at15, and freeze at17. The new take
  has revision2, not a gap caused by the losing Record intent. Seek into the outer
  delay at18 and stop the musical sources at19.

Clarify that stopped composites may relay real sync pulses without advancing their
program cursors. Also distinguish loop Stop from muting an independent host input:
the FX chain still passes D's external input while its loop is stopped. Exports carry
the original owner, revision, duration and cut, not the later inspection frame.

## Evidence and work accounting

The new guarded case compares every output sample on four ports, all absolute MIDI
packets, complete ordered/truncated journals and cumulative drop counts, every public
State field at all20 boundaries, delivery/cancellation frames, three immutable
exports, and channel0 initial MIDI notes/controllers. Every expected boundary must
contain all six decisions; missing states are not silently skipped. Whole/single/
irregular partitions, reversed declarations and journals0/1/1024 agree. Cleanup twice
recovers256 pages,64 versions/snapshots and zero ledger references.

The planner also avoids searching pending requests for a graph stage when no program
was selected. This is an explicit redundant-work removal, not a semantics change,
measured speedup or deadline claim. The pre-adjustment19-gate report/logs and planner
source are retained in `target/loop-layer-nested-lifecycle-stage6/first-broad/`; final
evidence is source-matched to the adjusted code.

`nested-lifecycle-stage6{,-provenance}.json` records19 scoped gates:153 loop cases per
native debug/release/Node/Chromium profile,2141 workspace passes/four existing skips,
warning-denied build/rustdoc/examples, policies/boundaries and14 runner helper tests.
Eight positive prototypes and23 expected compiler refusals are refreshed separately.
The initial fixture had an unspaced open-ended range before `=>`; its failed compiler
log is preserved. No failed intermediate-tree reproduction is claimed.

Miri registration is **228 =128 runtime+100 loop**, not a new interpreter pass.
The28 topology cases and both interloop cases now have individual exact filters.
Native discovery equals all30 names and every exact filter executes one passing test.
This preserves each workload, seed0, guards and the900-second command watchdog.
Historical interloop time468.43s plus growing grouped suites motivates this granularity;
it is not an observed new timeout or a smaller workload. Grouped historical interpreter
results are not claimed as evidence for the new inventory. Consolidated Stage6 Miri,
independent hosts, broader Stage8 stress and final acceptance remain separate work.
