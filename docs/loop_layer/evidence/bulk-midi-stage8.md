# Bulk MIDI preparation and validated whole-source routing candidate

## Source and scope

Base: `15cfb7fc570c5e37f362febe5f9221d3ad58dcd2`, dirty implementation snapshot.
`implementation.patch` and scoped hashes reproduce the candidate. The only
production change is `src/rust/rt_runtime/src/buffer.rs`; no manifest, lock,
dependency, public API, capacity, wire, musical workload, assertion, guard or
900-second Miri watchdog is changed. Unrelated `.pi/`, public-book and planning
work is excluded. Native/Wasm correctness and native timing do **not** establish
interpreter recovery. Stage7 remains7/8 and Stage8 remains3/8.

## Measured-cost changes and safety invariants

The preceding residual profile measured Event initialization/fill, repeated MIDI
routing/insertion/parsing and state-valued seeks. This candidate addresses the
first two, not the seek API:

1. Preparation extends an exactly pre-reserved Event vector with safe fixed
   arrays, then explicitly pre-touches **every** descriptor through black-boxed
   chunk copies and every byte through the existing full fill. The64-element
   preparation chunk is not a callback/music quantum, truncation or admission cap.
   All ordinary and reserved reset storage retains its size and initialization.
2. A whole-source MIDI merge may append all validated bytes/descriptors after
   existing window/arithmetic/capacity/recovery checks, only when the first
   rebased timestamp follows or equals the existing last timestamp. Descriptor
   byte starts are rebased independently from timestamp order; the source payload
   arena may have been populated by out-of-order insertion. Equal timestamps keep
   destination-before-source order. Partial windows or interleaved timestamps
   retain the existing stable insertion path.

`MidiBuffer` owns private descriptor/byte arenas. Its public packet writers
validate complete MIDI and timestamp order; no raw EventBuffer can inject its
unvalidated data into that owner. Recovery buffers are handled before the fast
path. Copying a whole source therefore does not need to parse those same packets
again. Full-source bytes equal the sum of all live packet lengths; checked target
capacity also bounds every rebased byte start below the existingi32/u32 limits.
Selected timestamps and checked target end establish nontruncating offset casts.
Both paths retain identical preflight/full-reset behavior and immutable source
contents. No new allocation, owner destruction, lock, I/O or producer retry is
introduced on routing. This path review is not the exhaustive Stage8 audit.

## Independent model and guards

New exact test:
`buffer::tests::bulk_preparation_and_window_merges_match_scalar_packets_through_refusal_reset_and_reuse`.

- Eleven preparation capacities check all descriptor slots/bytes, including
 32 reserved reset events/96 bytes, empty storage and partial chunk tails.
- Five destination capacities ×256 clear/reuse rounds = **1280** full rounds.
  Independent Vec packets are stably sorted by a scalar oracle, not by production
  routing helpers. Ascending/reversed/equal/seeded timestamps deliberately differ
  from physical byte-insertion order.
- Complete1/2/3-byte packets and25-byte SysEx, full appends, interleaving, partial
  and empty windows, rebasing, event/byte exhaustion, source/target recovery,
  maximum arithmetic and malformed packet refusal compare every offset/byte,
  byte length, reset flag and source preservation. Invalid-frame refusals retain
  the previous target; overflow has the complete independent32-packet reset table.
- Clear, inserts, malformed refusal, resets and merges are heap-guarded. A dev-only
  tracking allocator is installed for the unit-test binary; an actual allocation,
  reallocation and deallocation witness requires all three counters to be positive
  before model execution. Production remains safe Rust with `forbid(unsafe_code)`.
- The same full model runs on native and Wasm; there is no Miri-only workload cut.

Runtime chapter8 explains full pre-touch and stable bulk routing in the existing
packet workflow. `midi_packets` and `coordinated_loop` pass in both native profiles;
warning-denied builds, rustdoc/doctests and the shared test-usage policy pass.

## Verification actually run

All Cargo/Python commands used the Nix development environment and strict Rust/
rustdoc flags. `native-gates.log` and `final-native-gates.log` retain full commands'
outputs; package/host logs and structured reports retain tools/versions/hashes.

