# Loop execution semantics

## Status and authority

Contract for `PLAN_LOOP_LAYER.md`, specified in Stage 1. Stage 2 implements the pure primitive/composite resolver; runtime/resource/media/host integration remains pending. This is not a claim about capabilities of the existing backend. The governing decision is `INTER_LOOP_PROCESSING.md`. Full legacy compatibility is not a goal. Changing these design choices requires recorded reasoning; changing the plan's goals or LL-A acceptance criteria requires user approval.

The resolver owns musical state. Media renderers own cursors, MIDI state, smoothing state, and capture execution, but cannot independently decide logical mode, position, or synchronization. Absolute engine frames are checked `u64` values; node/loop/program/resource identities are generation checked. Sample-rate changes require stopping and re-preparing. No resampling is implicit.

## Time and intervals

- Audio intervals are half-open `[start, end)`. A transition at F affects sample F and MIDI events timestamped F, not sample F−1.
- Boundary settlement occurs immediately before rendering samples starting at that boundary. An event at a host callback's end belongs to the next nonempty callback. Interval completion reports observations and advances positions, but does not independently settle the following boundary.
- Zero-length callbacks do nothing. At the final rendered end, a loop may have position equal to length with a wrap pending; state publications identify the cut as `AfterInterval` rather than pretending that the next sample's boundary has already executed.
- Each nonempty interval ends at the earliest external scheduled action, musical boundary, resource preflight boundary, or callback end. Additional host cuts do not create musical occurrences.
- Inputs admitted before their deadlines are compared by absolute frame, not by which callback admitted them. Late external requests retain requested/effective/actual frames; the runtime's explicit late-admission policy is preserved. Late arrival is not a valid partition-invariance comparison.

## Primitive state and operations

A primitive has mode `Stopped`, `Playing`, or `Recording`; logical length; position; playback cycle count; prepared content identity; optional sync source; and pending requests. It starts stopped with position/cycles zero; without prepared initial content its length is zero as well.

| Operation | Meaning at the winning transition boundary |
| --- | --- |
| Start playing | Select prepared playable content, set the requested position (default zero), and enter Playing. Starting an already-playing loop without `retrigger` is an idempotent mode request; it does not reset its position or generate another start pulse. |
| Retrigger | Restart Playing at the specified valid position, even if already playing. It generates a start pulse. |
| Stop | Freeze an active capture at the common prefix end if needed; enter Stopped and set position zero. Cancel that target's older pending transitions as specified below. No subsequent media samples are played or captured. |
| Begin recording | Activate an already prepared capture group; retain any previous playable content through its lease. Enter Recording with a fresh take length zero. An explicit `monitor_previous` option may continue the retained take's playback while capturing a distinct new revision. No callback allocation or clearing an arbitrarily large old recording is allowed. |
| Finish recording and play | Freeze all capture channels at F and make the resulting group playable before rendering F. The captured interval excludes F; playback begins at its chosen position at F. Lifecycle/index/lease capacity must have been reserved before recording was accepted. |
| Seek | For Playing, position must satisfy `0 <= position < length`; retain mode and content, perform MIDI cleanup/restoration and audio discontinuity handling. Seek does not imply a trigger unless `retrigger` is requested. Reject seek during Recording or Stopped without changing state. |
| Activate prepared content | Atomically select a complete prepared channel group/revision at F. The request specifies reset position or preservation; preservation must fit the new length. Invalid/stale candidates leave previous content and state intact. Old leases remain valid. |

A playable zero-length primitive stays stopped: an attempted start has the explicit `EmptyContent` outcome, rather than entering a zero-duration wrap loop. Finishing an empty active capture into playback fails this precondition transactionally: the capture continues, without freezing or fabricating content; an explicit Stop can instead finish an empty take. An empty composite is a successful stopped no-op. Recording grows length by the successfully captured duration; a failed take never manufactures padding or resumes after a gap. A fresh start/retrigger resets playback cycle count; an ordinary wrap increments it once. Forwarded synchronization pulses do not increment media playback cycle count or reset an already-running playhead.

Playback wraps at its own logical length. A sync source supplies clock pulses for quantized requests and composites; it does not silently stretch media, change playback rate, or force the playhead to restart at every source pulse. Requests requiring alignment carry an explicit position/retrigger decision.

