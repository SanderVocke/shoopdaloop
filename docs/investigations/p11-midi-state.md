# P11: MIDI state edge cases

Investigated 2026-11-13 on `feat/backend-replacement` at
`bfe73c9afef1c0ba2155b42d693abbf9b39ab2a3`.
This is a source assessment and implementation proposal, not an implementation.

## Scope correction after comparison with master

The user clarified that the intended scope is basic short-message MIDI, without
SysEx state reconstruction or full RPN/NRPN parameter restoration. The broader
history design, acceptance criteria and implementation stages below are therefore
superseded as a recommendation for this work. They describe the broader wording
found in the parity ledger, not behavior established on master.

Verified against master at `bebdd42299a2dec7897aad233b5c0370bbf8c833`:

- `shoop_engine/src/midi_storage.rs:13` sets `MAX_MSG_BYTES` to **4**. Longer
  messages are refused by the engine storage/driver paths. Channel MIDI normally
  takes at most three bytes; the four-byte storage allowance is not arbitrary-length
  MIDI support.
- `shoop_engine/src/midi_state.rs` tracks notes, the latest value of each CC,
  pitch wheel, channel pressure and programs. It does not identify or restore
  multiple RPN/NRPN parameter values, nor interpret or restore vendor SysEx state.
  RPN/NRPN commands themselves are three-byte CC messages, but accepting them as
  individual controllers does not provide full parameter restoration.
- Master also summarizes initial channel state rather than preserving every
  original setup message and its ordering. That alone is not a basic-MIDI parity
  defect in the replacement.
- Replacement timeline storage accepts longer packets within configured bounds;
  that additional storage capability does not require expanding the application's
  intended MIDI support. The 4/64/255-byte host limits identified below are not
  missing long-packet parity with master under the clarified scope. Output-burst
  capacity for ordinary short-message restoration is a separate concern.

The earlier references to special setup meant SysEx patch/settings dumps or reset
messages and RPN/NRPN parameter histories, not ordinary volume CC or program-change
restoration. Reassess P11 against master's basic-MIDI behavior before proposing
implementation. The parity ledger itself has not been changed by this correction.
Source paths in this section are relative to `src/rust/`.

The subsequent [Rust/RTL MIDI runtime design](../native_session_recovery/midi-runtime-rtl-design.md)
describes the proposed four-byte-cap system under the clarified scope. Use that
document for the current design; the broader history proposal below is retained
only as investigation history.

## Scope and conclusion

The [parity ledger](../native_session_recovery/feature-parity.md) leaves P11 open
for complete MIDI packet order, initial/preroll/seek state and parameter history
through capture, import, export and session save/load/store. Primitive/P17 and
cross-kind duplication have already landed. Preserve their behavior; this work
only needs to carry richer MIDI content through their existing operations.

Ordinary timeline storage already retains complete packets, including opaque
SysEx, stable equal-frame order, immutable cuts and edited versions. The central
gap is crossing a boundary: before recording starts, at a grabbed window's start,
after history eviction, at logical zero, or on a seek. These paths retain or
restore a `MidiState` projection rather than the complete preceding history.
There are also smaller host packet limits that prevent end-to-end packet parity.

This is an extension across runtime storage, loop restoration, transfer, codecs
and hosts. Removing the existing refusal checks alone cannot close P11.

## Concrete gaps

