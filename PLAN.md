# Remaining work

[Architecture](ARCHITECTURE.md) · [Testing](VALIDATION.md)

## Application feature parity

The [feature backlog](docs/native_session_recovery/feature-parity.md) retains
unfinished application behavior and its acceptance requirements. Completed
implementation plans and historical run records are available in Git history.
Removing them does not close open features or certify the current branch.

## USB MIDI physical qualification

Software implementation exists, but no physical USB qualification has been
performed. The following scope and acceptance criteria remain required. The
platform [guide](platforms/zynq7000/README.md) contains build and operating instructions.

Make the ZC702 FreeRTOS host discover, select and exchange MIDI with
class-compliant USB-MIDI 1.0 instruments, controllers, synths and USB-to-DIN
interfaces, using TinyUSB in USB host mode. Include startup discovery, runtime
hotplug, simultaneous devices, composite devices, virtual MIDI cables and hubs.
Class-compliant USB-to-DIN interfaces use the same USB driver; their firmware
owns DIN serialization.

Use ZC702 USB0, one Cortex-A9 and 1 GiB PS DDR. Retain the current 48 kHz,
256-frame dummy-audio renderer, authenticated TCP service, client-side session
files and network/SD deployment. Add physical MIDI independently of real audio.

Initial qualification profile: four simultaneous non-hub USB devices, two
external hubs with up to two external hub tiers, and sixteen logical MIDI inputs
plus sixteen outputs in aggregate. A virtual cable is a logical port; its sixteen
MIDI channels are not separate ports. Support up to sixteen cables on a supported
MIDIStreaming endpoint within the aggregate limits. Support complete SysEx
messages of at least 2048 bytes. Publish all actual limits and refuse excess
resources explicitly. Larger configured capacities may be added after budgeting.

MIDI 2.0/UMP, proprietary USB protocols, USB audio, USB device/gadget mode,
simultaneous ownership of USB1, direct UART-to-DIN hardware and general USB
peripheral support are outside this implementation. Devices using composite
audio+MIDI descriptors must still expose their supported MIDI function.

### Acceptance criteria

1. **Real host operation:** reproducibly built FreeRTOS firmware enumerates
   supported USB-MIDI 1.0 devices on ZC702 USB0 and exchanges real bulk IN/OUT
   traffic through TinyUSB. No desktop MIDI service or Linux guest substitutes
   for the firmware USB stack.
2. **Discovery and routing:** devices attached before boot and during a running
   session appear as distinct selectable input/output ports, including separate
   interfaces and virtual cables. Connect/disconnect and duplicate identities
   are represented accurately without restarting the engine or disturbing
   unrelated routes.
3. **Compatibility profile:** the stated device, hub, port and SysEx capacities
   work within their combined limits. Qualify full-speed and high-speed devices,
   direct attachment, full-speed hubs, high-speed single-TT and multi-TT hubs,
   and nested hubs. A direct full-speed keyboard, bidirectional instrument and
   USB-to-DIN loopback must pass physical tests; also qualify multiple devices
   operating concurrently.
4. **MIDI correctness:** notes, control/program changes, pressure, pitch bend,
   system-common/realtime messages and SysEx retain payload, ordering, direction
   and cable identity. Realtime messages interleaved with SysEx remain separate
   engine messages. Timestamp conversion and output scheduling have deterministic
   tests; physical latency/jitter is measured and reported separately.
5. **Failure behavior:** malformed descriptors/packets, exhausted capacities,
   oversized or incomplete SysEx, endpoint stalls, queue pressure and device/hub
   loss cannot corrupt another port or halt rendering/control. Reattachment and
   USB-address reuse cannot deliver stale traffic to a replacement device.
6. **RT and memory:** no USB API, blocking wait, allocation or deallocation enters
   rendering. Work per render call and USB/control iteration is bounded. Pools,
   queues, stacks and DMA regions fit the existing memory contract; repeated
   hotplug reaches a stable resource baseline and preserves existing margins.
7. **Shutdown and deployment:** orderly shutdown/updater entry stops new MIDI
   work, performs bounded output recovery and drains live-device USB transfers
   before confirming cleanup. Unplugged or unresponsive devices produce explicit
   loss/failure outcomes, never fabricated delivery or an indefinite wait.
   Existing boot, update, rollback and credential/authority behavior still pass.
8. **Evidence and delivery:** portable tests, actual QEMU execution and physical
   qualification are reported separately. Required firmware/native/browser and
   deployment CI checks pass on the final PR head. Documentation includes build,
   connection, hub/power, capacity and recovery guidance. Address automated review
   and leave the PR unmerged with a clean worktree.

### Remaining qualification

