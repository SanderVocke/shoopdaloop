# Zynq-7000 FreeRTOS USB MIDI host

## Status and delivery

Plan written on 2026-11-03 using `.agents/prompts/write_plan.md`. Implementation
authorized on 2026-11-03. The user selected the ZC702 reference profile and asked
to defer physical testing until the software is ready for their assistance.
Physical acceptance criteria remain required and unchecked until then.
Implementation PR: https://github.com/SanderVocke/shoopdaloop/pull/928 (unmerged).

Start implementation from `feat/zynq-network-deployment`, inspected at
`de38520c8eb27d5bc9741a1fd8c394c817ba38bc`. Create
`feat/zynq-usb-midi-host` and a separate, unmerged PR targeting
`feat/zynq-network-deployment`. Record any subsequent prerequisite movement and
reconcile it without dropping the FreeRTOS or deployment integration.

The base was fetched and verified at the recorded SHA; implementation is on
`feat/zynq-usb-midi-host`. Shell commands need their network permission enabled
to reach the session proxy. No physical USB test has been performed. Stage
verification will use automated evidence first and collect physical evidence
with the user's assistance in the final qualification stage.

## Goals and scope

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

## Immutable acceptance criteria

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

## Design rules and constraints

### TinyUSB and ZC702 platform ownership

- Pin a reviewed TinyUSB revision, record its license/provenance and keep local
  port changes reviewable. Reuse its MIDI and hub class drivers and FreeRTOS OSAL.
  Enable host/MIDI/hub pools explicitly; device count, interface count, hub count,
  endpoint count and logical-port count are different budgets.
- Adapt the ChipIdea/EHCI host implementation rather than assume a ready Zynq
  port. Verify controller offsets, host-mode reset, port-speed reporting,
  periodic-list size and integrated/full-speed transaction handling against Zynq.
  Do not enable a different MCU identity merely to satisfy TinyUSB conditionals.
- Own USB0 exclusively. Verify the ZC702 ULPI PHY, MIO/reset, clock and VBUS
  configuration, host cable/adapter and board power settings. Initialization must
  work after cold boot and updater reset, without assuming bootloader USB setup.
  Do not run the AMD USB driver and TinyUSB against the controller together.
- Route USB interrupts through the existing FreeRTOS/FPU-preserving IRQ path,
  respecting the GIC priority rules for RTOS APIs. ISRs signal the USB task;
  enumeration and application callbacks execute outside interrupt context.
- Give the USB task bounded service work below the render owner. Use explicit
  aligned DMA storage, ARM barriers and validated cache-maintenance or uncached
  mappings; reserve complete cache lines/regions and retain buffers until transfer
  completion. Preserve linker boundaries and the existing Ethernet DMA region.

### MIDI bridge and live inventory

- Keep target Rust `no_std` + `alloc`; use a narrow C/Rust interface and fixed
  queues between USB, control and rendering. All TinyUSB I/O is owned by the USB
  task. Publish immutable/copied inventory snapshots to the control side.
- Prefer TinyUSB's four-byte packet API to preserve cable/CIN information.
  Validate packet lengths, cable indices and message framing. Assemble complete
  engine messages with per-cable state; split realtime bytes from SysEx and
  packetize output correctly. Bound SysEx bytes and assembly time, including
  discard/recovery after truncation, timeout or overflow. Test partial writes,
  endpoint packet boundaries and short/zero-length transfers.
- Anchor input timestamps and output deadlines to the platform monotonic clock
  and renderer frame position. Define late/future-event handling, frame offsets
  and rounding. Neither MIDI clock nor disconnect handling may depend on a UI
  connection; USB transport does not promise sample-accurate physical delivery.
- The engine prepares fixed host-port slots today. Preallocate the qualified
  MIDI capacity; map live device/interface/cable identities onto those slots.
  Expose current device labels, availability and routing through the existing
  host/application surfaces, extending versioned messages only where needed.
  Do not rebuild the engine or relabel an existing route silently on hotplug.
