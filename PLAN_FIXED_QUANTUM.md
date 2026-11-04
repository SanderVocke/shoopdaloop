# Configurable fixed-quantum redesign

## Goals and acceptance criteria

Replace adaptive graph intervals and the loop planner/observation lifecycle with
one configurable fixed tick. Preserve supported behavior except for the explicit
alignment policy. Keep preparation, ownership and reclamation safe for real-time
execution. Complete production native/browser integration and retire the old
execution path only after parity and end-to-end validation.

- Every core graph call processes exactly the prepared quantum (16/32/64/128,
  default 32), independently of host callback size. Q=1 also provides fixed,
  sample-accurate reference/offline execution.
- Loop lengths, seeks and transitions align to that grid. Requested/effective
  times and same-tick ordering are deterministic. MIDI retains intra-tick offsets.
- Arbitrary host partitions preserve the global clock and report buffering latency.
- Supported recording, MIDI, sync/program, graph/media editing, persistence and
  recovery behavior remains covered; existing unsupported features stay tracked.
- RT execution does not allocate, free owners, lock, wait, or perform I/O.
- Validation reports full-callback microseconds, budget use, tails, maxima and all
  misses for steady and transition-heavy work. VM timings are not device acceptance.

## Design rules

Quantum is immutable after preparation. Changing it requires stop/reprepare.
Host capacity, core quantum and plugin block size are separate settings. Alignment
rounds future requests upward, never earlier; overdue requests use the next eligible
tick. Unaligned stored durations enter through explicit offline conversion;
source media is retained until conversion succeeds. Tempo conversion derives related lengths from a
shared tick clock and must not silently drift through independent rounding.

Explicit sample-rate/quantum conversion, dependency-aware loop retiming and atomic
live import are specified in [the media conversion plan](PLAN_MEDIA_FORMAT_CONVERSION.md).
Its implementation and local validation precede all remaining CI work and human
acceptance.

The outer callback owns ingress and maintenance budgets. Each tick settles changes,
executes the compiled graph, then advances capture/state. No partial graph tick is
permitted. General host adaptation has constant declared latency; direct divisible
hosts may use a separately prepared zero-buffering mode.

Cache prepared routing and state bindings. Keep atomic preflight for resource/owner
changes; removing interval planning does not remove required failure/ownership rules.
Do not port the benchmark's opt-in flags or bypasses into the production lifecycle.

## Implementation

- [x] Create `feat/fixed-quantum-runtime` from `feat/backend-replacement`; retain
  independent benchmark worktree/evidence.
- [x] Specify timing types, alignment and callback contracts; verify with scalar
  reference tests, overflow/refusal cases and all supported quantum sizes.
- [x] Implement compiled fixed audio/MIDI graph execution with prebound routing,
  Q-sized buffers and a direct shared state lifecycle. Verify DAG output, exact
  tick counts, MIDI ordering, errors, heap guards and callback-scoped maintenance.
- [x] Implement direct and arbitrary-partition host adapters. Verify constant
  latency, clocks, audio/MIDI correspondence, zero blocks and partition invariance.
- [x] Implement direct tick-based loop playback, transitions, sync/programs,
  recording and media ownership. Verify against independent reference models,
  aligned legacy cases, capacity failures and resource reclamation.
  - [x] Primitive audio/MIDI playback, aligned seeks, gain, stop and state queries.
  - [x] Grouped whole-tick recording, coherent publication, monitored previous
    content, explicit exhaustion and failed-start preservation.
  - [x] Sync/program compilation and domain precedence/request-family semantics.
  - [x] Live media/graph publication and complete shutdown/export transport.
    - [x] Prepared fixed-catalog routing publication, tick-aligned epochs,
      processor state retention and control-side retirement with heap guards.
    - [x] Quiescent command settlement and recording cleanup after normal stop
      or a partial-tick fault, preserving unread results under saturation.
- [x] Integrate engine, protocol/host configuration, settings, persistence and
  recovery. Verify native and browser production routes and capability parity.
  - [x] Share production strip/fader/bus/reader DSP with fixed-core execution,
    including ramps, meters, MIDI monitoring and callback-scoped bulk reads.
  - [x] Prepare production engine parts with independent host capacity/Q and
    typed node operations, including inactive slots and shared ingress budgeting.
  - [x] Integrate the full protocol endpoint and logical slot lifecycle; carry Q
    through startup/configuration/settings/persistence and select the fixed host.
