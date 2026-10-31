# Loop-library foundations: complete guide backfill through Stage6 implementation

These are instructions for the separate documentation worker, not replacement
user guides. Reviewed every current loop crate guide, README and coordinated_loop
example, plus repository SEMANTICS, RUNTIME_INTEGRATION and FACADE_DESIGN. Baselines
and fresh example/rustdoc checks are in `loop-guides-baseline.json`. Read destination
drift again before applying. Preserve historical measurements and archived guides.
Stage6 verification, independent hosts, Stage8 evidence and publication remain open.

## Destination coverage and exact stale statements

| Destination under src/rust/shoop_loop_engine | Required correction / preservation |
| --- | --- |
| README.md | Replace both blanket general graph/role transactions pending statements and the initial facade limitation with the tested prepared-template configuration/program-graph paths. Keep independent hosts and final acceptance pending. Link the existing media/renderer guides and the current activation documentation, not implementation logs as a substitute for a walkthrough. |
| docs/audio.md | Remove claims that the runtime adapter, separate-node graph, coherent media export and public lifecycle example remain unimplemented. The low-level operation still has no timing/lease/lifecycle authority; the facade supplies it. Preserve all DSP/range/zero-frame/refusal contracts and historical six-test evidence as historical. |
| docs/midi-restore.md | Replace complete loop adapter pending with a facade link. Preserve channel/key projection limits, protocol ordering, first-sink-failure accounting and cleanup/restore capacity bounds; actual independent loop AudioWorklet remains pending. |
| docs/midi.md | Replace full loop binding/state/export/FX example pending and Stage5-not-complete statements. The focused generic runtime proof is still only that proof; newer facade/integrated evidence must be attributed separately. Retain the stack-overflow history, fixed-box remedy and absence of captured exact stack frame. |
| docs/media.md | Replace planned/future-facade wording with the integrated facade link. The ledger itself still cannot certify a settled musical cut, execute transitions or acquire a coherent active prefix. Add owner-activation permit coverage from loop-media-owner-activation.md; never transfer facade authority to plain IDs/views. |
| docs/topology.md | Keep all flat-array/view/capacity contracts. Replace the final claim that zero-cut running changes and all graph/role-generation transactions remain Stage6 implementation work; distinguish immediate replace_topology from stage_program and prepared facade configuration. |
| docs/topology-staging.md | Keep LTP1 schema, lifecycle, bounded validation and full-capacity recycling rules. Replace the final blanket missing running-program/configuration integration statement. Ready still certifies timing IR only; it is neither graph activation nor a complete worklet protocol. |
| docs/program-boundaries.md | Keep probe nonpublication/invalidated-preview semantics. Update final facade/lifecycle pending statements using program-change, paired-program/graph and nested-lifecycle notes. A probe still owns/reserves nothing and is not activation permission. |
| docs/program-changes.md | Keep standalone single-program scope, one retained owner, cut selection, family refusal and quiescence. Replace general graph/role transactions pending with separately attributed facade implementation; independent hosts and broader final stress remain pending. Do not broaden standalone stage_program to change logical identities or roles. |
| docs/topology-activation.md | Preserve the fixed-graph APIs as a documented subset, but remove the claim that Controller cannot change signal routes. Add/link GraphActivate, TopologyGraphActivate, ConfigurationActivate and ProgramGraphActivate with their different preconditions and retirement paths. Use the four existing milestone notes, not a blanket unrestricted replacement claim. |
| docs/runtime.md | Correct current constructor/route/recovery surface, retain startup/capture/state/export/quiescence semantics, and incorporate the user walkthrough requirements below and in loop-runtime-backfill.md. |

These eleven destination rows are source-backed review findings. Historical totals
inside guides should either remain explicitly dated or move to linked operational
evidence, not be silently replaced with current counts. A full Miri inventory or
running interpreter job is not a pass. Existing generic host regressions are not an
independent loop host. No change to included/excluded musical scope is proposed.

The crate introduction in src/lib.rs was also read: its portable single-owner,
prepared-render-state and host/device-exclusion wording remains accurate. Preserve
it; adding navigation to the eventual published guides is an optional usability
improvement, not a needed semantic correction. Public IDs in src/ids.rs confirm
LoopId/RequestId are slot/generation values, unlike the separately scoped staging
and media tokens; the companion note makes caller routing explicit.

## Preparation walkthrough missing from the current user path

Keep the public coordinated_loop example as the starting task. Walk through small
Limits/Config values, one primitive, distinct playback/capture nodes, a plain FX
node and a complete signal DAG. Explain that compile allocates off-render and
compiles timing relations/programs; it does not compile the signal graph. The
facade compiles/validates signal routes and decision bindings before returning at
frame0/graph epoch1. No warm-up callback or initial command/result credit is needed.
Preparation failure consumes supplied owners off-render; do not promise recovery
of a partially constructed live engine.

Prepare dimensions and resource pools before rendering: audio channels, max frames,
MIDI event/byte/packet bounds, indexed versions/checkpoints, two capture groups per
media-bearing primitive, staging bytes and final snapshot leases. Explain media,
pin/export and normal/recovery request pools separately. normal_requests is positive
and leaves one extra resolver slot for recovery. Disabled audio/MIDI yields a valid
timing-only primitive, not a missing required source for nonempty enabled media.
Composites have no media binding. Sample-rate or physical-shape changes require
stopping/repreparation; runtime role-generation edits select fixed prepared templates.

Imports transfer existing leases, validate common duration/channels/scoped indexes,
and reserve every imported take revision in the resolver watermark—even revisions
not initially selected. Revision0 initial content must match the prepared topology.
No renderer silently pads short imports or guesses mismatched metadata. If a host
wants padded channels, it must explicitly materialize valid common-duration content
off-render; the facade does not expose an automatic padding option.

