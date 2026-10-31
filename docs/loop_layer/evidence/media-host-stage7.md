# Stage 7 prepared playable replacement and capture exhaustion

This is local implementation/host evidence, not final-head CI, Miri, performance or
full-plan completion. Source base: `d038c208f46c98ac8d7561ddcf09ff7c8e7a9d94`
(`wip` checkpoint), with the milestone implementation dirty. The commit introducing
this evidence contains the intended fixture, parser, guide and plan changes;
unrelated goal-state/loop-book work is excluded. Production libraries, dependencies,
Cargo manifests/lockfiles and the primitive LLC1/LLR1 protocol are unchanged.

## Implemented interface and scenario

- `BrowserBridge.new_media`, native direct/capacity-one actors and replay
  `--media-json` / `--media-two-thread-json` prepare the actual loop facade.
  Two indexed stereo/MIDI imports remain receiver-local: content revision0 has
  four frames, revision7 three. LPS1 adds Select27 (packed target/catalog ordinal),
  RecordAfter28 (packed target/declared source/ticks to skip), and QueryState29.
  No Rust pointer, MediaId or independent lease crosses an envelope.
- Play0 and Seek6(position1) enter through primitive credits. Explicit Play0 of
  the one-frame clock avoids a trigger/sync dependency cycle. Select4 activates
  the prepared indexed revision7. Record requested/effective/delivered7 executes
  at9 after skipping two source ticks. LPR1 preserves the delayed terminal frame;
  its decoder does not relabel runtime delivery as musical execution.
- Capture has42 prepared pages. Four import pages, five audio-prefix pages and one
  MIDI-prefix page retain40 frames; the next interval needs another audio page and
  a32-page worst-case MIDI reservation (128 records/512 payload bytes). Only32 pages
  are free. Preflight therefore stops capture before49, retaining a common incomplete
  prefix while unrelated stereo live/FX output continues1/10. This is required
  rendering storage, not a background CPU-work quota.
- Query64 reports stopped/incomplete,40 captured frames/revision8 and Capacity2.
  Previous playback content remains revision7/length3. Export65 reports latest8,
  source cuts49 and both capture stamps9–49. Owning cursor/read commands verify all
  320 interleaved audio bytes and30 MIDI bytes (cleanup of keys62/70 at relative0).
  All16 initial MIDI channel projections are checked, including held62/90 and70/50.
  Explicit Release returns generation2; normal stop refuses15 while the export is held.
- Catalog indices2/2^32/u64::MAX refuse before client credit changes. Actual receiver
  alias probes2^32/u64::MAX return Slot7 without consuming the valid retry. Malformed
  format2/foreign-session9 retries and foreign/duplicate replies preserve state.

## Complete independent checks

Closed-form sample/MIDI/semantic tables, not resolver-generated expected data, verify:

| Data | Complete result |
| --- | --- |
| Stereo engine output | 91 samples; left `3,6,9,12,11,21,21,31,11`, then1; right×10 |
| Absolute MIDI | 274 packets /4110 bytes, including per-channel restoration/cleanup |
| LSJ1 | 552 records /88360 bytes:91 boundaries,182 decisions,279 ordered events |
| Requests/results | 2/4 primitive,4/8 staging,26/46 export |
| Final ownership | all8 reported counts zero; export generation2 |
| Recovery | 42 pages /16 versions /16 snapshots /0 media references |

Native direct and capacity-one actors compare full reports across whole/single/
irregular/zero partitions of the57-frame capture call. Callback counts and positive
raw resource append revisions may vary; only descriptor bytes224–239 normalize after
positive-value checks. Logical content revisions, media, semantic state and clocks
never normalize. Fixed-step Node/Chromium/actual reports normalize actual session
bytes only and otherwise equal the complete native report.

Actual offline/live sessions use30 acknowledged calls:1/6/57, then27 one-frame
query/export/read/release calls. Deliberate driver pauses let ingress acknowledge
engine clocks; they are not mandatory processing quanta or continuous live deadlines.
A separate mono marker identifies every rendered downstream quantum without altering
stereo output. Every3840 probe sample/channel and short-call zero tail is checked;
offline additionally checks every7936 output sample/channel. All7 input and14 result
buffers return with checked transfer detachment. Both ordinary memories remain
independent/non-shared, control marker0x51a17 versus worklet0, with no isolation headers.