- Use serial identity where present together with topology/interface/cable. Duplicate or absent serials must remain distinguishable. USB addresses
  and TinyUSB class indices are reusable handles, not persistent identities.
  Tag bindings and queued traffic with a connection generation; invalidate it and
  clear partial assembly before slot reuse.
- Define admission, fair per-port service and separate overload counters. Reserve
  output-recovery capacity using the existing 32-message reset sequence. Treat
  queue acceptance and USB transfer completion as separate events; draining USB
  proves transport completion, not that an instrument played or serialized DIN.
  Device loss has a bounded terminal outcome and cannot block other outputs.

### Simulation and evidence boundaries

The base pins QEMU 10.0.13 at
`98f88efe8f07067667b8d9b1a5ce9551d34036c1`. Its Zynq machine instantiates
ChipIdea controllers at `0xe0002000`/IRQ 53 and `0xe0003000`/IRQ 76. The reduced
deployment build selects the controller through `CONFIG_ZYNQ`; its checked-in
model patch adds a watchdog, not MIDI or hub coverage.

Source inspection identifies these limitations:

- The pinned standalone EHCI model registers high-speed-only root ports; its
  built-in hub has only a full-speed descriptor. Adding `usb-hub` alone does not
  supply the required Zynq topology.
- There is no built-in USB-MIDI fixture in that source. `usb-audio` is not a MIDI
  instrument. TinyUSB's ChipIdea speed fields and periodic-list assumptions also
  need an executable compatibility probe against the model.
- Generic device routing does not establish realistic transaction-translator
  scheduling, silicon cache/DMA coherency, PHY/VBUS behavior or physical timing.

Use three complementary layers:

| Layer | Required evidence |
| --- | --- |
| Portable tests with a fake HCD | Packet/message conversion, descriptor/enumeration and hub state machines, topology changes, deterministic clocks and injected faults; exercise TinyUSB where relevant. |
| Actual QEMU firmware | USB task/IRQ/controller integration and exact bidirectional MIDI through a deterministic high-speed MIDI device fixture; retain launch, registers and USB traffic. |
| ZC702 hardware | Full-speed devices, real hubs/TTs, PHY/VBUS, DMA/cache correctness, USB-to-DIN loopback and latency/jitter under load. |

Implement only the QEMU adaptations needed for the controller/MIDI integration
gate initially. Keep model changes separate from production port logic, pinned,
hashed and tested. Broader full-speed/hub emulation is an additional simulation
milestone if justified; it cannot replace the physical acceptance criteria.
Optional libusb passthrough needs an enabled build and attached hardware, still
uses the emulated controller and does not reproduce physical hub topology.

## Staged implementation

### Stage 0 — Establish the base and freeze interfaces

- [x] Fetch/verify the requested base, preserve unrelated work, create
  `feat/zynq-usb-midi-host` and carry this plan onto it. Re-read its agent/build/test
  instructions and record base SHA, tools and prerequisite PR state.
- [x] Re-run the original firmware/deployment baselines. Record current memory,
  task and MIDI-cleanup behavior before adding USB.
- [x] Select/pin TinyUSB; audit the Zynq/EHCI port and run a minimal QEMU register,
  speed and scheduling probe. Confirm ZC702 hardware, representative devices and
  hub access for physical qualification; record unavailable resources honestly.
- [x] Freeze supported descriptor shapes, interface/endpoint pools, logical-port
  admission, SysEx/queue limits, inventory/binding semantics and shutdown deadlines.
  Budget stacks, queues, DMA/cache placement and worst-case simultaneous ownership.

Verification: publish a concrete compatibility/evidence matrix and memory budget.
Resolve controller feasibility before building the application bridge. A new
finding may revise implementation steps, but cannot remove acceptance criteria.

### Stage 1 — Port TinyUSB to FreeRTOS on ZC702 USB0

Depends on Stage 0.