In the current example, max_frames8/stereo means64 raw audio bytes per full interval.
Its MIDI bound128events/512bytes needs2048 encoded staging bytes (12bytes header per
event plus payload);4096group staging bytes cover both, with page/index/final-lease
costs separate. MIDI replay128/checkpoints4 yields511packet capacity per indexed
version, not512. This is illustrative prepared sizing, not a performance promise.
Verify these formulas against the final example before publication.

## Commands and composites need an example-led user section

Use the actual command names: Timing::At with Request.at or Controller's at argument;
AfterTicks { source, skip }; Play/Seek with position and retrigger; Record with
monitor_previous; Stop; PlayIteration/SeekIteration for composites. Timing::At has
no embedded Frame field. Facade Select derives length/revision from a pinned MediaId;
raw Replace is only a standalone resolver operation. Transition::Program is an owning
transaction's receipt/journal tag, not a directly submittable command.

Build a small independently predicted sync example: source pulses at4 and8, target
armed at0 with skip1, so it stays pending through4 and applies at8. An idle valid
source does not timeout or become unsynchronized. A missing/stale/wrong prepared
source is a refusal. Keep admission, delivered, effective and terminal frames distinct;
late delivery is not comparable to on-time partition-invariant input. Match reliable
results by sequence; optional/truncated journals cannot drive control or media state.

Add a nested regular/explicit example with a visible initial delay and seek into a
recording span. Regular programs repeat; explicit-mode programs perform one pass.
Nominal units provide duration/seek arithmetic, not an autonomous clock. A stopped
relay may forward a real source pulse without media playback; an unseeded sync cycle
produces none. Empty playback children stay stopped and never implicitly record.
Composite record mode itself is unsupported. Explicit/implicit styles cannot mix
inside one program, recursive programs/action feedback are refused, and pure-sync
cycles remain valid under the finite one-edge-per-loop/sample rule.

Show highest direct sequence overriding a nested action, and distinguish pre-clock
Stop suppressing a pulse from reactive Stop that cannot retract an already delivered
pulse. Losing requests get Superseded, not Applied. A non-retriggering seek into an
already-playing descendant repositions without inventing another clock; a new
playback/recording start still has its real start edge. Seeking into recording starts
a fresh reserved take at that frame, never fabricated historical samples. Failure
of an authoritative request family preserves the old configuration and take revision.

Use existing independent resolver/composite/interloop references to predict samples,
absolute MIDI, public states, journals and outcomes. Compare whole/single/irregular
partitions, declaration reversal and journal0/1/large; include a zero callback at a
cut and leave callback-end settlement for the next nonempty callback. Do not copy a
resolver's output as the oracle. Keep the original137-page exhaustion scenario and
the new256-page sufficient-capacity nested scenario separately attributed.

## Renderer/ledger contracts to preserve at point of use

- Audio: scoped ReadIndex/actual pinned metadata, interleaved LEf32 to planar windows,
  finite gain including negative, no-source immediate silence, zero-frame no-op,
  authoritative discontinuity and partition-independent LinearRestart. Gain alone
  does not restart smoothing. Stop has no invented release tail. Render intervals
  cannot cross a musical wrap; the resolver supplies the cut/position.
- MIDI: cleanup, strict-preoffset projected restoration, then originals at the offset;
  preserve complete SysEx/equal-time bytes/trailing silence. Initial capture state is
  the input projection strictly before F, not prerecord event history. Tracking runs
  while stopped if the input consumer is connected. Input reset/preserve on role reuse
  is explicit and must not be applied repeatedly after a successful finish.
- Restoration: loaded program bank differs from pending selectors; CC96/97 are not
  cached persistent values. RPN/NRPN histories, SysEx device effects and synth voices
  remain outside projection. CLEANUP_MAX_EVENTS2096 and RESTORE_MAX_EVENTS8449 are
  conservative protocol bounds, not quotas that drop originals or internal events.
- Output overflow: finish logical cursor/state and other required work, report the
  first output error, then use runtime reset delivery. A successful RenderReport with
  output_error is not lossless output; do not retry later packets or replay missed
  events at the next host cut. Reserve downstream recovery capacity separately.
- Ownership: copied MediaId/ReadIndex/Source/Export views acquire no reference. Resident,
  playback, latest, pending pins and external exports are distinct owners. Retiring a
  resident does not invalidate pinned old content. A final release frees underlying
  snapshots once; generic pages still need bounded reclamation. Owner activation uses
  currently free metadata, not credit anticipated from retiring old owners.

The current source contracts for these mechanics need clarification/attribution, not
a semantic rewrite. The documentation worker must retain all refusal, zero-frame,
stale-scope, first-failure and immutable-old-content guarantees while making them
usable without reading the old backend.

## Evidence and publication boundary

Fresh warning-denied coordinated_loop debug/release and loop rustdoc pass. The example
checks full stereo sample arrays, transitions, initial held note, exported audio/MIDI
bytes and256pages/16versions/16snapshots/zero media references at teardown. It has no
allocator guard of its own and is not a nested-composite or actual worklet example.
Do not mislabel it as direct guarded proof; the integration tests supply that evidence.
If an expanded tutorial claims to demonstrate heap safety directly, add the scoped
guard and verify its canaries rather than relying on prose or a release-only build.

All123 scoped Rust/10 runner-configuration inputs and11 archived runtime guides remain
unchanged. Prior153-loop native/Wasm and2141-workspace evidence is referenced with
its immutable provenance, not freshly rerun here. Runtime book handoff, loop guide
handoff, architecture/audit follow-ups, authored publication and final acceptance are
separate obligations. Independent native/actual loop hosts and Stage8 benchmarks must
supply their own future READMEs/wire/journal notes after implementation.
