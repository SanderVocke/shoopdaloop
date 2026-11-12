# FPGA audio I/O implementation plan

Status: implementation in progress on `feat/fpga-audio-io`. Stage 1 complete.

Base: `proto/fpga-rust-reference`, inspected at `012ed6bd`. The current local
`work` branch points to that base. Follow `.agents/prompts/write_plan.md` when
executing and maintaining this plan.

## Goals and scope

Implement continuous, fabric-only audio buffering around the real transition,
state, job, media and AXI processing pipeline. The initial configuration has 16
input and 16 output channels, all in lockstep on one audio frame timeline.
Demonstrate generated input -> processing graph -> output, including direct
monitoring and recording/playback through 16 independent logical loops.

The I/O subsystem knows channel numbers and frame numbers. Stereo pairing,
logical loop identities, synchronization between loops and signal routing belong
to the processing graph. A future TDM/ADAT adapter assembles/disassembles these
same lockstep frames; implementing that adapter is outside this milestone.

Support build-time quantum changes, shallow configurable buffer rings, fixed
`2Q` and `Q+P` playback latency, recoverable output deadline misses and bounded
escalation when processing continuity is lost. Audio stays in PL. No ALSA,
ARM audio transport, new audio DMA, physical codec, optical interface or board
deployment is included. Existing media DDR/AXI transfers remain in use.

## Immutable acceptance criteria

These criteria may only change with explicit user approval.

1. A synthesizable top connects the new I/O subsystem to the actual processing
   pipeline with 16 simultaneous inputs and 16 outputs. The loopback/demo does
   not substitute a behavioral processor or require host execution per quantum.
2. Every channel uses the same sample-frame timeline. Directed patterns prove
   channel identity and frame alignment through monitoring, recording and
   playback. No stereo or loop grouping is encoded in I/O buffering.
3. Both `2Q` and `Q+P`, with integer `1 <= P <= Q`, work with fixed, declared
   latency. Increasing bank capacity alone does not increase selected latency.
   Q can change at build time without redesigning the I/O modules.
4. Playback never waits for processing. A missing output becomes silence for
   its entire scheduled quantum across all outputs. Late output is discarded;
   partial results, stale banks and delayed replay never reach the sink.
5. An isolated output deadline miss recovers without resetting the session
   when all inputs remain available and execution succeeds. Processing and
   recording continue in order, state advances exactly once per processed
   quantum, and subsequent audio resumes at the original latency.
6. Input loss, watchdog expiry and actual core/memory failures enter a bounded,
   observable recovery protocol. Output remains silent, outstanding traffic is
   safely resolved, and reset/reload/rearm demonstrably starts a fresh epoch.
7. Channel routing uses real Input/Output and BeginCapture/Capture/ReadView
   jobs. The 16-loop demonstration records distinct channel data, plays it back
   exactly, and supports independent loop controls without cross-channel writes.
8. Existing standalone and integrated testbenches remain operational with their
   semantic coverage intact. New continuous-audio tests verify scheduling and
   recovery separately from functional processing comparisons. Required CI passes.

## Existing integration points

- `fpga/job_processor/rtl/job_processor.sv` owns single-quantum input/output RAM
  and scratch RAM. Input/Output jobs use channel-major addressing; output is
  cleared before each run. The current audio configuration port is not a design
  constraint and may be replaced.
- `fpga/loop_pipeline/rtl/loop_pipeline.sv` coordinates transitions, state/jobs
  and successful clock advancement. A processing failure latches a fault;
  successful whole-pipeline completion is the publication boundary.
- `fpga/axi_memory/generate_tops.py` generates AXI and native-memory wrappers.
  Public port changes must propagate through the generator, canonical testbench
  and generated tops, rather than hand-edited generated files.
- `src/rust/rt_backend_testbench/src/rtl.rs` already lowers graph ports to Input
  and Output instructions and each logical loop channel to its own mono media
  view. `fpga/verification` compares the production Rust engine with RTL.
