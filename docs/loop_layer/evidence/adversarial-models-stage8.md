# Stage 8 adversarial timing, delivery and capture/index models

Source base: `dfd610d90c3484dc7ec7ffaa86d1cf80f861c6d6`, plus the complete scoped
`implementation.patch` in the archive. Three new integration-test files and the
additive interpreter inventory/helper tests change; no production API, callback
body, dependency, manifest or lockfile changes. The source proof compares all208
existing scoped Rust/configuration/fixture/runner inputs with that base byte for
byte. The new Rust files have no shorter `cfg(miri)` workload.

## Models and independently checked results

Table prefixes name integration-test targets; exact filters are the function names
after `::`, passed with `--test TARGET FILTER -- --exact`.

| Exact case | Workload and checks |
|---|---|
| `adversarial_timeline::seeded_programs_preserve_scalar_timing_through_full_receipts_journal_loss_and_rejected_owners` | Four seeds×eight generated regular/explicit programs, each across three journal capacities and three partition/declaration profiles:288 runs. The independent scalar simulator supplies all64 frames of primitive states, composite cursors and complete ordered events. All three external receipt slots stay occupied while music progresses. Invalid unit, overflowing delay and descendant trigger recursion refuse. Same-epoch owning replacement returns the exact node-array storage and preserves current epoch/state/journal/retained requests. Cleanup forces request-slot reuse with a new generation, rejects stale cancellation and returns every credit. |
| `adversarial_timeline::seeded_sync_components_deliver_each_occurrence_once_with_saturated_results_and_optional_journals` | Four seeds×eight generated sync rings of2–17 nodes, periods1–7, across the same nine profiles:288 runs. An independent event list checks exact membership and multiplicity; the actual full event order must also agree across partitions/declarations. Every per-frame state, cycle and last-pulse value is checked. Both request credits remain retained, including an eligible-source countdown with `skip=u64::MAX`; optional journal loss cannot change music. |
| `adversarial_delivery::seeded_typed_payloads_receipts_and_recycled_tokens_match_a_full_length_independent_model` | Four seeds×512 mixed actions, then complete drain and maximum-sequence checks. An allocating off-render model independently tracks two normal credits, one recovery credit, queued/delivered/terminal/read states and terminal FIFO. Every delivery copies the expected payload after the caller overwrites its original buffer; every result checks full sequence/action/lane/requested/delivery/terminal frames and lateness. All18 semantic outcome witnesses must occur, including both full lanes, all credits retained, success/domain failure/cancellation, deferred credit release, early/duplicate/stale/foreign tokens, malformed-payload refusal and delayed terminal execution. A rejected higher sequence must not prevent the following lower valid admission. High-bit frames/sequences survive; sequenceMAX is usable once per lane and cannot reset after recycle. |
| `adversarial_capture::seeded_capture_indexes_keep_immutable_prefixes_across_partitions_reuse_and_exhausted_maintenance` | Four seeds×four successive32-frame stereo/MIDI takes×three callback partitions:48 takes/1536 stereo frames. A16-byte legacy read consumes the minimum valid background byte budget once per nonempty callback. Required capture and indexed reads still progress with zero remaining credits. An unaligned optional snapshot correctly refuses `Budget` at frame1 without mutation; aligned audio64/MIDI48-byte prefixes at frame8 retain immutable views with no tail copy. Full256-byte audio/168-byte MIDI captures, all11 complete Note/real-time/SysEx packets, initial state, every strict-pretarget seek and replay bound are checked independently. Old full snapshots survive replacement; incomplete/cancelled intervals, wrong members/shapes and stale tokens preserve published state. Snapshot-credit exhaustion refuses transactionally, forced same-slot reuse changes generation, old/foreign indexes cannot retarget, and maximum offsets refuse. All128 pages/eight versions/eight snapshot credits/three MIDI rows recover. |

Each case first proves that its shared instrumentation observes real allocation,
reallocation and deallocation, then guards actual resolver/scheduler/resource
mutation and reads. Reference vectors, compilation, expected-content construction,
result comparison storage and owning destruction stay outside guarded operations.
The experimental frame counts/partitions are not production interaction quanta or
musical-work limits. Capture checkpoint positioning projects the documented MIDI
state, not full synthesizer/SysEx history.