Beginning a fresh recording emits a start clock edge, but recording alone creates no periodic wrap clock. Logical capture revision serials do not reuse a previously reached serial, even after selecting an older prepared revision.

Recording normally has no playback output. With `monitor_previous`, the resolver additionally publishes the retained take's playback position/length and the new take's capture-prefix position; all playback channels share the former and all capture channels share the latter. They advance from the same absolute interval, not independent renderer clocks. Existing playback continues from its current position (or zero when starting from stopped), can feed FX, and can be captured into the new revision without overwriting its source. Its wraps still emit clock edges. Freeze-and-play explicitly selects the new take and resets its playhead/cycle count. If capture exhausts while monitoring, keep the old playback running and mark the new take incomplete; otherwise enter Stopped. This is separate media execution, not in-place overdub or full application dry/wet policy.

### Delayed requests and reliable outcomes

`At(F)` requests become eligible at their effective absolute frame. `AfterTicks { source, skip }` requests are armed at their effective frame and become eligible on the `(skip + 1)`th source pulse at or after that frame. Multiple pulses coincident at one sample are one clock edge, as defined below. An idle but valid source leaves the request pending; no UI round trip or timeout invents a source pulse. An invalid/stale source is an explicit failure, not an unsynchronized fallback. An armed request's source must match its target's single prepared sync source; admitting a request cannot introduce an unvalidated timing edge. Topology replacement must revalidate or explicitly fail affected pending requests. An unsynchronized immediate request is expressed as `At`, not a missing-source delayed request.

The application sees distinct phases:

1. Transport submission/ownership acceptance.
2. Generic runtime delivery/admission result, including lateness if applicable.
3. Musical pending state, if waiting for a source boundary.
4. Exactly one reliable musical terminal outcome: `Applied { frame }`, `Superseded { frame }`, `Cancelled { frame }`, or `Failed { frame, reason }`.

An external request reserves its terminal record and pending storage before it becomes a promise. Automatic wraps/composite actions do not allocate external completion records. Their state and diagnostic journal are separately observable. Unread terminal records retain their credits.

A ticket cancels only before the runtime observes dequeue, as today. After admission, a typed `Cancel(request_id)` resolves a still-pending musical request; cancelling one already terminal reports that fact without undoing execution. At a boundary, cancellations/direct stops are processed before eligible older pending actions; a direct stop cancels older pending requests for that target, while later-sequence requests may explicitly arm a new transition. Shutdown abandonment is reported as such when orderly draining is possible; endpoint destruction after callbacks stop is not a fabricated successful execution.

## Coincident decisions, pulses, and termination

### Desired state versus clock occurrences

Mode/position/content requests are desired-state intents, not a demand to render an intermediate zero-duration sound. There are two ordered phases at a boundary: already-eligible decisions, then decisions made eligible by that boundary's clock edge. Select one complete winning transition per target in each phase. Losing external contenders receive `Superseded`, not a false `Applied`. If a caller needs a compound change, it submits one transition containing the desired mode/position/content; it must not rely on intermediate effects from competing requests in the same phase.

A clock-reactive transition may follow an earlier applied transition at the same sample: for example, a program starts A, creating the clock edge that makes a pending Stop(A) eligible. Both causal steps are real and journaled at F, but no samples render between them. The later decision does not retract the edge or rewrite an earlier Applied outcome. Carry the earlier winner's precedence into the reactive phase so a lower-priority reactive intent cannot override a protected higher-priority direct decision.

Order intents by this total precedence:

1. Direct external transitions, including eligible previously armed requests: highest acceptance sequence wins.
2. Explicit-mode composite/trigger actions.
3. Regular-composite inherited playback actions.
4. Natural continuation/wrap behavior.

Within a non-direct class, lower stable source identity wins; then higher compiled action ordinal wins. Duplicate ordinals within a source are invalid. Source identity includes generation. The compiler normalizes adjacent entries that continue the same target occurrence/mode: no redundant stop/start at a contiguous boundary. No result depends on declaration order or signal traversal order.

A Stop already eligible before clock resolution prevents that target's coincident natural/start pulse and blocks forwarding through it for this boundary. A winning explicit restart establishes the new position instead of also applying the old wrap. A stop that becomes eligible *because of* a source pulse does not retract that already-occurring source clock edge. A previously stopped loop can otherwise relay its source's clock; forwarding alone does not start media playback.