- [x] Extend `platforms/zynq7000/{pins.json,build.py}` with reproducible TinyUSB
  sources/configuration and a board/HCD adapter. Add USB0 initialization,
  interrupt/task ownership and DMA/cache integration in the platform layer.
- [x] Implement bounded initialization/failure/teardown paths, diagnostics and
  restart after reset. Keep dummy MIDI fixtures selectable for existing tests.
- [ ] Enumerate a real device and complete control and bulk IN/OUT transfers;
  test full-speed and high-speed attachment and IRQ delivery under render/network
  load. Inspect link-map bounds and task/heap watermarks.

Verification: controller transfers work on ZC702 without desktop assistance.
Record QEMU successes and missing model behavior separately. Commit the platform
milestone before adding application routing.

### Stage 2 — Implement the bounded MIDI transport bridge

Depends on Stage 1; packet/clock tests can be developed after Stage 0.

- [x] Add packet/message codecs, per-cable assembly, monotonic/frame conversion,
  fixed RX/TX handoff and deadline-aware output pumping. Integrate the bridge with
  `shoop_host_freertos`, the firmware FFI and prepared `MidiBuffer` arrays.
- [x] Add reserved recovery, backpressure, disconnect generations and transfer
  completion accounting. Report received/sent/refused/dropped/failed traffic.
- [ ] Test channel/system messages, fragmented and interleaved SysEx, all cable
  indices, packet boundaries, partial writes, queue saturation and clock edges.
  Compare exact payloads through a real MIDI device or USB-to-DIN loopback.

Verification: independent golden vectors and fault tests prove framing, ordering,
isolation and recovery. Rendering performs bounded queue work with allocation
guards active. Commit the transport milestone.

### Stage 3 — Add discovery, port selection and lifecycle integration

Depends on Stage 2.

- [x] Implement the bounded live inventory and binding policy. Expose device,
  direction, interface, cable, connected state and limits to existing control/UI
  discovery and routing flows. Version and test any required protocol additions
  across firmware, native and browser clients.
- [x] Map physical ports onto preallocated engine slots. Handle startup presence,
  runtime attachment, one-direction devices, composite audio+MIDI interfaces,
  duplicate identities and explicit selection/reselection.
- [x] Extend shutdown, engine MIDI recovery and updater handoff to stop ingress,
  drain/reset live outputs and quiesce USB DMA/task ownership. Handle loss and
  timeout distinctly from successful USB completion.

Verification: select a physical port in the native UI, record and play MIDI, then
disconnect/reconnect while the engine and network remain live. Assert that stale
queued data never reaches another device and that shutdown confirmation follows
the required cleanup. Commit the application milestone.

### Stage 4 — Qualify hub topology and recovery

Depends on Stages 1–3.

- [x] Enable and size TinyUSB hub/device/interface/endpoint resources. Exercise
  hub power/status/reset and enumerate all admitted downstream MIDI interfaces.
- [ ] Test the full qualified profile, full-speed and high-speed devices,
  full-speed hubs, high-speed single-TT/multi-TT hubs and two-tier nesting.
  Include an unrelated unsupported device, excess capacity and mixed-direction
  MIDI ports without starving admitted devices.
- [ ] Test rapid hotplug, downstream hub removal during transfers, stalled
  endpoints, enumeration failures, address/slot reuse and repeated reattachment.
  Check fair service and stable memory/task/handle counts over 100 cycles.

Verification: fake-HCD topology/fault tests pass, and the physical ZC702 matrix
shows concurrent bidirectional traffic with unrelated routes preserved. Capture
device descriptors, hub topology and failures. Commit the compatibility milestone.

### Stage 5 — Add repeatable USB QEMU/CI coverage

Depends on the Stage 0 probe and Stage 1 port; final integration requires Stages
2–4. It may proceed alongside those stages once interfaces are stable.