| Criterion | Software evidence | Still required |
| --- | --- | --- |
| 1. Real host operation | Guest IRQ/EHCI/TinyUSB control and bulk IN/OUT; production ARM builds | ZC702 USB0 transfers |
| 2. Discovery/routing | Preboot/runtime discovery, duplicates, cables/interfaces, saved routes, retired identities and actual replacement isolation | Native UI selection on the board |
| 3. Compatibility | Four devices/16 cables, eight interfaces, nested FS model, fake FS/STT/MTT and actual EHCI TT selection | Physical device/hub/DIN matrix |
| 4. MIDI correctness | Golden codecs/clocks and exact guest recording/playback with 2048-byte SysEx/realtime | Physical payload capture and latency/jitter |
| 5. Failure behavior | Descriptor bounds/aliases, queue pressure/loss counts, stalls/NAK, generations, 100 hotplugs | Real hub loss, device faults and concurrent load |
| 6. RT/memory | Allocation guards, fixed pools, map bounds, heap/stack evidence and resource reuse | Silicon DMA/cache and load margins |
| 7. Shutdown/deployment | Observed USB reset completion, bounded refusal, U-Boot/update/cold entry/watchdog rollback | Board PHY/reset/power during these workflows |
| 8. Evidence/delivery | Reproducible scripts/manifests, docs and unmerged PR #928 | Final-head CI/review, physical evidence and clean final worktree |

## Portable loop validation handoff

The earlier loop milestone did not complete final acceptance. This is an
unfinished handoff, not an instruction to restart its retired execution workflow.
Before claiming completion:

- Run native debug/release, Node and actual non-isolated offline/live AudioWorklet
  hosts against the same implementation. Compare complete samples, MIDI,
  transition journals and receipts; verify fixed render memory and owner recovery.
- Complete selected ownership/lifetime/concurrency Miri checks. Investigate the
  sync-component timeout rather than treating partial execution as a pass or
  shortening the model. Preserve native stress workloads when interpreted runs
  use shorter loops. Miri and Chrome remain manual checks under the existing CI
  capacity decision.
- Run reference/partition invariance, mixed saturation/exhaustion teardown,
  failure canaries and release measurements. Check applicable application and
  platform CI and address available review feedback before final acceptance.
- Keep workload, failures, tool versions and reproducible commands with CI/PR
  artifacts. Generated results do not need to be committed to this repository.

### Retained loop acceptance criteria

These approved criteria describe the original milestone. They are retained for
unfinished acceptance work; they are not a claim that all hold on today's
quantum-based implementation. Resolve differences explicitly before closure.

| ID | Required result |
| --- | --- |
| LL-A1 — boundaries | `rt_runtime` remains generic, with no production/build dependency on `shoop_*`, UI, device, or browser crates. `shoop_loops` depends on the runtime, not the old engine/backend/application or browser/device facilities. Both libraries build and execute on native and `wasm32-unknown-unknown`; host bindings remain in fixtures. |
| LL-A2 — timing authority | One render-owned resolver owns authoritative primitive/composite timing. Renderers consume coherent decisions and return observations; they do not discover sync by reading partially updated peer states. Explicit planner-to-consumer dependencies are validated independently of audio/MIDI routes. |
| LL-A3 — musical execution | Every required finite transition of an admitted supported program executes at its intended sample, including A → B → C at the same frame and one-sample-loop wraps. No fixed interaction quantum, mandatory block/per-hop delay, or arbitrary per-callback wrap, propagation-round, or subdivision cap may alter musical results. |
| LL-A4 — termination | The supported trigger/program model has documented termination semantics and a progress argument. Same-frame infinite behavior is rejected/reported through that model, not a work counter. Valid cyclic sync relationships remain possible; they are not confused with instantaneous causal recursion or signal feedback. |
| LL-A5 — interval invariance | With identical initial musical state, timestamped inputs admitted before their deadlines, and sufficient prepared resources, sample output, absolute MIDI events, and semantic transition journals agree across host callback partitions and permitted declaration/traversal orders. Callback-end events execute once, never late or twice. |
| LL-A6 — callback safety | All callback-reachable planning, delivery, rendering, observation, recycling, and failure paths are allocation/reallocation/deallocation-free, nonblocking, and free of locks, waits, external I/O, and unbounded producer-chasing retries. Storage is prepared and fixed. Work is finite and application-dependent; no hard-real-time deadline claim is made. Guards remain active in debug, release, and the actual worklet. |
| LL-A7 — ownership/admission | External ingestion remains a finite callback snapshot/batch with reliable owning returns and explicit admission versus execution outcomes. Internal musical events do not consume external command/completion credits merely to progress. Stalls, cancellation, stale identities, disconnect, plan replacement, and teardown cannot leak, retarget, double-release, or destroy owners on the callback. |
| LL-A8 — media and capture | A stereo-plus-MIDI loop records, finishes at an exact frame, immediately plays the recorded content without an unintended silent block, wraps, seeks, replaces prepared content, stops with defined MIDI cleanup, and exports a coherent immutable snapshot. Playback → FX → recording works without an artificial signal cycle. Multichannel exhaustion leaves a truthful common captured prefix/incomplete status and does not abort unrelated audio. |
| LL-A9 — composites and outcomes | The resolver executes the included prepared composite features and primitive/composite interactions under one ordering/termination model. Capture/resource outcomes capable of changing timing reach the resolver before affected execution crosses their semantic boundary; speculative whole-block planning cannot conceal them. |
| LL-A10 — resource/accounting separation | Fixed storage limits and background-transfer/admission quotas remain explicit. Required rendering and internal musical progression cannot fail merely because a background CPU-work quota or diagnostic counter ran out. Indexing, capture activation, coherent publication, and reclamation have documented preparation, ownership, and exhaustion contracts. Optional planning/trace storage exhaustion does not change music. |
| LL-A11 — evidence | Deterministic reference/property tests, heap/destructor guards, selected Miri/concurrency checks, native offline/two-thread replay, Node/Chromium tests, and a real non-isolated AudioWorklet prove the new paths. Actual Web Audio output and transition/MIDI journals agree with native reference execution; ordinary Wasm memory does not grow during rendering. Performance reports expose workload, raw timings, transitions, memory/storage peaks, and failures without hiding expensive workloads. |
| LL-A12 — docs and delivery | Runtime and loop-library documentation match shipped APIs and limitations; old evidence remains labeled historical. Existing applicable runtime/application tests remain green. Milestone commits are pushed in a PR; required final-head CI is green and available automated reviews are checked and addressed through approval, or absence/unavailability is reported explicitly without inventing approval. |