- [ ] Re-run whole-callback performance experiments for regular, two-transition,
  simultaneous-burst, recording, MIDI and graph/media workloads; include plugin
  block/readiness adapter coverage and target-device evidence.
  - [x] Initial checked audio playback sweep: 983,040 whole callbacks over
    16/32-frame quanta, four graph sizes and three transition scenarios.
  - [x] Final-source audio repeat and comparison report, including every raw
    callback, pooled tails, all misses and historical comparison tables.
  - [ ] Recording/MIDI/edit/plugin and target-device performance.
- [ ] Run final native/browser end-to-end and parity gates, remove superseded
  execution code, push, open a PR, obtain green CI and address automated review
  feedback through approval.

Each stage depends on its used interfaces, but independent work may move earlier.
Partial milestones remain explicitly partial; do not switch production to a path
that lacks currently supported functionality.

## Production replacement continuation

The user approved replacing the backend on this branch. A separate checkout at
`6de0b7a1` is the comparison reference; production transports carry a quantum
setting, without backend selectors. Shared command vocabulary, resource ownership,
topology compilation and transaction staging are retained. Execution bypasses adaptive graph intervals, borrowed
decision lookup and consumer observation acknowledgements. Transactional musical
preflight remains necessary for coherent capacity failures and request families.

- [x] Add fixed execution hooks to the integrated runtime and loop domain so the
  complete existing control vocabulary can target fixed ticks. Enforce the grid
  for topology, media durations, seeks and future deliveries.
- [x] Replace engine execution, including direct and buffered host
  processing, preserving endpoint transactions, exports, telemetry and recovery.
- [x] Carry quantum through native/process/worklet setup, protocol,
  application settings and persisted configuration; report Q and stream latency.
- [x] Implement explicit session/media format conversion and atomic affected-family
  imports using [the conversion plan](PLAN_MEDIA_FORMAT_CONVERSION.md).
- [ ] Run parity scenarios against the separate reference checkout, including sync,
  program families, slot generations, media replacement, cancellation, shutdown,
  persistence and reconnect/recovery. Keep component tests as additional evidence.
- [ ] Remove superseded execution, obtain green native/browser CI, and update
  measurements for the integrated implementation.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Progress and evidence

Single-kernel continuation: adaptive graph execution is removed. Generic capture
start/stop/snapshot and graph publication wait for an eligible tick, including when
host input for an earlier tick was already collected. MIDI recovery suppresses later
same-callback packets after resetting consumers, and terminal fault output contains
one device reset. The configurable headless service uses direct loop/DSP nodes and
accepts correctly rounded DSP controls while still refusing actual late selections.

Validation: 66 selected core/domain/engine/service/worklet cases pass in both Node
and Chromium; production application/worklet Wasm builds and codec/AudioWorklet
artifact contracts pass. Runtime/engine dependency boundaries pass on all four
supported targets; warning-denying workspace build, formatting, test attributes,
tracing inventory (369 modules) and book checks pass. All runnable runtime/loop
examples were exercised; the bounded generic example now declares Q=32 and checks
capture revisions per tick. These debug smoke timings are not performance evidence.
Final full native run passes all 2,598 tests, with four host-facility skips.

An initial final-suite attempt raced a default workspace build against tests, which
replaced the test-enabled host executable. The corrected run separates compilation
from test execution. An isolated recovery repeat also observed a timing-sensitive
Stopped-versus-Recording snapshot; no assertions or waits were weakened.


Production cutover continuation: engine preparation uses whole fixed ticks and
direct loop consumers through the full existing protocol/ownership endpoint.
Q reaches persisted host settings, native/process startup and worklet records;
protocol capacities report Q and the constant host stream delay. A one-frame
fixed mode preserves exact-sample behavioral fixtures without an interval fallback.
Additional Q=16/32/64/128 tests exercise arbitrary host partitions, real audio/MIDI
record/play/export and slot reuse, MIDI overflow recovery, delayed synchronized
recording and live program replacement with exact journals. The generic executor's
adaptive callback/graph-range execution has since been removed. Older processor
interfaces have whole-tick adapters; production and headless service nodes use
direct domain access. Standalone resolver interval analysis remains a library API,
not a selectable graph backend. Runnable examples pass under that contract. Final CI/device and integrated performance gates remain open.


Initial branch: `6de0b7a18f04caf43f8cb76f81b1b2c040e089e2`.
The earlier experiments are in `/workspace/work/fixed-quantum`; no experimental
source edits were copied into this branch.

Fixed-core milestone: `rt_runtime::fixed` implements immutable prepared routing,
Q-sized buffers, direct state hooks, terminal faults and MIDI reset output, a
bounded copy-command SPSC queue with reserved cancellation, and a constant-latency
audio/MIDI host adapter. Command ingress obeys a callback-wide budget, including
when cancellation targets work that has not yet been admitted. Prepared routing
publication is available for the immutable processor catalog. Its owning channel
returns retired/rejected routing to control; publication is tick-aligned and
validates namespace, expected epoch and bounded state requirements. Atomic
graph/media/owner transactions remain incomplete.

