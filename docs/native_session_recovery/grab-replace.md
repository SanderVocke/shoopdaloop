# Application Grab and Replacing integration

Implementation contract for [the application plan](../../PLAN_FINISH_GRAB_REPLACE.md).
Both P04 and P05 remain **OPEN**. No production capability is enabled by this
document or the window arithmetic helper. Final application evidence will be
recorded here as the implementation stages pass.

## Layer inventory

| Layer | Required work |
| --- | --- |
| Normal host preparation | `shoop_host/engine`, native `direct`, and worklet `config`/`host` must prepare histories from a common production policy. `shoop_rt_engine::prepare` currently supplies empty shared settings. |
| Portable domain | Add continuous writers and prefix continuation to `shoop_loops/runtime`, with a distinct Replacing mode in the resolver/program state. Retain the private-candidate API for prepared edits. |
| Kernel resources | Grab roots now retain visible audio pages/MIDI chunks; prepared metadata follows retained credits. Production global pooling, checkpoint-state density and continuous-writer reservations still need completion. |
| Engine / protocol | Add bounded admission, musical batch settlement, completions and capability/limit negotiation in `shoop_rt_protocol`, `shoop_rt_engine/endpoint`, jobs, model, cells, topology and resync. |
| Adapter | Implement Grab and Replacing in `shoop_rt_backend/backend`, requests, incoming/mirror, reads, capture, family and persistence. Invalidate canonical-import media on actual committed revisions. |
| Service | Replace deliberate refusals in `shoop_app/service/commands` and `commands/control`; reconcile media, projection, persistence and family guards with active/scheduled Replacing. |
| UI / Lua | Enable negotiated actions in loop/global controls and the normal app runtime; preserve script constants and route every action through service ownership. |

## Reference policy matrix

Let L be the current sync period, P its phase at the effective cut, N the
apply-N-cycles value clamped to at least one, T the targeted loop's period in
sync cycles, and C its current cycle. Resolve these musical values from
authoritative state at settlement, not an earlier UI poll.

| Intent | Targets | Window / initial mode | Completion |
| --- | --- | --- | --- |
| Synced UI Grab, no target | Initiator plus selected peers only when the initiator is selected | Last N complete sync cycles before current phase; position P; Playing if play-after-record, otherwise Stopped | One coherent batch |
| Synced UI Grab with target | Same selection rule | T complete cycles ending before phase P plus C earlier cycles; position C*L+P; same post-mode policy | One coherent batch |
| Unsynced UI Grab, no target | Initiator only | Prefix (N-1)*L+P ending at the live cut; continue Recording from its end | Finish at the reference next sync boundary into Playing/Stopped |
| Unsynced UI Grab with target | Initiator only | Prefix C*L+P ending at the live cut; continue Recording | Finish using target-delay policy |
| Control/Lua Grab | Exactly the explicit list | Global musical Grab policy, with no implicit selection expansion | Reliable batch outcome |
| AdoptRingbuffers | Exactly the explicit list | Explicit window, cycle position and post-mode | Reliable batch outcome |
| Solo | Peers on affected tracks | Stop only peers outside the admitted target set | Do not stop peers if admission refuses |
| Missing sync length for UI/default Grab | No mutation | Refuse with an actionable reason | No optimistic completion |

The low-level `shoop_loops::grab::WindowSpec` returns a retained source range,
logical duration, cycle/phase destination and explicit trailing empty duration.
It implements window arithmetic only. The service/domain must still validate
source continuity, IDs, modes, Q, budgets and admissibility before acquiring or
publishing media. Its frame coordinates remain u64, including above u32 time.
Available samples start at logical zero in the legacy adoption path; missing
requested duration is explicitly empty at the end. An unavailable/faulted source
range is not silently reclassified as startup padding.

`GestureSpec` resolves the musical portion using the authoritative sync period
and targeted state at that cut. It returns the initial mode and, for unsynced
grabs, the completion mode/delay. Target membership, solo peers and atomic
admission remain service/engine responsibilities. The target-membership audit
found that legacy Control Grab expands selection while raw AdoptRingbuffers is
exact-list. The user identified selection-dependent explicit scripts as a bug;
Control/Lua Grab will intentionally use the exact supplied list. UI gesture
selection semantics remain as shown above.

## Behavior-to-test matrix

The case IDs below define the production acceptance group to add in F4–F8.
They are specifications, not claims that those public-route tests already pass.
Reference policy comes from `grab_targets_inner`, `grab_window`,
`adoption_window`, BasicLoop and the direct channel replace/erase scenarios.
Existing kernel proofs are listed separately so they cannot substitute for the
service/adapter/UI assertions.