| Gap | Current evidence | Observable consequence |
| --- | --- | --- |
| Initial/preroll input is restricted | `shoop_rt_backend/src/prepared.rs:27` admits only short channel packets and System Reset. `prepared/validation.rs:177` also applies that restriction to events before the channel origin and rejects CC6/38/96/97 history unless a later System Reset clears it. `shoop_persistence_codec/src/codec.rs:747` independently validates the same area. | SysEx and system-common/realtime prefixes refuse. Multiple RPN/NRPN writes and relative parameter commands in a prefix refuse. Changing a window can make previously accepted timeline data unsupported. The duplicate validators can drift. |
| Uploaded initial packets become a projection | `shoop_rt_protocol::Request::ContentInitial` contains `MidiMessage`; `shoop_rt_engine/src/content.rs:352` narrows this to three bytes. `rt_runtime::resource::Request::MidiInitial` is `[u8; 3]`; `resource/midi_index.rs:190` applies packets to checkpoint zero without retaining their originals. | Even accepted initial packet order is not available from the runtime resource. A richer wire message alone does not fix storage. |
| Capture exports synthesized initial packets | `shoop_rt_engine/src/bulk.rs:537` calls `restore_channel` and refuses `parameter_history_untracked`. `shoop_rt_backend/src/capture.rs:233` concatenates sixteen channel reads. `reads.rs:429` limits each initial read to 16 KiB and parses one-byte lengths of at most three bytes. | Recapturing engine-owned content cannot preserve the original initial prefix or cross-channel ordering, and cannot carry long initial SysEx. Save/copy/export can refuse when needed state includes untracked parameters. |
| History eviction, Grab and Replace lose nonprojected prefix information | Production installs the semantic indexed MIDI pool in `shoop_rt_engine/src/history.rs:186`. `rt_runtime/src/resource/midi_writer/history.rs:197` folds discarded history into `MidiState`, then releases/clips packet roots; `grab_source` obtains only projected state at the new start. `midi_writer.rs:1077` refreshes logical-zero initial state from the input projection. | An old vendor command or parameter write cannot later be reconstructed from the retained window. Packet coverage and restorable-state coverage currently have no separate history representation. Preserve the older `shared_midi` path too where it remains supported, but changing only that path misses production. |
| Start/seek/wrap restoration cannot recover parameter or vendor state | `shoop_loops/src/midi.rs:181` seeks to a projection and calls `midi_restore::restore`. The restorer deliberately nulls selectors around cached Data Entry and does not synthesize relative commands or vendor state. `runtime/processor.rs:216` consumes only `.output_error`, discarding the renderer's untracked-history report. Window validation explicitly refuses untracked parameter state. | Playback can preserve all later originals while restarting with the wrong external parameter/vendor state. The parameter limitation is not surfaced through this render path. Existing key/pedal, loaded/pending bank and selector-order behavior is already implemented and must remain correct. |
| Logical exact export projects away prefix history | `shoop_persistence_codec/src/codec.rs:660` folds pre-origin events through `MidiState`, replaces `start_state` with synthesized restoration packets and crops/rebases the timeline. | Exact export is exact only for the supported projection. It refuses vendor/untracked parameter preroll instead of retaining that prefix. Session storage of the raw window and logical standalone export need compatible, explicit history semantics. |
| SMF loss reporting is incomplete | `shoop_session/src/media.rs:443` places initial packets at tick zero; decoding returns an empty `start_state`. System-common/realtime packets are encoded as SMF Escape events, while the decoder's `as_live_event` path skips those events. The existing shared-MIDI codec test explicitly expects System Reset to disappear. `StandardMidiExport` reports only timing error; the client warning mentions quantization. | Initial-prefix distinction is lost, and some packet classes disappear on round trip without a specific loss report. Exact-format and SMF claims must be separate. |
| Host paths cannot carry all stored packets or guaranteed restoration bursts | Native `shoop_rt_host_native/src/midi.rs` caps bridged packets at 64 bytes and queues at 1024 messages; output drops packets that do not fit. Browser track input in `shoopdaloop/src/browser_midi.rs` refuses packets over four bytes even with SysEx permission. Worklet `lane.rs` uses a one-byte length (255-byte maximum), and `config.rs` defaults to 4 KiB MIDI lanes; `host.rs:376` counts output drops. | A packet supported by media storage can never enter or leave some live hosts. A valid engine restoration can overflow the smaller host lane. These are counted refusals/drops, not byte truncation, but still prevent end-to-end exactness. |

Source paths above are relative to `src/rust/` unless qualified otherwise.
Native bridge limits apply to the threaded MIDI bridge; direct JACK/direct-driver
paths have their own sink capacities and must be verified separately.

An important testing trap: the adapter retains canonical imported content and can
capture it without rereading engine media. Existing tests correctly preserve
ordered supported initial packets through that cache. P11 tests must additionally
force engine-owned reads after recording, Grab, Replace or cache invalidation.
An unchanged import/save round trip alone does not prove runtime history retention.

## Recommended design

Keep `MidiState` for fast keys, pedals, controllers, program/bank and pressure
queries. Pair it with an immutable ordered history owner using the existing packet
slabs, semantic indexes, snapshot leases and retirement accounting. Avoid copying
a large history vector into every checkpoint.

The content model should distinguish:

- Original initial packets, with stable total order across channels.
- Timed preroll/history and ordinary timeline originals, preserving complete
  packet boundaries and strict pre-target cuts.
- Baseline provenance and history coverage: what was actually observed/retained,
  where reconstruction is valid, and which replay requirements are unresolved.
- A prepared restoration description, owned by the same immutable content version.

Do not flatten the initial prefix into frame-zero timeline events. At target F,
state/history covers only packets strictly before F; originals at F play once in
their original order after cleanup and restoration. Preserve existing preplay,
start-offset, alignment, role and composite timing rules without depending on
P09/P10 controls.

For known RPN/NRPN semantics, add a bounded sparse parameter index keyed by channel,
selector family and parameter number, retaining selector changes/nulling, partial
Data Entry, multiple parameter values and relative commands. Emit absolute values
only when they are actually known. Keep original commands independently of any
semantic compression. Update sequential and indexed state implementations together.