- Existing standalone Rust/reference comparisons in `fpga/job_processor`,
  `fpga/media_store`, `fpga/loop_pipeline` and `scripts/fpga_testbench` remain
  useful regression gates. This work does not retire either verification path.

## Design rules and constraints

### Frame and clock contract

Use a protocol-neutral audio-domain interface carrying one complete frame of
signed 32-bit Q8.24 samples for all configured channels, with a common frame tick.
It neither accepts per-channel clocks nor independently stalls channels. Future
serial-protocol adapters handle slots, word widths and conversion at this boundary.
The initial synthesizable source produces deterministic, channel-distinct frames;
the sink exposes/observes the resulting frames. This is an open signal path,
not output-to-input feedback.

Support an audio interface clock unrelated to the processing clock. The audio
interface clock services frame ticks; it need not equal the sample rate. Keep
the authoritative audio frame counter and playback decisions in that domain.
Use dual-clock BRAM banks for samples and per-bank request/acknowledge
mailboxes for ownership handoff. This documented refinement replaces general
asynchronous descriptor FIFOs: the shallow bank rings already bound descriptor
capacity, and a mailbox directly represents each bank owner without FIFO pointer
constraints. Cross multi-bit descriptors atomically; never synchronize a
raw multi-bit frame counter or bank ID bit by bit. Descriptor FIFO implementation
depth constraints must not impose a power-of-two constraint on Q or bank counts.

Each channel lane stores `bank * Q + frame_in_quantum`. Audio accesses all lanes
for a frame; the core accesses the lane selected by the Input/Output job port.
Account for synchronous BRAM read latency and prefetch output before each frame
tick. Preserve an explicit same-clock configuration for simpler integration tests.
Select/reset CDC primitives and document constraints as part of the interface
contract; simulation alone does not establish metastability safety.

### Storage and ownership

Provide independent RX and TX rings, each with at least two quantum banks.
Start the recovery-capable demo with three RX banks and three TX banks, and test
the two-bank minimum. These capacities are independent of playback latency.

- RX: `FREE -> FILLING -> READY -> PROCESSING -> FREE`.
- TX: `FREE -> PROCESSING -> READY -> PLAYING -> FREE`.

Descriptors identify epoch, sequence, first audio frame, bank and shape. One
descriptor covers all channels. A bank changes owners only through an acknowledged
handoff. RX remains pinned through the core's last possible read; TX remains
pinned through its last possible write and, if played, its last playback read.
Define how expired TX results bypass playback and return safely to FREE.

Expose dedicated synchronous core sample read/write interfaces; latch bank IDs
with run metadata. Keep scratch storage local to the job processor. An output
bank must have defined zero samples for unwritten ports before publication.
The continuous path must not copy every quantum through an idle-only debug port.

At Q=32, 16+16 channels and three banks per direction, logical I/O sample storage
is 12 KiB; two banks require 8 KiB. Report physical BRAM packing separately.

### Scheduling and latency

Define RX quantum n as frames `[nQ, (n+1)Q)` relative to an epoch start. Its first
output sample is scheduled at frame `nQ + L`. For `2Q`, set `L=2Q`; for `Q+P`,
set `L=Q+P`. Start with P=Q as the conservative profile and select smaller P only
after establishing the complete bound for the chosen graph and clock profile.

The P budget includes input publication/CDC, command/control admission, core
execution, output publication/CDC and output-read prefetch plus margin. Bank
capacity does not determine L. Configuration changes occur while stopped/rearmed,
not halfway through a quantum. The I/O counters support arbitrary positive Q;
full-core profiles must additionally satisfy `Q % CHUNK_FRAMES == 0` and existing
supported numeric/profile constraints.