| Gate | Result |
| --- | --- |
| Native runtime/loop discovery and tests, debug/release |191 runtime,153 loop per profile |
| Standalone loop fixture, debug/release |59 tests/profile |
| Package Wasm, Node22.22.2 and Chromium147.0.7727.137 |187 runtime+153 loop in each runtime |
| Loop native replay, each profile |17 commands; all eight complete native payloads equal prior measured-cost reports |
| Rebuilt loop bindings, each profile/runtime |20 groups |
| Actual loop offline/live, each profile |16 successful contexts; six exact Rust-origin error/heap gates |
| Generic runtime native replays |Both native profiles; unchanged offline golden content |
| Generic actual offline/live and failures |Fresh controls for each native profile; this fixture always builds release Wasm |
| Helper suites |72 passing tests |
| Workspace nextest0.9.116 |2144 passed/four existing skips, final warm wall34.195s |
| Format/build/docs/examples/dependency/tracing policies |Pass; closed207 modules, no inventory exemption added |

Each loop failure is rechecked for its exact code, processor/planner origin,
heap counters, lifecycle phase and equal positive initial/final memory—not merely
a nonzero exit. Full media/semantic/receipt/content/ownership checks and fixed
memory in successful actual contexts remain mandatory. Native comparison excludes
only wrapper source metadata, comparing the complete `native` payloads. Generic
raw reports retain the last actual run; both successful command summaries remain
in `hosts.log`. No remote Windows/macOS/CI or application-browser acceptance is
inferred from these local gates.

Preserved failures/corrections:

- Initial runtime package-Wasm compile failed before executing tests: the new
  model's `sum()` became ambiguous with Wasm's serde_json PartialEq implementations.
  Explicit `sum::<usize>()` fixes only that test expression, without removing or
  weakening the assertion. The loop package's153 tests passed in that attempt;
  the complete package selection then passed in both Node and Chromium.
- An evidence-helper attempt looked for the generic fixture's
  `expected_failure_observed` field in loop canaries and raised KeyError before
  writing a scope claim. Corrected verification checks the actual loop code/
  origin/heap/phase/fixed-memory witness fields. Program tests did not fail here.

## Native affected benchmark (descriptive, not a paired speedup)

The existing driver ran all14 cases ×3 trials ×1024 measured callbacks:
**43,008 callbacks**, plus128 checked warmups/case/trial, **12,288** verified
repeated cascade starts, six specific heap canaries and complete scalar/content/
request/pool/owner recovery. All **43** observed deadline misses remain. Raw CSVs,
source/binary/toolchain/hardware/configuration and memory/work/credit limitations
are retained. Timing never gates success. Old0b853b4a measurements remain labeled
historical; these uncontrolled warm native runs are not an interpreter speedup,
paired experiment, cold-cache margin or device/browser deadline guarantee.

The archived `timing-table.md` gives all fourteen distributions; the largest
observed callback is4,511,603ns in the continuous one-frame MIDI workload. Neither
the fewer observed misses nor successful correctness gates resolve Miri's timeout.

## Interpreter registration and smallest adequate recovery

Current inventory: **146 commands/299 executions =136runtime+100loop+63host**.
The new exact runtime-library model adds one command/execution, minimum1/seed0.
The before/after comparison explicitly removes only that addition and obtains
exact equality with all prior cases/counts/seeds/relative order. Host10/63 remains
unchanged. Metadata/native dispatch are not interpreter passes.

The actual unprofiled timeout, finalized residual costs and this relevant
implementation/model change justify one unprofiled retry of the same complete
exact host10/63 test after committing/freezing this candidate. All seven replays,
assertions/guards, seed0 and900-second watchdog stay intact. Even a passing retry
would prove only that test/source, not consolidated current/final coverage.
Broader models, the complete callback audit, source-matched interpreter/remote
artifacts and final delivery remain open. Nothing was pushed or merged.

## Artifacts

Local root: `target/loop-layer-bulk-midi-stage8/`. It retains initial/final native
logs, discovery, initial Wasm failure and corrected package reports, loop/generic
host reports and exact failure witnesses, old/new native equality, benchmark raw
records, inventory before/after comparison, implementation patch/hashes and scope.

Archive: `bulk-midi-stage8-logs.tar.gz`, **3,927,567 bytes**,238 files.
SHA256: `c98972984e4d3dc421794f2ec1645aec8adc39d47c2f632111834a30d9896f68`.