- [x] Add a deterministic high-speed USB-MIDI device fixture with commandable
  input, independently observed output, descriptor variants and fault injection.
  Add minimal model adaptations for documented port-speed/schedule mismatches;
  preserve production controller behavior and the watchdog model.
- [x] Extend `qemu.py`, the deployment launcher and the firmware suite with owned,
  bounded USB test processes, explicit bus/port discovery and attach/detach
  controls. Make exact RX/TX assertions traverse the guest USB stack and engine;
  direct injection into engine MIDI buffers does not pass this gate.
- [x] Update model/build manifests, patch allowlists, cache keys and CI tooling.
  Retain ELF/map, source/patch hashes, launch/QMP topology, serial/USB traces,
  counters, watermarks and per-case results. Add a deliberate content-mismatch
  canary and verify it fails. Keep portable/fake-HCD tests independent of QEMU.

Verification: clean and cached builds run the USB gate without physical USB or
privileged access; timeout/failure paths leave no child processes. Label the gate's
high-speed/direct-device coverage precisely. Commit the CI milestone.

### Stage 6 — Final end-to-end qualification and delivery

Depends on all previous stages.

- [x] Run clean/cached firmware builds and both the original direct-load suite
  and SD/network deployment suite. Test USB startup after U-Boot entry, successful
  update, updater cleanup and rollback with devices connected.
- [ ] On ZC702, connect a keyboard, bidirectional instrument and USB-to-DIN
  loopback, directly and through the qualified hub matrix. From the actual native
  UI, discover/select ports, record/play exact MIDI, round-trip session media,
  disconnect/reconnect the UI, hotplug devices and request shutdown/update.
- [ ] Run simultaneous MIDI/render/network load, overload/failure recovery and
  repeated hotplug. Report physical latency/jitter, traffic correctness, renderer
  xruns, queue counters, memory and stack margins. Hardware criteria remain open
  if hardware evidence is unavailable; software success is a partial milestone.
- [x] Run target `.agents/info/test.md` gates: formatting, warning-denying build,
  complete Rust tests, tracing inventory and native/browser checks. Run
  `scripts/check_shoop_test_usage.py` if Rust tests changed. Required real-host
  tests cannot pass via a missing-backend allowance.
- [x] Document ZC702 host wiring/power, build/run commands, port identities,
  supported descriptors/capacities, hub matrix, SysEx/recovery behavior and exact
  QEMU/hardware coverage. Review each immutable criterion against retained evidence.
- [ ] Commit completed milestones, push the implementation branch and open the
  separate PR against the recorded prerequisite. Ensure all required checks pass
  on the final head; inspect arriving automated reviews and repeatedly address
  feedback as appropriate until the reviewer approves. Re-run affected checks
  after fixes and leave the PR unmerged with a clean worktree.

Verification: the final evidence links each criterion to an actual passing test;
no QEMU result is presented as physical hub, DMA/cache or timing qualification.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution evidence (in progress)

- Base fetched and verified at `de38520c`; planning milestone `2660a158` and
  platform milestone `bc20f07c`. No physical USB test has been performed.