The first design decision is replay policy. Repeating CC96/97 against an unknown
receiver baseline is not idempotent. Opaque SysEx may also be stateful; a generic
engine cannot infer its reset or deduplication semantics. Recommended contract:
preserve the exact observed packet history; reconstruct supported known protocol
state; describe baseline-dependent replay explicitly; refuse unsupported exact
reconstruction before mutation. Never silently replay clocks, Start/Stop or other
transient system traffic as persistent state. Their originals still belong in
exact media. Do not promise synthesizer envelopes, voices or state never observed.
Unknown legacy baselines must remain unknown rather than being upgraded to known
reset state during migration. These semantic limits need to be stated in the P11
completion contract, not hidden by suppressing a warning.

Prepare restoration and reserve its full output event/byte cost before activation.
Start/seek/wrap execution must not scan an arbitrary history prefix, allocate, or
spill a required restoration across later musical callbacks. Reserve across every
simultaneous loop and through actual host egress, including recovery traffic.
History retention, parameter indexes, old snapshots, imports and retirement all
consume finite credits. On eviction or an input gap, retain a valid replay base or
advance truthful restoration coverage; do not claim that a projection preserved
opaque history. Physical device refusal remains an explicit failure, not a
guarantee the application can eliminate.

## Work by component

| Component | Implementation responsibility | Focused verification |
| --- | --- | --- |
| `rt_runtime` state/index/resource layers | Own immutable initial/history packet roots, baseline/coverage, sparse parameter state and bounded replay lookup. Extend ordinary uploads, semantic writers, Grab, zero-crossing Replace, retained exports and supported shared-store paths. Charge ownership and retirement. | Independent linear packet-history oracle and parameter receiver model; equal-frame strict cuts, eviction/gaps, COW edits, old leases, pool pressure and no-heap guards. |
| `shoop_loops` | Compose cleanup, history-aware parameter/vendor replay and existing projected key/pedal restoration. Define start, preplay, seek, wrap and recording transitions with prepared readiness and output budgets. Carry restoration status to the planner. | Repeated seeks/wraps; loaded versus pending banks; sustain/sostenuto/Hold2 and initially held notes; resets, selector changes and relative commands; Q=1/16/32/64/128 with irregular callbacks. |
| `shoop_rt_protocol`, `shoop_rt_engine` | Add a versioned chunked history upload/read shape and preparation/admission metadata. Export original history from the same immutable cut, not `restore_channel` output. Include negotiated restoration limits in layout/capacity planning. | Typed and encoded equivalence, packet splits across pages/chunks, stale generations, cancelled uploads/reads, complete refusal and simultaneous restoration pressure. |
| `shoop_backend`, `shoop_rt_backend` | Extend canonical content DTOs and validation proofs; stream/assemble initial/history bytes with aggregate limits. Preserve history through cache invalidation and engine capture. Replace duplicate short-packet policy only after downstream support exists. | Compare cached and uncached capture; load/import/copy refusal before mutation; preserve unrelated media and supported metadata; large initial packets and cancellation. |
| `shoop_app_protocol`, `shoop_app` | Carry history and coverage in admitted musical content and immutable capture. Reuse persistence staging, duplication and operation recovery. Project useful unsupported-baseline/capacity diagnostics to clients. | Public UI/Lua save/store/load/import/export/copy, retained cuts and detach/reconcile without replay. Existing cross-kind clear-then-copy failure semantics remain intact. |
| `shoop_format_conversion`, `shoop_session`, `shoop_persistence_codec` | Preserve original prefix/timed history and metadata in exact files, session archives, logical cuts and rate conversion. Version new semantics when required; load old supported files faithfully. Add structured SMF packet/prefix/timing loss reporting or refuse unsupported conversion. | Old/new file compatibility, raw/session versus logical export, cross-channel equal-frame order, repeated resampling, complete history round trips, explicit SMF losses. |
| Native hosts, browser MIDI and AudioWorklet | Replace short inline live-packet assumptions with bounded complete-packet transport. Size/admit native queues and worklet lanes against restoration bursts; retain generation/order and report device/transport failure. Keep scripting-control and musical-input budgets distinct. | Live SysEx beyond 4/64/255 bytes; simultaneous restores with saturated egress; direct JACK and threaded bridge separately; actual AudioWorklet plus Web MIDI forwarding. |
| Client/UI, firmware capability reporting and docs | Expose truthful coverage/refusal/conversion outcomes. Advertise only capacities actually supported by each configured host. Update the persistence contract and P11 evidence after end-to-end validation. | Public workflows, unavailable SysEx permission, finite firmware configuration, actionable diagnostics, parity/dependency-boundary checks. |

## Proposed acceptance criteria

1. Every admitted exact-media operation preserves complete observed packet bytes,
   packet boundaries, initial/preroll distinction and stable equal-frame order
   through Record, Grab, Replace, duplication, import, exact export and session
   save/load/store. Boundary originals are neither duplicated nor omitted.