A **clock pulse** is identified by `(absolute_frame, logical_loop_identity)`. At most one clock edge exists for a given logical loop/sample, whether it is caused by a wrap, a start, or forwarding of coincident source pulses. This is clock-edge coalescing semantics, not a callback CPU allowance: every eligible request/action still participates in arbitration and obtains its required outcome. MIDI events are not coalesced by this rule.

### Separate relation types

Prepared topology distinguishes:

- `Sync(source, follower)`: forwards a clock edge and makes source-delayed actions eligible; it does not itself impose a mode change.
- `OnPulse(source, target, transition)`: a declared state-changing action.
- Composite parent-to-child action dependencies, including every possible target of its prepared program.
- Signal edges and generic planner/view bindings, which are separate runtime structures.

The baseline permits **pure synchronization cycles** but rejects instantaneous state-changing feedback. Compilation computes strongly connected components of the union of sync/action dependencies. A cyclic component is valid only when all its internal edges are `Sync`. An action edge inside such a component, a self-action dependency, or recursive composite containment is rejected as `InstantaneousDependencyCycle`. This is a deliberate constrained program language, not a universal decision procedure for arbitrary callbacks. Even state-dependent action feedback that might happen to terminate in one state is unsupported; it cannot be smuggled into the model through custom event callbacks.

Collapse valid sync components to a DAG and compile a stable component order. All sources of a target's state-changing intents then precede its component. A pure sync component may contain primitives and composites, but its composites/actions can target only downstream components.

### Boundary algorithm and progress argument

At F:

1. Gather the finite batch of externally delivered operations, pending cancellations, and any observations completed before F. Do not poll external producers again during settlement.
2. Walk prepared components in topological order. All incoming state-changing intents and upstream pulses for a component are now known.
3. Resolve already-eligible direct/incoming intents and identify surviving natural/start pulses. Propagate seeds through the component's Sync edges using one visited mark per member, respecting pre-clock Stop masks. An unseeded component creates no pulse.
4. Make source-delayed requests eligible once per source clock edge. All of a target's due requests use its single prepared source, so arbitrate them together when that source edge arrives. Forward the edge where not pre-clock masked, then apply the reactive winner. A higher-priority reactive start may lift an earlier Stop mask and seed that member's first pulse; a reactive stop cannot retract a delivered edge. Continue the finite clock worklist until no newly seeded member remains. No second pulse is invented, and no request becomes eligible twice. Since only pure sync edges are internal, no newly generated action can invalidate an earlier component's decision.
5. Advance each activated/running composite's tick cursor once for its clock edge and stream its due compiled actions to downstream components. Starting/seeking a composite emits the selected iteration's authoritative state; its own start pulse must not also advance it past that iteration.
6. Commit the component's final state, request outcomes, and downstream intents. After all components settle, preflight resources and publish the next interval.

A pending request armed against an internal source cannot generate the component's first pulse merely by waiting for that pulse. Start seeds come from immediate winning intents, natural boundaries, or already seeded upstream clocks. This prevents circular self-justification in an idle mutual-sync group.

Termination follows from finite prepared components/edges/program actions/pending requests, acyclic inter-component action order, and one delivery of each loop's clock edge at F. A request can become eligible once at F; each compiled source action due at the selected iteration is considered once; all scratch sizes are derived during preparation. There is no wave counter. Rendered intervals advance by at least one sample toward a finite callback end, so one-sample loops may process every sample without a subdivision cutoff.