- Native warning-denying workspace build and all 2550 Rust tests pass (5 skipped
  under the repository's native suite policy). Tracing inventory and Rust test
  attribute policy pass. Protocol v12 goldens, offline UI routes, encoded service
  availability/identity fencing and saved missing-device reconnection pass.
- Original echo, intentional mismatch, production, actual UI and recovery gates
  pass. The complete original fixture now passes the 40 MiB media round trip,
  100-cycle retained-heap check, full recording-pool exhaustion/reclamation and
  injected cleanup-failure retry. Recording-pool fill waits for confirmed recording;
  bounded service queues are preallocated to avoid later growth. Final baseline
  reruns after the nonblocking cleanup changes remain pending.
- Portable ASan/UBSan tests pass independent MIDI golden vectors, all cable IDs,
  2..2048-byte SysEx, oversized/truncated/expired assembly, clock boundaries,
  fixed queues, generations and completion-based reserved recovery. The actual
  pinned TinyUSB core/MIDI/hub fake-HCD test passes 129 mounts/unmounts and 1497
  control requests: eight interfaces across four devices, composite/one-direction
  functions, short/malformed enumeration, control timeout, endpoint stalls,
  address reuse, capacity refusal, nested FS/STT/MTT descriptors and hub change
  bits at ports 8, 15 and 31. These do not prove physical TT scheduling.
- Actual guest recording/playback preserves channel, system/realtime and 2048-byte
  SysEx payloads across direct HS/FS, sixteen cables, two nested FS hubs and four
  simultaneous devices with duplicate serials and unrelated input traffic.
  UI detach/reattach, independent output assertions and completed shutdown resets
  pass. Final combined suite is running against the latest controller fixes.
- Actual 100-cycle hotplug with bidirectional traffic on every reattachment passes.
  This exposed an EHCI QTD leak after cycle 32: endpoint teardown now quiesces
  schedules and frees detached QTD/QHD ownership before reuse. Endpoint OUT stall
  keeps render/control live; permanent OUT NAK refuses shutdown after five seconds
  of guest time while stopping the controller. The final suite also checks that
  no controller-handshake error is hidden by a successful high-level result.
- USB-enabled signed deployment passes preboot-attached discovery, firmware A→B→A
  network updates with exact USB reset delivery, fresh cold-entry nonce/discovery
  and two failed trial boots followed by watchdog fallback. Final rerun remains
  pending after the last EHCI teardown correction. The original SD/network suite
  passed before USB; final regression and CI remain open.
- Evidence-driven budget revision: the original 16-message queue cannot admit
  the engine's normal 32-message reset burst; a 64-entry queue also overflowed at
  SysEx/loop boundaries. Fixed queues now hold 256 complete events and the bridge
  ceiling is 32 MiB inside the unchanged 128 MiB platform region. USB render block
  storage is 4096 bytes per port; each individual message remains at most 2048.
  Link-map and task/heap evidence must pass the final resource gate.
- Evidence-driven identity rule revision: topology is always part of an ID, even
  with a serial. This distinguishes duplicate serials without rekeying an existing
  route when a second identical unit arrives. Four duplicate-serial QEMU devices
  pass isolation; moving hardware to another hub port requires explicit reselection.
- Build manifests pin source/patch/fixture/binary hashes and record source inputs.
  Cached emulator builds reconstruct the reviewed patch set and reject unexpected
  source changes. Test credentials/signing keys/private Ethernet captures are
  excluded from public evidence; deployment outputs pass the existing secret audit.
- Transport/routing milestone committed as `c7635157`. The complete twelve-phase
  USB suite passed locally, including the 100-cycle soak, mismatch canary,
  endpoint stall/NAK and signed deployment recovery. Subsequent review corrected
  nearest-high-speed-TT selection for FS→FS→HS nesting; the actual patched EHCI
  constructor passes 32-bit sanitizer tests, with affected guest regressions next.
  Hub status parsing now supports ports 1–31 and explicitly refuses larger hubs.
  EHCI interrupt intervals are validated before schedule access, including zero
  and out-of-range HS intervals; the actual HCD entry point passes sanitizer tests.
- The platform map places the static bridge at 17,060,352 bytes (below its 32 MiB
  ceiling) and platform load regions below 64 MiB (within 128 MiB). In the 100-cycle
  guest soak, Rust peak was 347,607,056 bytes within 768 MiB and final use was zero;
  FreeRTOS minimum free heap was 16,149,992 bytes. USB stack free was 7873 words
  of 8192. These are simulator margins; silicon load/timing qualification is open.
- PR #928 targets the unchanged prerequisite at `de38520c`. Firmware, USB,
  deployment, native/browser, coverage, documentation and CodeQL CI are running;
  no automated review has arrived yet.
- Local native regression passes all 2550 tests; Node Wasm passes 2142 tests in
  33 packages. Warning-denying native and browser/worklet builds pass, as do
  Chromium core and Web MIDI record/save/load/playback workflows with zero xruns.
  The complete original seven-phase firmware suite passes, including the 40 MiB
  fixture, 100-cycle heap window and recording-pool exhaustion/reclamation.
- The final robustness pass counts queued-message loss on disconnect/recovery,
  separately from pending USB packet loss, and verifies empty output queues at
  teardown. Descriptor validation rejects duplicate default interfaces and active
  endpoint aliases across interfaces. Portable sanitizer tests pass; the complete
  USB suite is rerunning these changes before the next commit.
- CI on `db10380b` passes original firmware, all twelve USB phases, bare-metal
  builds, Linux release and documentation; remaining platform/deployment/coverage
  checks are in progress. No automated review has arrived.
- The complete original SD/network-deployment suite now passes locally and in
  CI. The twelve-phase USB suite also passes locally and in CI. Final robustness
  changes then pass direct HS, direct FS and 100-cycle guest runs, with exact
  MIDI/reset delivery and empty queues/completion ownership at each drain point.
- Control retires an offline, unreferenced physical identity before requesting
  C slot reclamation. A retirement marker suppresses old snapshots until USB
  acknowledges it; pending session replacements retain their device pins. Native
  adapter/service tests pass, all 2550 Rust tests pass again, and the 68 affected
  Node Wasm tests pass. The actual guest rejects a late old-ID connection, leaves
  a different-serial replacement unrouted, then exchanges MIDI after selection.
- All native-platform, firmware, USB, deployment, coverage-upload and browser-debug
  CI jobs pass on `db10380b`. Browser release failed its 120-second large-load
  deadline. The identical uploaded release artifact passes the complete large-file
  workflow locally on the exact pinned Chrome 147.0.7727.117 with zero xruns;
  final-head browser CI still needs to pass without changing test limits.
- Physical qualification remains deferred at the user's request, not waived.
  `platforms/zynq7000/usb/QUALIFICATION.md` contains the required hardware matrix.
  No physical PHY/VBUS, cache/DMA, TT, DIN or latency result is claimed.

## Acceptance review before physical qualification

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

## Source references

- [Inspected base and platform](https://github.com/SanderVocke/shoopdaloop/tree/de38520c8eb27d5bc9741a1fd8c394c817ba38bc/platforms/zynq7000),
  including the existing FreeRTOS/deployment plans, build, pins and launchers.
- [FreeRTOS engine/MIDI integration](https://github.com/SanderVocke/shoopdaloop/blob/de38520c8eb27d5bc9741a1fd8c394c817ba38bc/src/rust/shoop_host_freertos/src/embedded.rs)
  and [fixed host-port preparation](https://github.com/SanderVocke/shoopdaloop/blob/de38520c8eb27d5bc9741a1fd8c394c817ba38bc/src/rust/shoop_rt_engine/src/description.rs).
- [TinyUSB host documentation](https://docs.tinyusb.org/en/latest/reference/class/host.html),
  [inspected ChipIdea adapter](https://github.com/hathach/tinyusb/blob/27ed5944736c6145cb8674622a74c798d92db867/src/portable/chipidea/ci_hs/hcd_ci_hs.c)
  and [MIDI host implementation](https://github.com/hathach/tinyusb/blob/27ed5944736c6145cb8674622a74c798d92db867/src/class/midi/midi_host.c).
- Pinned QEMU [Zynq machine](https://github.com/qemu/qemu/blob/98f88efe8f07067667b8d9b1a5ce9551d34036c1/hw/arm/xilinx_zynq.c),
  [EHCI model](https://github.com/qemu/qemu/blob/98f88efe8f07067667b8d9b1a5ce9551d34036c1/hw/usb/hcd-ehci.c)
  and [hub model](https://github.com/qemu/qemu/blob/98f88efe8f07067667b8d9b1a5ce9551d34036c1/hw/usb/dev-hub.c).