2. Start/seek/wrap correctly restores the protocol state covered by the documented
   baseline/replay contract, including multiple RPN/NRPN parameters and relative
   operations with a known baseline. Unknown/vendor-dependent cases preserve bytes
   and produce the documented outcome rather than invented state or silent loss.
3. All histories and restoration plans are finite, allocation-free during render,
   generation-fenced and version-owned. Required output fits admitted engine and
   host budgets; unsupported candidates refuse before publication.
4. Existing key/pedal/program/bank behavior, role/timing semantics, immutable cuts,
   copy semantics and accepted-operation recovery remain correct.
5. Supported older files remain compatible. New requirements have explicit version
   handling. SMF reports packet/prefix loss as well as timing quantization.
6. Native typed/socket/applicable shared-memory, actual browser AudioWorklet,
   public UI/Lua and offered firmware configurations have relevant evidence before
   P11 is marked complete.

## Suggested stages and dependencies

- [ ] **1. Contract and oracles.** Create a focused implementation branch from
  `feat/backend-replacement`. Fix baseline/replay semantics, history shape,
  versioning and negotiated budgets; add independent packet/parameter oracles and
  failing boundary fixtures. Verify the new fixtures against today's explicit
  refusals and existing projection behavior.
- [ ] **2. Runtime representation.** Implement resource/history and sparse
  parameter ownership, cut/eviction semantics, import, Grab and Replace. Verify
  linear-oracle equivalence, retained snapshots, capacity, cleanup and allocator
  guards. This establishes the ownership model needed by all subsequent stages.
- [ ] **3. Transfer and host capacity.** Extend protocol/engine/adapter paths and
  native/worklet complete-packet transport; evolve schemas and configuration
  versions together. Verify typed/encoded parity, cached/uncached capture, chunk
  splits, cancellation and worst-case host output admission. Depends on stage 2.
- [ ] **4. Loop restoration.** Integrate prepared replay with cleanup/projection,
  start/preplay/seek/wrap, planner status and graph-wide output budgets. Verify
  independent receiver models, repeated discontinuities, concurrent loops and
  all-Q/irregular-callback behavior. Depends on stages 2 and 3.
- [ ] **5. Application and files.** Carry history through service operations and
  codecs, logical exports, rate conversion and duplication; add compatibility,
  SMF loss and client diagnostic behavior. Verify public files/workflows and
  accepted-operation recovery, forcing engine-owned capture. Depends on the
  stable model/transport and restoration contract from earlier stages.
- [ ] **6. End-to-end acceptance and delivery.** Run packaged native and actual
  AudioWorklet workflows with boundary MIDI, pressure/fault injection and
  persistence/re-edit; verify applicable firmware flags/budgets. Run repository
  formatting, warning-denying build, full native suite, test-usage, parity and
  dependency checks. Update evidence/ledger, push and create a PR targeting
  `feat/backend-replacement`; work CI to green and address automated review until
  approved. Do not merge automatically.

Use focused tests during stages 1–5 and the full suite at integration milestones
and final delivery. This avoids repeatedly paying for unrelated platform suites
while preserving end-to-end confidence. Follow `.agents/info/build.md` and
`.agents/info/test.md` for the selected platform and exact gate commands.

Execution contract: keep this plan updated and check off completed work; commit
each meaningful milestone; revise implementation steps when evidence warrants it;
revise design rules only with a documented reason. Do not change agreed goals or
acceptance criteria without explicit user approval. No implementation branch, PR,
push or feature changes are part of this investigation itself.

## Investigation verification

Twelve focused existing tests passed with the repository's prepared toolchain and
`RUSTFLAGS="-D warnings"`:

- `cargo test --locked -p rt_runtime --test midi_restore_state`: 2 passed.
- `cargo test --locked --offline -p shoop_loops --test midi_restore`: 4 passed.
- `cargo test --locked --offline -p shoop_persistence_codec` with the individual
  filters `importing_unrestorable_midi_initial_state_refuses_before_staging`,
  `a_final_system_reset_makes_prior_parameter_history_restorable` and
  `--lib codec::shared_midi_tests`: 3 passed in total.
- `cargo test --locked --offline -p shoop_rt_backend --test persistence
  untracked_initial_parameters_refuse_but_a_later_system_reset_is_preserved`:
  1 passed.
- `cargo test --locked --offline -p shoop_rt_host_worklet --lib lane::tests`:
  2 passed. These exercise native lane logic, not an actual browser.

These tests confirm the existing behavior and explicit limitations. No full suite,
physical MIDI device run or actual browser acceptance is claimed for this source
investigation. The new history design and receiver semantics are proposals, not
validated implementations. The only repository edit is this report; no production
code, tests, ledger status or existing plan was changed.