The boundary transaction uses prepared shadow state and outcome slots. Validate targets and resource preconditions before committing dependent state. If preflight fails, discard the preview, release provisional reservations, and preview again from the same committed state/inputs with that failure latched. Each retry must add a previously unlatched capture-group/activation failure; therefore the retry count is derived from finite prepared groups, not a configurable work allowance. Never deliver a pulse, consume a pending request, publish a result, or advance a generation twice because preview was repeated. An authoritative composite snapshot carries its originating external request through nested snapshots for atomic preflight refusal. Autonomous tick and OnPulse actions are not falsely attributed to an unrelated coincident request; `Observation::CaptureUnavailable { target, reason }` can refuse their primitive capture starts/restarts without an external receipt. It leaves the target's prior mode, capture/content revisions and capture length intact, emits no start pulse, and does not block Play/Stop/Seek. An ongoing take's append exhaustion remains a separate `CaptureExhausted` input; both may be latched for the same primitive if a refused restart falls back to an exhausted old take. Use `Refused { request, reason }` when the entire originating request family must fail; this takes precedence over target refusal regardless of observation order. Target refusal alone does not reject a parent composite or siblings. The fixed input bound accommodates both per-loop failure classes plus retained request refusals. The coordinated facade now wires capture preflight outcomes to these inputs; the integrated cascade/nested scenario also verifies exact exhaustion and autonomous-start refusal. Fixed-graph owning program activation now uses the same preflight; general graph/role transactions remain separate integration work. Resource failures become boundary inputs and affected targets' failure decisions, not a late rollback of rendered samples. The exact reservation lifecycle is specified in `RUNTIME_INTEGRATION.md`.

## Prepared composites

A prepared program has stable target identities, a positive nominal sync-unit frame length, sequential sections containing parallel entries, nonnegative delays, positive durations/repetition counts, and optional explicit modes. All sample/tick/offset arithmetic is checked. Empty timelines remain stopped. Reject zero-duration entries, unknown modes, mixed implicit/explicit mode styles within one program, stale targets, unsupported modes, recursive containment, and insufficient prepared descriptor/index storage.

- Regular programs use inherited Playing behavior and repeat their finite pass. Empty playback children reserve duration but stay stopped. They do not implicitly record empty children.
- Explicit-mode programs use supported Stopped/Playing/Recording transitions and stop after one finite pass. Recording children require prepared capture reservations; finish-to-play is explicit in the program. Repetition does not imply unlimited retained takes or recording storage.
- Nested programs use the same resolver, conflict rules, and prepared dependency graph. Their iteration-zero actions execute at the parent's activation frame, without a block or propagation-hop delay.
- Actual delivered clock edges drive iteration advancement. A stopped source with no incoming clock stalls that advancement; a stopped relay can still forward its own source's edges. A composite without a sync source stays at its selected iteration after start; nominal units do not supply a fallback timer. The nominal sync-unit length is preparation metadata for derived entry durations and seek offsets, not an instruction to invent pulses on a fixed interaction grid. Derived duration is `max(1, ceil(child_length / nominal_unit))`; explicit durations override it. Changes to that metadata require a prepared program replacement.
- Composite state reports iteration, pass count, source identity, last actual clock frame, iteration-entry frame, and nominal length separately. Elapsed time is computed from the requested publication frame and the appropriate stored frame; seek entry and actual clock delivery are not conflated. Do not describe a stalled or irregular source as a uniformly advancing sample position.
- Start/seek establishes an authoritative desired-state snapshot for the program's declared targets: stop no-longer-desired descendants, then apply selected target states through ordinary arbitration. Seeking to iteration k uses prepared lookup information, not replay from iteration zero. `PlayIteration` starts/retriggers at a selected iteration; `SeekIteration` requires Playing and retains the pass count unless retriggering. A non-retriggering seek repositions already-playing descendants without manufacturing clock edges; descendants newly entering playback/recording retain their normal start edge. Playback offsets are derived from the selected occurrence's elapsed nominal units modulo nonzero child length. Seeking into a recording occurrence starts a new explicitly prepared capture at the seek frame; it does not fabricate historical samples. If no reservation is ready, reject the seek atomically.
- Stopped/pending program replacement may activate at the requested boundary. A running program stages a replacement for its next iteration-zero boundary; explicitly stop/seek/restart if another cut is desired. Recheck target generations before activation. Preserve defined contiguous child continuations; never address a replacement object through an old slot generation.