Execute complete RX quanta in sequence with no parallel state-mutating runs.
Reserve a TX bank before starting a run. Publish only after successful completion
of the entire coordinator, including logical clock advancement. The audio-domain
controller latches either the matching completed TX bank or silence once per
scheduled output quantum. Specify an exact eligibility cutoff before the first
sample, including how publication at that cutoff is classified; use it everywhere.

Emit startup silence until the first scheduled block without counting expected
priming as an underrun. Do not adapt latency to observed completion times. Keep
wall-clock audio frames distinct from processed logical frames during backlog.
Commands remain associated with their intended logical quantum; delayed runs
must not retimestamp actions or apply later controls to earlier queued input.
Provide bounded fabric-side command admission at quantum boundaries, preserving
the coordinator's existing serialization requirements.

### Overrun and recovery policy

For an output deadline miss, select silence for all Q frames and all channels.
Let the late run finish; preserve its recording/state effects and discard only
its output. Do not cancel it or advance its logical clock a second time. Catch up
by processing available RX blocks in order while later outputs retain their own
original deadlines. Buffer ownership remains valid during this backlog.

Retain enough queued input for bounded transient recovery. Do not claim that three
banks guarantee recovery from arbitrary stalls. Declare watchdog and queue bounds.
Input/FIFO loss, no safe capture bank, watchdog expiry or core/memory fault stops
new runs and enters recovery. Continue the audio tick and silence output; never
silently skip an input quantum and resume mutated logical state.

Provide `fault/recovery_requested`, reason, and an explicit reload/rearm handshake.
Quiesce or invalidate old memory traffic safely before storage reuse; tag I/O
descriptors with epochs and reject old completions. Keep sample/frame counting
running while recovery drains, resets and waits for reinitialization. The demo
controller reloads a known prepared program/session image and rearms on a defined
frame boundary, proving recovery without promising preservation of a damaged
session. A new epoch explicitly resets session logical time.

Expose saturating counters/status for processed quanta, played quanta, missed
deadlines, discarded late output, input loss, queue high-water marks, watchdogs,
core fault and epoch. Retain the first failure's sequence/frame and stage/code.

### Channel-to-job mapping

Use graph input/output ports 0..15 as the canonical I/O lane numbers. The demo
maps generated lane c to Input(c), the capture node for logical loop c, and its
own stable mono view. Playback of that view writes Output(c). Direct monitoring
routes Input(c) to Output(c). When monitoring and playback are combined, use
explicit graph Mix/Gain nodes in the established order, not multiple implicit
writers to an output port. Select the demo graph/control sequence accordingly.

Reuse BeginCapture for first take/retake and Capture/ReadView for media data.
No new job opcode is needed merely to identify an audio channel. Allocate state,
jobs, scratch buffers, views and media capacity from the compiled 16-channel
image, including the extra BeginCapture jobs on recording starts. Do not just
increase INPUT_PORTS/OUTPUT_PORTS while retaining undersized default capacities.
Multiple channels sharing a logical loop remain a graph concern, with no I/O
changes. Graph permutations, fan-out and explicit mixes must preserve this rule.

## Implementation stages

Execute stages in order; each stage's verification is required before its
milestone commit. Independent checks within a stage may run concurrently.

### Stage 1 — Freeze interfaces, timing and baseline

- [x] Create an implementation branch from the inspected reference branch;
  preserve unrelated work and record the exact base/tool versions.
- [x] Specify frame ports, core sample ports, descriptors, ownership handshakes,
  CDC/reset behavior, deadline cutoff, command admission and recovery/rearm API.
- [x] Define the initial Q=32/chunk=32, 16x16, three-bank demo capacities from a
  compiled graph; define two-bank and alternate-Q test profiles.
- [x] Record a targeted baseline for standalone audio, integrated loop/media,
  production/RTL comparison, wrapper generation and current 64-loop timing.
  Verify runner source manifests match the current metadata/shared-AXI sources;
  make necessary integration fixes rather than bypassing those components.