Primitive-loop milestone: `shoop_loops::fixed` owns cached sources and tick state,
reusing capture/lease primitives without resolver preview/commit or consumer
observations. It supports audio/MIDI playback, seeks, gain, stop, state queries,
whole-tick recording, previous-content monitoring, coherent take publication,
explicit capacity failure and cleanup. Content and active-capture revisions are
separate. Repeated-take tests alternate banks 32 times and recover every page.
Quiescent shutdown retains unread results and disconnects pending/unadmitted
requests exactly once while holding both endpoints. It also reclaims a take
after a fault during partial-tick staging without replaying that tick.
Sync/program arbitration and production integration remain incomplete. At this earlier milestone the production application still selected the interval path.

Production-composition continuation: `shoop_rt_engine::fixed` prepares the real
engine processors next to the direct loop bank and reuses capacity calculations.
Typed operations bind targets off-render, execute before same-tick route changes,
and share normal callback ingress capacity with loop commands. Engine DSP render
bodies are shared between both paths. Fixed bulk streaming retains one allowance
across all ticks. At this earlier milestone host/protocol startup still selected the interval composition;
complete logical slot/endpoint lifecycle was added by the production cutover.

Validation completed:

- Production-DSP continuation: warning-denying workspace build and 2587 native
  tests pass (four host skips). Node and headless Chromium each pass 14 core,
  eight loop and six engine cases, including the full production signal chain,
  recorded audio/MIDI, meters, staged configuration, cross-tick ramps and bulk
  transfer limits. Logical slot/endpoint and host startup cutover remain pending.

- Live-routing continuation: 2582 native workspace tests pass (four host skips),
  with 12 core and eight loop cases passing natively, in Node and Chromium.
  Tests cover changed dependency order, inactive processor retention, same-tick
  MIDI offsets, callback ingress snapshots, stale/foreign/refused candidates,
  capture rerouting and quiescent ownership return after a render fault.

- Nine fixed-core and seven fixed-loop integration cases pass natively, under
  Node 24.19.0 / wasm-bindgen 0.2.127, and headless Chromium 154.0.8037.57 with a
  matching-build ChromeDriver 154.0.8037.92. These are library/adapter browser
  tests, not AudioWorklet or physical-device acceptance.
- The production application and worklet compile separately for Wasm, and the
  client-codec/worklet artifact contract passes. Runtime/engine dependency
  boundaries pass for all four supported targets. These checks exercise the
  existing production path; the fixed core has not been wired into it yet.
- Warning-denying native workspace build passed. The full native nextest run
  passed 2578 cases, with four host-facility skips, using the documented features,
  three test workers and loopback permission. Test-attribute, boundary, book,
  tracing-inventory, formatting and diff checks passed.
- Initial validation attempts are retained under `work/`: the first full test
  build exhausted disk space, resolved by removing only generated debug output
  and rebuilding without debug info/incremental compilation. An earlier loaded
  run passed 2574/2575 tests with an existing bounded lifecycle stress failure;
  that test passed 20 actual isolated executions. A repeat omitted loopback
  permission and failed IPC tests. The final correctly permitted full run is
  green. No existing test was weakened or removed.

The checked `fixed_quantum_benchmark` example and `benchmark_fixed_core.py` driver
retain every callback, verify every sample and graph visit count outside timing,
and report full-128-frame callback means, p99/p99.9, maxima and deadline misses.
The initial sweep observed two missed deadlines and the final-source repeat
observed three, all retained. Across both runs that is five in 1,966,080 measured
callbacks. The final 64-loop means were 143.8/143.7/148.5 us for Q=32 and
170.8/171.5/176.6 us for Q=16 (steady/two-seeks/two-bursts). These use
5.39/5.39/5.57% and 6.40/6.43/6.62% of the full 128-frame/48-kHz callback budget.
The maximum was 4060.3 us (152.26%); the VM run does not establish xrun safety.
See `docs/fixed_quantum/README.md` and its checked evidence for per-configuration
tails, all misses and the original/prototype comparisons. Full application
parity/cutover and device/plugin acceptance remain unchecked above.

The branch was pushed after explicit authorization. Draft PR #930 targets
`feat/backend-replacement`; production integration continues on the same branch.
Docs CI passed on the initial head; build/test and CodeQL were still running
when integration work resumed. No automated review comments had arrived.