The standalone owning `stage_program` path now implements this cut for one changed
program with fixed identities/sync/triggers/other programs. Configuration selection
precedes the real musical preview, using the old unpublished cut without emitting
its zero actions. A higher-sequence Stop cancels an older pending configuration
request; a newer configuration may install at the stopped cut. A same-frame
seek/restart to zero can select the candidate, while ordinary musical arbitration
still determines the final state. An explicit one-pass program reaching its end
remains stopped after installation unless a winning explicit restart says otherwise;
installation itself is not another Play. Removed active targets stop through normal
arbitration, and authoritative seek/restart/stop cuts also stop removed declared
targets. Candidate direct/nested actions carry the configuration request origin for
atomic family preflight refusal; autonomous OnPulse actions retain their usual
attribution. The `Transition::Program` receipt/journal tag is not directly submittable
without its owner. See the [owning program guide](../../src/rust/shoop_loop_engine/docs/program-changes.md).
The fixed-graph facade's `ProgramActivate` now connects prepared receiver owners,
real capture-bank preflight, runtime execution receipts and graph/topology epoch
checks to this transaction. General simultaneous graph/role-generation changes
remain Stage6 work. Final quiescence is distinct from musical Stop: it permanently
prevents execution and permits owner recovery after a render fault without
advancing time or rolling back a published boundary.

## Media behavior

### Audio

Resource audio is interleaved little-endian `f32`; callback ports are planar. Conversion uses prepared scratch. Logical group duration is common to its audio/MIDI channels. Shorter imported channel data is explicitly zero-padded to the declared group duration during preparation, not guessed during rendering; reject incompatible metadata if padding was not requested.

The resolver publishes an authoritative `Decision::playback_discontinuity` for the final interval, independently of diagnostic journal capacity. Successful start/retrigger/seek/replacement and natural wrap set it when playback survives final arbitration. A same-position seek or identical replacement is still an explicit discontinuity even if visible state is unchanged. Starting monitored playback sets it; continuing monitored playback, idempotent Play, or monitored-capture failure with old playback continuing does not. Final non-playing states clear it; their consumers stop/clean up. The flag is rebuilt on every preview and committed with the decision, so failed/retried/losing/cancelled intents cannot leak a DSP restart. This makes the existing media behavior executable without relying on optional diagnostics; it does not add a timing authority or change request/pulse precedence.

Gain applies per sample. Default smoothing is `None`, making fixture comparisons exact. Optional `LinearRestart { frames: N }` on start/retrigger/seek/wrap/content replacement interpolates from the previous output value toward each new raw gained sample with weights `(i+1)/N`, for N samples. Continue the ramp across host cuts; clamp it to a shorter next discontinuity by restarting from the last actual output. Stop is immediate silence at F, not an undocumented release tail. N=0 is equivalent to None. No smoothing algorithm may introduce an additional signal block.

### MIDI

Store complete MIDI 1.0 packets with resource-relative timestamps and explicit duration, including trailing silence. A packet at the capture end belongs to subsequent execution, not the frozen prefix. Preserve equal-time packet order. Treat note-on velocity zero as note-off for state tracking while preserving packet bytes.

Maintain prepared channel state for notes/velocities, sustain, controllers, program, pitch bend, and supported channel/poly pressure. Capture start records an initial state snapshot from the input tracker; this is not prerecording of past events. An exported MIDI take includes that initial state and its timestamped event stream. Input tracking continues while stopped when the corresponding capture/input operation remains connected.

At stop/seek/retrigger/wrap/content replacement, clean up the loop's sounding notes and sustain state deterministically, restore the selected content's state *before* the target offset, then deliver events exactly at that offset. Restore held notes with their stored velocities. Pedal-held released keys retain their projected velocity without being falsely restored as held; sostenuto restoration latches only the recorded subset. Bank selectors at the last program change and later pending selectors are distinct. Restore the last selected RPN/NRPN family after temporarily nulling selectors for cached Data Entry values; relative parameter commands are not persistent state. The fixed projection does not reconstruct synthesizer voices, vendor SysEx or parameter-value histories, and restoration explicitly reports observed untracked parameter history rather than claiming a complete device snapshot. Original recorded packets are preserved. Prepared checkpoints plus bounded local event ranges provide seek/state reconstruction; no scan from the beginning of a long take. Resource/index preparation accounts for the maximum checkpoint replay region.

Loop-owned note cleanup does not imply ownership of unrelated voices sharing a physical MIDI channel. Hosts must route/channelize independently controlled parts appropriately; MIDI 1.0 cannot distinguish two owners of the same key/channel. Runtime overflow recovery remains CC120/CC123 with reserved delivery. A refused packet reports overflow, advances logical/media processing as specified, and invokes recovery; it must not abandon unrelated audio. Sufficient prepared output capacity is a precondition for lossless sample/event comparisons.