## Native composition acceptance handoff

The original Milestone 1 delivered a narrower native composition slice. Its
original full acceptance was not established. Preserve the following requirements
when separately undertaking that work; current production features do not by
themselves establish the historical measurement and independent-oracle checks.

| ID | Required result |
| --- | --- |
| M1-A1 — compatibility direction | A source/test-grounded parity map identifies major current capabilities, intended owners, reusable modules, gaps and intentional semantic differences. Detailed contracts cover this slice; architecture remains explicit about undescribed parts. |
| M1-A2 — independent composition | One runtime graph/render owner executes loop playback/capture plus DSP supplied by a separate crate. The DSP crate has no production dependency on the loop, old engine/backend, UI or browser/device modules. Loop timing remains authoritative in the existing resolver. |
| M1-A3 — typed DSP control | A non-parameter-only operation such as `SelectPreparedPreset` has a component-defined request/result, executes at an admitted requested engine frame, and reports actual terminal timing/revision. It is not encoded as magic float parameters or added to the loop command vocabulary. |
| M1-A4 — musical/media correctness | A native public-API executable imports initial content, records stereo and MIDI, finishes at an interior frame, immediately plays without an extra silent block, wraps, seeks/selects retained media, and stops with appropriate MIDI cleanup. Full samples, packets, exports and outcomes match independent expectations across valid callback partitions. |
| M1-A5 — configuration cuts | Initial graph/roles are valid before frame zero without a warm-up callback. At least one supported prepared fixed-role route change activates at a known boundary with coherent state and continued capture/playback; preset changes use their declared boundary semantics. No unsupported cross-domain atomicity is claimed. |
| M1-A6 — control-side visibility | Native meters accumulate peaks with specified read/reset semantics without reliable meter-query commands. Loop queries name a coherent resolver cut. A live prefix export retains unchanged stereo/MIDI content and initial MIDI metadata while capture continues; reads/releases retain correct ownership. |
| M1-A7 — safety and lifecycle | All new callback-reachable ingress, control application, processing, telemetry publication, results and recycling are prepared, heap/free guarded, nonblocking and free of locks/waits/I/O. Fixed capacities, reserved reliable outcomes and an explicit recovery/disconnect policy preserve progression and recover owners after stalls/refusals/faulted teardown. |
| M1-A8 — parity-risk probe | An executable fixed-delay dry/wet experiment compares physical capture samples and proposed logical-window expectations with an independent oracle and current-engine references. Report supported mappings and precisely located missing mechanisms. Findings are required; full production compensation is not. |
| M1-A9 — measurement/portability | Record raw native release callback tails, command/meter costs and live-export throughput with a comparable current-engine baseline for a declared common workload. New libraries compile for native and Wasm; portable new tests run in Node. No unmeasured parity/deadline claim is made. |
| M1-A10 — collaboration/delivery | No unapproved protected-path/API changes or borrowed fixes to the loop executor's work. Final integration is checked against its completed agreed revision, relevant regressions and CI pass, runnable usage/docs are updated, and PR review is addressed or its unavailability is explicitly reported. |

## Other unresolved checks and design work

- Hosted track processors were closed at the user's direction before all CI
  finished. The subsequent unused-helper and lockfile repairs are implemented;
  verify current platform/browser/firmware checks before claiming a green matrix.
- Six composable-loop Miri commands exceeded the 900-second watchdog on the
  reference machine, including on the baseline. The timeout remains a limitation.
- Planner composition and cross-domain transactions remain future work when a
  second planning domain is needed. Keep one render owner and timing authority.
- The adapter's one-callback observation lag can schedule near-wrap requests one
  cycle later than the legacy reference; see the architecture's observation contract.