- [x] Verification: review a frame-by-frame schedule for both latency modes,
  one recoverable miss and one exhaustion/reset case; save reproducible baseline
  commands/results. No behavior implementation is required to freeze this contract.

### Stage 2 — Implement bank storage, clock crossing and frame transport

- [ ] Add `fpga/audio_io/` RTL, a runner and local protocol tests. Implement
  banked lane RAM, ownership descriptor transport, frame assembly/playout and
  priming using a stub quantum consumer solely for these local tests.
- [ ] Implement parameter checks, coherent reset/rearm and bank reuse rules.
- [ ] Verification: channel/frame patterns, wrap/reuse, two/three/more banks,
  Q=1 and odd Q in local tests, multiple unrelated clock ratios/phases, bounded
  descriptor stalls and reset during every ownership transition. Check dual-clock
  RAM inference and CDC structure; assert exclusive ownership and complete frames.

### Stage 3 — Attach real processing and migrate bench transport

- [ ] Replace/refactor job-processor I/O RAM access with the dedicated bank
  interfaces; propagate run-bank metadata and sample ports through audio/media/
  loop tops. Preserve scratch/debug observability needed by existing benches.
- [ ] Connect the autonomous quantum coordinator to real transition/state/job/
  media execution and successful whole-pipeline publication.
- [ ] Update standalone and integrated testbench drivers together with the port
  change. Supply a bench-side RAM/transport adapter where appropriate; existing
  noncontinuous benches may retain explicit run control. Their compatibility
  adapter must not become a production idle-port dependency.
- [ ] Regenerate AXI/native-memory wrappers using `generate_tops.py`; update all
  source manifests, synthesis scripts and explicit/implicit port connections.
- [ ] Verification: existing bit-exact audio/media/state results, zero unwritten
  outputs, sticky processing faults, partial-output suppression, stalls and reset;
  run generator `--check` and lint every affected standalone/integrated top.

### Stage 4 — Implement deterministic scheduling and recovery

- [ ] Implement fixed L scheduling, quantum publication/cutoff, expired-output
  reclamation, ordered catch-up and bounded command/control admission.
- [ ] Add diagnostics, queue/watchdog limits and memory-safe recovery/rearm.
- [ ] Verification: on-time, just-before/exactly-at/just-after cutoff; a single
  miss with uninterrupted recording; several misses followed by catch-up;
  two-bank pressure; input exhaustion; a hung run; core and late AXI faults;
  reset/reload with pending traffic and stale epoch descriptors. Prove all-channel
  whole-quantum silence, original-latency resumption and exactly-once advancement.

### Stage 5 — Add the 16-channel graph and synthesizable loopback demo

- [ ] Build/export prepared programs and routing metadata using existing graph
  compilation. Add 16 independent mono loops with stable per-loop views and
  distinct deterministic input patterns; implement a synthesizable generator,
  observable sink and prepared command/session loader.
- [ ] Exercise monitoring, record/stop/play/wrap, retakes, independent controls,
  and simultaneous recording on some lanes with playback on others. Use a
  bounded command sequence, not a second RTL graph compiler or sample oracle.
- [ ] Verify channel permutations, channel 15, unused outputs, fan-out and an
  explicit ordered mix. Keep channel maps and loop IDs outside I/O ownership logic.
- [ ] Verification: inspect emitted jobs and compare all recorded/playback sample
  words and loop states with the existing semantic oracles. Show continuous
  operation without per-quantum sample loading or host approval.

### Stage 6 — Extend continuous-audio verification and CI

- [ ] Add a continuous harness that advances audio independently of processing
  and uses the actual full AXI pipeline. Collect traces through nonintrusive
  observations; pause or drain only after the finite stimulus ends.
- [ ] Use production Q8.24 results for graph/audio/media semantics, existing
  reference comparisons for retained local contracts, and a small independent
  frame/deadline scoreboard for I/O scheduling. Neither Python nor new RTL may
  reimplement the processing algorithm as its expected-result oracle.