## Normative boundary examples

Names below are test obligations for the independent reference model and integrated suites. These are expected results, not claims of implementation.

| Case | Inputs | Expected semantic result |
| --- | --- | --- |
| `record_to_play` | Record A at 0; finish/play at 5 | Capture `[0,5)`; first playback sample at 5 is captured sample 0; no silent block |
| `cascade` | A length 4 wraps at 4; OnPulse A starts B; OnPulse B starts C | A wrap, B start, C start all at 4; all post-boundary decisions visible before sample 4 |
| `callback_end` | Same cascade, render partitions `[4,4]` versus `[8]` | Same frame-4 transitions; no duplicate settlement on the second callback |
| `dense` | A length 1, render 128 frames, already playing at 0 | 127 interior wraps at 1…127; wrap at 128 belongs to the next nonempty callback; no subrange-budget failure |
| `mutual_sync` | Sync A→B and B→A; A produces a pulse at 7 | One clock edge for each at 7; each eligible countdown consumes one edge; no artificial playback-cycle increment in B |
| `idle_mutual_sync` | Same relation, no natural/start/upstream seed | No pulse; delayed requests remain pending; no spin |
| `instantaneous_feedback` | OnPulse A restarts B and OnPulse B restarts A | Reject prepared topology before activation; keep prior topology |
| `stop_suppresses_wrap` | A wraps at 4 but winning direct Stop(A) is at 4 | A stops at 4; no A wrap/start pulse at 4; downstream OnPulse actions do not fire |
| `direct_conflict` | Two already-eligible direct transitions to A at 4, sequence 20 then 21 | Sequence 21 wins; 20 receives Superseded; no intermediate start pulse from losing intent |
| `reactive_stop` | A winning program start seeds A's clock at 4; an armed higher-priority Stop(A) becomes eligible from that edge | Start and reactive stop are ordered at 4; the clock is delivered once and is not retracted; final sample-4 decision is Stopped |
| `delayed` | Arm skip=1 at 0; source pulses at 4, 8 | Pending through 4; apply at 8, regardless of callback cuts |
| `nested` | C starts at 10; C's iteration-zero action starts D; D starts primitive A | C, D and A activate at 10 in dependency order |
| `capture_failure` | Group captured through 5; preflight for next interval fails | Common prefix remains `[start,5)` and incomplete; resolver failure decision at 5 precedes dependent samples; other loops continue |
| `source_stall` | Composite at iteration 1; no subsequent source pulse | No invented iteration advancement; report last tick/elapsed time separately |
| `seek_midi` | Note-on before seek offset, note-off after it | Cleanup old state, restore held note at seek frame, later emit recorded note-off at its rebased frame |

## Legacy behavior mapping

| Existing evidence | Treatment |
| --- | --- |
| `tests/basic_loop.rs` stop/record/quantized-delay/cancellation/zero-length cases | Reexpress in typed requests and reliable outcomes; explicitly test the new absence-versus-idle source rules |
| `audio_midi_loop_audio.rs` recording, playback, wrap, and channel coordination | Reuse scalar media expectations for included modes; no dependency on old channel/session classes |
| `audio_midi_loop_midi.rs` playback, recording, CC state, pitch wheel, boundary notes | Reexpress using initial-state/checkpoint semantics; document expected differences where old prerecord/preplay was involved |
| `composite_state_machine.rs` compilation, stable IDs, authoritative start/seek, regular/script, nesting, replacement | Reuse included behavior as reference cases; adapt excluded modes and new typed resource preparation |
| `composite_timeline.rs` precedence, source-stop suppression, same-frame nesting, declaration-order independence | Retain these properties under the explicit component/pulse model |
| `composite_timing.rs` exact first sample, partition invariance, repeated source wraps | Retain as new resolver/media conformance requirements |
| Old event-wave/sub-block overflow tests and rejection of all cyclic sync graphs | Superseded: test dense valid work completing and pure-sync cycles terminating instead |
| Old in-place replacing, prerecord/preplay, dry/wet application modes and Session integration | Outside this milestone; do not silently mark them implemented |