| Profile | Fixed media worklet bytes | Offline quantum origins | Live quantum origins |
| --- | ---: | --- | --- |
| Debug | 3342336 | 128..7552, step256 | 1024..15872, step512 |
| Release | 3276800 | 128..7552, step256 | 0, then1024..15360, step512 |

All guarded allocations/reallocations/deallocations and journal loss are zero.
Resource backing2688/read64/request96/response1024/two banks/one export credit are
component sizes, not total native RSS. JS allocation/GC/cloning and the browser's
underlying MessagePort queues are outside the Rust callback guarantees.

## Commands and observed gates

Commands ran inside `nix develop`, with `RUSTFLAGS="-D warnings"` and
`RUSTDOCFLAGS="-D warnings"`. Full invocations/output are archived.

| Gate | Observed result |
| --- | --- |
| Fixture debug/release | 57 tests per profile; no unexpected skips |
| Node and Chromium bindings | 19 groups per runtime/profile, complete native media equality |
| Actual hosts | 14 successful contexts/profile; four existing exact Rust-origin intentional failure gates/profile |
| Python host/parser tests | 21; two new mutation cases reject missing content, delayed receipts, initial state, clocks/tails, alias refusals, ownership/memory/recovery and contexts |
| Workspace nextest0.9.116 | 2141 passed, four existing skips;34.419s warm wall time |
| Build/rustdoc/doctests | warning-denied workspace/fixture builds, runtime/loop/fixture rustdoc, loop doctests pass |
| Policies | format, test attributes, both dependency boundaries, closed tracing inventory pass |
| Documented media commands | both CLI actor modes and exact actor-test filter pass in both profiles |

Tools: rustc1.97.1(8bab26f4f), cargo1.97.1(c980f4866), nextest0.9.116(2416f2fb1),
wasm-pack0.15.0, bindgen0.2.127, Node22.22.2, Chromium/driver147.0.7727.137.
No cold-cache, callback-timing, speedup or CI-margin result is inferred from suite time.
Existing processor/graph processor/heap witnesses remain distinct from the still
required planner-specific canaries. Intentionally failed sessions do not replace
successful stop/disposal evidence.

Three new applicable native cases join consolidated Stage7 debt, now33 graph/export/
staging/dense/stall/media cases plus applicable ingress/observer coverage. They are
not yet registered/interpreted. No extra per-commit Miri campaign was launched;
unchanged workloads/guards/seeds and900-second watchdog remain mandatory.

## Failures retained

The initial native prototype used the wrong Selection namespace (compile failure),
then omitted the target's declared sync source: countdown normally refused at7
rather than executing at9. The fixture now declares that source and explicitly starts
its clock; the expected frame9 was not relaxed. The first MIDI oracle incorrectly
placed held notes after all channels rather than within each channel's restoration;
the corrected complete per-channel table passes. An unspaced `49..=>` pattern caused
rustfmt parsing failure and was corrected without workload changes. All failed logs
remain archived. No runtime/native/Python/browser failure remains unresolved.

## Raw artifacts and limits

Archive: `media-host-stage7-logs.tar.gz`, **3234091 bytes**, SHA256
`0e2e0004016dddaa55fb04a36cf2900a4ecfaf36bf2fbd6c7b2cc4a93e7eb7a1`.
Root: `loop-layer-media-host-stage7/`; `final-reports/{debug,release}` contain full
native direct/threaded, Node, browser and intentional-failure reports. Earlier failed
attempts and final workspace/documented-command/version logs are included.
Parser input `scripts/tests/data/loop_media_reports.json.gz` is21784 bytes; its
captured reports are unit-test inputs, not a fresh execution substitute.

Wasm SHA256:

- Debug: `06844ea0a42be59bd98ad7597a0d1337f77b22449f5a5593be66ebb0776cd262`
- Release: `e2c0d91c337a8b3fce764c5b11b4cbb6458011bce29621a4cbde184fcec537f2`

Stage7 is **6/8**, not complete. Planner canaries, consolidated Miri/CI artifacts,
Stages8–10, final-source verification, independent completion audit, unmerged PR,
final-head green CI and review remain required. Raw MIDI uploads still lack playable
checkpoint indexes; this profile imports indexed content off-render. No production
Session/backend/worklet migration, browser-queue bound or hard-deadline claim is made.