| Cases | Required public route and expected confirmed result | Existing lower-layer proof |
| --- | --- | --- |
| G01–G02 | Grab button with sync enabled: an unselected initiator changes only itself; a selected initiator includes selected peers across source tracks. All targets share one effective cut. | Gesture/window arithmetic; pooled source complete-cut tests. |
| G03–G04 | Targeted Grab uses the target's rounded sync-cycle period and current cycle at settlement. Without a target, apply-N is clamped to at least one. Both play-after-record settings produce Playing or Stopped, respectively. | `gesture_policy_resolves_target_cycle_and_unsynced_finish_at_the_authoritative_cut`; targeted lookback tests. |
| G05–G06 | Unsynced button Grab uses only its initiator, retains the prefix through the live cut, continues Recording once, then completes at the target delay or next sync boundary into the configured post-mode. Include pending target transitions. | Gesture finish-delay tests; padded MIDI continuation and wrapped audio-tail oracles. |
| G07 | A per-loop default action with default-recording Grab targets that loop. The bundled keyboard script may choose its explicit action list; it must not gain another implicit selection expansion in the service. Pending default actions cancel under the reference policy. | Reference `production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`; production route pending. |
| G08 | Control/Lua Grab uses exactly its supplied ordered targets and the global musical policy. AdoptRingbuffers uses its explicit window/position/post-mode arguments. Neither expands selection. | Window arithmetic, including raw adoption above u32; intentional explicit-list correction recorded above. |
| G09 | Missing sync length refuses UI/default Grab without changing targets or solo peers. Explicit raw adoption without sync retains the available range at position zero. Empty target lists are no-ops; an invalid member refuses a nonempty batch before mutation. | Missing-sync and raw-adoption arithmetic; reference `fake_grab_preflights_every_target`. |
| G10–G11 | Startup, silent and missing-input histories produce explicitly padded requested durations; sparse MIDI retains complete boundary state. A source failure or discontinuous requested range refuses instead of fabricating startup silence. | Audio discontinuity tests; indexed MIDI padded windows, byte-density eviction and strict-before-cut state tests. |
| G12 | Solo stops only peers on affected target tracks outside the admitted target set, atomically with Grab. A stale ID, route change, full owner pool or invalid final target changes no target or peer. | Coordinator source-generation/candidate fences; production batch proof pending. |
| G13 | Audio-only, MIDI-only and mixed tracks share the policy above. Trigger tracks retain meaningful length/phase/transition behavior without requiring sample/packet resources. | Audio/MIDI-only and mixed coordinator tests; trigger production policy pending. |
| R01–R02 | Immediate and sync-scheduled Playing → Replacing preserve period, phase, cycle scheduling and controls. Replacing advances and repeats while suppressing ordinary loop playback; monitoring remains independently controlled. | Reference BasicLoop playing-mode transition and REPLACE flag tests; fixed writer wrap oracles. |
| R03 | Every admitted positive tick overwrites all channels and complete MIDI packets. Positive empty MIDI input erases originals; zero-length input performs no edit. Preserve equal-frame order, SysEx and natural suffix-state effects. | Continuous writer packet/state oracle; stereo COW replacement tests. |
| R04–R05 | Stop exits to complete media and rewinds. Replacing → Playing preserves the playing-mode phase. Replacing → Recording starts a fresh growing recording; Recording → Replacing uses the completed recording as its fixed base. Cancellation before entry changes no media. | Core writer complete exit and immutable export tests; reference transitions; production lifecycle pending. |
| R06 | Prefix continuation consumes the first live tick once, grows beyond the history horizon and may subsequently become a fixed replacement base. Old exports and sibling users retain their original bytes/state throughout. | Wrapped-prefix/loaded-PCM continuation and subsequent replacement; shared continuous MIDI resource tests. |
| R07–R08 | Writer pressure/exhaustion, clear/removal, slot reuse, source reset and terminal faults produce truthful final outcomes and release private owners. Required tick work and accepted exits do not depend on optional transfer credits. Unrelated playback/recording continues. | Source-reset/revision fences, required-work/exit tests and full credit recovery; production job/fault proof pending. |
| X01–X02 | Active or scheduled writers obey the existing media/save/import/family/session conflict policy. Previously pinned immutable exports remain readable. Save/load persists resulting media/default-Grab settings and reloads loops stopped. | Existing private publication persistence tests; continuous production persistence pending. |
| X03 | UI detach/reattach and controller loss preserve the service-owned admitted request and its outcome exactly once. Lua is not restarted or replayed. | Existing service recovery contract; new operation recovery proof pending. |
| X04 | Normal startup negotiates enabled Grab/Replacing only after host, protocol, adapter and service support agree. Unsupported/old peers stay unsupported; warmup and capacity refusal remain per-request outcomes. | Protocol capability/refusal contract; production success gates pending. |