### Stage 8 model-row coverage

These additions complete the broader model row together with the already verified
[descriptor-bank model](descriptor-model-stage8.md), [sparse/indexed MIDI models](midi-sparse-stage8.md)
and [bulk merge scalar model](bulk-midi-stage8.md). They retain all preceding
models unchanged, including the native/Wasm8192-step typed lifecycle model (its
historical64-step Miri variant is unchanged), the complete2048-action capture
ownership model, arithmetic/index/generation exhaustion, malformed wire records,
program staging, concurrency and mixed lifecycle/disconnect suites. This closes
**only the model/property implementation row: Stage8 is6/8**. The separate complete
applicable interpreter and checked remote-CI rows remain open.

## Executed gates

| Gate | Result |
|---|---|
| Runtime/loop/standalone fixture, debug and release | **193/155/60 per profile**,816 passes total |
| Exact native dispatch of each new interpreter filter | **8/8**, one test per command; not interpreter evidence |
| Workspace CI profile, nextest0.9.116 | **2148 passed/four existing skips**,34.372s |
| Helper tests | **73 passed** |
| Node package-Wasm | **189 runtime+155 loop**, no failed/ignored/malformed/missing cases |
| Chromium package-Wasm | **189 runtime+155 loop**, same complete discovery/content checks |
| Strict build/format/rustdoc/doctests/test-attribute/tracing/both dependency boundaries/Wasm dependency policy | Passed |

Commands are archived as `commands/native.sh`, `commands/wasm.sh` and the one-off
execution/source proof `commands/verify.py`. Rust/rustdoc use `-D warnings` inside
Nix. Tools: Rust/Cargo1.97.1, nextest0.9.116, Node22.22.2,
Chromium/ChromeDriver147.0.7727.137, wasm-pack0.15.0,
wasm-bindgen0.2.127/test0.3.77. Package watchdog600s and suite watchdog3600s remain.
Node builds the package test inputs; Chromium explicitly reuses matching dev
assets. This is not a new actual-worklet, benchmark or remote-platform run.

The initial occurrence test incorrectly supplied a source other than the target's
declared sync source and correctly received `MissingSync`; only its setup was
corrected. Capture development first used a nonexistent `Info` type and then an
invalid one-byte transfer budget. The corrected setup uses `ResourceInfo`, the
minimum valid16-byte budget, and spends it completely on each background read.
The optional-tail-copy refusal is explicitly tested rather than bypassed. Initial
failure logs and passing retries are retained. A one-off discovery schema probe
also looked for a nonexistent nested `binary` key; the final proof uses the actual
`package-name`/`binary-name` fields and checks all discovered identities.

## Interpreter scope and outstanding debt

The new selection is **151 commands/304 executions =138runtime+102loop+64host**.
Removing only the four appended exact seed0 model commands reproduces the complete
prior147/300 inventory: every old case/filter/count/seed and relative order matches.
Native exact dispatch, synthetic runner tests and package-Wasm passes do not certify
interpretation. **These four cases and the earlier descriptor case have not run
under Miri.** No routine interpreter invocation was launched per helper.

While this milestone ran, the frozen Stage7 recovery completed and was collected:
[verified146-command/299-execution evidence](miri-consolidated-stage7.md). It proves
only `3bf1dbca` and cannot include these later cases. Commit/freeze the accumulated
model milestone and run one consolidated applicable Stage8 selection, retaining
all workloads, seeds, guards and900s per-command watchdog. Remote source-matched
native/worklet/Miri artifacts, final-head CI/review and Stages9–10 remain required.

## Archive

[Complete logs/proofs](adversarial-models-stage8-logs.tar.gz): **207092 bytes,
43 files**, SHA256 `735fb1e55b9a177d4bda1d2fe759d8a6acd0d6b1ebef867bd245f856465b0664`.
Every archived member was compared byte for byte with its local source. The archive
contains the complete scoped patch/input hashes, before/after inventories, native
and Wasm discovery/raw reports, exact dispatches, strict gates and initial failures.
It does not certify arbitrary programs, callback deadlines or subsequent source.