- [ ] Add Q=16/32/64 full-core builds with compatible chunk sizes, Q greater than
  one chunk, 2Q and valid Q+P profiles, two/three-bank configurations and clock
  ratios. Local odd-Q tests remain separate from production profile restrictions.
- [ ] Exercise instant/fixed/stalled memory, AXI32/64 and selected DRAMsim3
  record/playback/overrun cases. Include deterministic random seeds and replay
  artifacts identifying epoch, quantum, channel, frame, cutoff and failure cause.
- [ ] Update `.github/workflows/fpga_simulation.yml` filters/jobs for AUDIO_IO,
  new RTL and both relevant testbench paths. Keep fast deterministic coverage in
  each PR and a bounded representative DRAMsim3 campaign; retain existing gates.
- [ ] Verification: run the new CI commands locally where supported and validate
  artifacts and failure reporting with deliberately injected deadline/input faults.

### Stage 7 — Final end-to-end validation and delivery

- [ ] Run the 16x16 demo through both latency modes with monitoring and complete
  record/playback sessions. Verify every output and recorded word, expected
  startup silence and exact latency from the declared schedule, without aligning
  traces after observing results or dropping prefixes/tails.
- [ ] Repeat with a recoverable processing overrun and with exhaustion followed
  by quiesce/reset/reload/rearm. Demonstrate that output returns at the configured
  latency and that no old bank or memory response crosses epochs.
- [ ] Run targeted Rust/contract suites, all affected FPGA regressions, production
  comparisons and the existing 64-loop scale/timing workload. Complete repository
  mandated formatting, warning-denying builds and test gates for changed code.
- [ ] Lint and synthesize the full 16x16 top and bank/quantum variants; report
  BRAM/LUT/FF/DSP use and measured complete input-publication-to-output-eligibility
  cycles. Explain the selected P and clock assumptions. Synthesis/cycle simulation
  is evidence for the design, not a claim of routed board timing closure.
- [ ] Document ports, channel mapping, parameters, clock/reset constraints,
  recovery/rearm ownership and exact replay commands in `fpga/audio_io/README.md`.
- [ ] Push the implementation branch and create a PR targeting
  `proto/fpga-rust-reference`. Monitor CI to green; inspect automated reviews,
  address applicable feedback, rerun affected checks and repeat until automated
  approval or documented absence of an approving reviewer. Do not merge the PR
  without explicit user authorization.

## Testing cost and confidence

During development, run the smallest relevant local protocol or semantic cases
after each change. Cover deadline/ownership edge cases exhaustively in small
profiles, then use representative integrated cases to verify real processing.
Run the broader affected regression matrix at integration milestones, not after
every edit. Reserve long seeded sessions, DRAMsim3 campaigns, full synthesis and
the 64-loop benchmark for the integrated/final gates and repeat them when changes
affect their conclusions. Every stage records its commands, seeds and outcomes.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution evidence

### Stage 1

- Branch: `feat/fpga-audio-io`, base `012ed6bd`. Rust 1.99.1, Verilator
  5.032, Yosys 0.52, cocotb 1.9.2, Python 3.12. Tools are workspace-local.
- Frozen ports/timeline/reset/ownership contract: `fpga/audio_io/README.md`.
- Baseline Rust suites for `rt_fpga_reference`, `rt_fpga_testbench` and
  `rt_backend_testbench` pass. Standalone default/small audio cases pass with
  one seeded case; integrated small loop/media cases pass with one seeded case.
- The production runner had an outdated source manifest. Added metadata-cache
  and shared-AXI sources; its mono record/stop/play comparison then passes.
- Generated wrappers pass drift checking; the existing 64-loop instant-memory
  timing fixture passes (four complete Rust-matched quanta).
- Baseline logs/artifacts: `work/baseline-*.log`, `work/audio-baseline/`.