The service/adapter policy group must port the meaningful expectations from
`grab_policy_covers_targeted_selection_solo_and_immediate_completion`,
`production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`,
`expanded_loop_actions_route_modes_grab_and_balance` and
`fake_grab_preflights_every_target`. Dry/wet assertions remain under P06.
The final packaged-app sequence must exercise button, bundled keyboard and public
Lua routes against real samples/packets, not substitute internal operation calls.
Requested/effective cuts must include fixed-quantum alignment and any declared
host buffering. Admission, pending projection, completion, reconnect and resync
must identify the same accepted operation and effective cut.

## Replacing and continuation contracts

Replacing uses fixed-period overwrite and cursor repetition, without ordinary
playback from that loop. Input monitoring remains a separate route. Playing to
Replacing preserves phase; Stop rewinds according to the reference. MIDI empty
positive intervals erase originals, and edits have natural suffix-state effects.
No synthetic restoration packet is stored at a splice.

Unsynced Grab must seed a growing recording with the retained prefix and join
the live stream once at the cut. A normal Record transition that starts a fresh
empty take is insufficient. Stop and playback return complete immutable media;
old exports and sibling takes retain their original data.

Current CandidateMidi preparation owns one patch, rebuilds affected indexes in
bounded steps, and refuses another patch while Building. It cannot be polled as
a continuous recording implementation. The continuous writer must separately own
every admitted tick and bound index debt, input retention and exit resources.
Neither moving publication deadlines nor queue growth establishes that bound.

## Production sizing gate

The reference nominal retention is 30 seconds. At 48 kHz, sixteen stereo sources
need 184,320,000 payload bytes for that interval. The shared arena's page geometry,
retained users, COW writes and MIDI add to this amount. The existing 256 MiB
loop-memory setting must keep its documented meaning.

Audio owners now charge the window's covering pages and retain a clipped root.
Its index budget follows page credits plus boundary paths rather than a full ring
per view. MIDI owners similarly retain and charge the intersecting chunks;
metadata follows the total retained-chunk credits, with admission reserving edit
growth and immutable source pins. These conservative guarantees still need
production global pooling and
admission must reserve guaranteed live history/writer work while sharing immutable
data, bounding metadata and refusing additional owners before live capture loses
its reservation. Exact budgets/defaults and their measured limits remain an F2
verification gate, not an implied unlimited capacity claim.

MIDI configuration now offers an allocation-free estimate of the complete
prepared metadata/state shape and owner ceiling. Unreachable handle/initial-state
slots are excluded. A heap-guarded pressure test retains all admitted owners while live
input wraps repeatedly, then edits a retained cut and checks the old export.
Further packet/state oracles cover clipped roots and growing/shrinking edit-credit
transfer while old exports remain readable. Full checkpoint projections,
per-source reservations and continuous-writer debt
still require the remaining production work; this change does not establish a
default application budget or enable either capability.

The semantic continuous MIDI pool now has ordinary resource/snapshot backing.
Uploaded packet bases are pinned by bounded import jobs; multiple fixed/growing
writers share source tick bodies and publish complete revisions without a fresh
exit allocation. Byte reads, seek/state reconstruction and independent retained
exports use the same public resource interfaces as existing media. Generation,
source-reset and content-revision fences prevent stale jobs from publishing.
Cancellation and teardown retain prepared staging arrays until host destruction.
An allocation-free sizing API reports this pool's metadata and payload reservation;
production live/writer credit guarantees and host defaults remain outstanding.

Audio resource views can now reserve a growing tail after a retained wrapped
prefix, or after immutable loaded PCM, without copying the prefix. The tail has
separate physical coordinates and may exceed the history horizon. Empty startup
takes, immutable exports, capacity refusal and later fixed-period edits use the
ordinary resource boundary. This remains a kernel/resource path; musical settlement,
loop mode/output behavior and normal host/application integration are not enabled.

## Validation status

No application success is claimed yet. CI will run the new arithmetic/reference
cases and the complete integration gates as the plan progresses. Existing
unsupported-backend tests remain valid until negotiated support is implemented.
