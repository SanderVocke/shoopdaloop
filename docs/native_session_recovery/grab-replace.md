# Application Grab and Replacing integration

Implementation contract for [the application plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_FINISH_GRAB_REPLACE.md).
Grab and Replacing run through negotiated production capabilities and the normal
service, adapter, companion, firmware and AudioWorklet. P04/P05 are COMPLETE under the
[application acceptance evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/grab-replace.md), including exact-media
checks, measured capacity limits and final validation pins.

## Layer inventory

| Layer | Implementation |
| --- | --- |
| Normal host preparation | Common thirty-second audio/indexed MIDI histories, independent source generations and a reservation separate from ordinary recording. Native host setup v8 and worklet config v7 carry the limits. |
| Portable domain | Continuous fixed/growing paired writers, Replacing tag 5, complete-cut exports, reserved exits and render-owned musical finishes. DryPlay/Rerecord retain tags 3/4. |
| Kernel resources | Persistent audio roots and semantic MIDI indexes share immutable payloads. Global owner admission protects live sources, staging, edit paths and retirement debt. |
| Engine / protocol | Protocol v23 carries bounded input transactions, effective cuts, status/cancel/completion and negotiated limits. Batch settlement includes targets, solo peers and automatic completion atomically. |
| Adapter | One owned admitting input job; confirmed mutation identity, pending projection, revision-driven media invalidation and independent processor controls. |
| Service | Selection-aware gestures, exact explicit Control/Lua lists, targeting, solo, default Grab, apply-N and play-after-record; persistence and topology changes fence accepted work. |
| UI / Lua | Normal Grab controls, bundled keyboard/default action and public Lua; primitive-loop “Replace input” menu, confirmed mode/pending state and an incomplete-capture warning. Application protocol v31. |

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
It implements window arithmetic only. The service/domain validates
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
Control/Lua Grab intentionally uses the exact supplied list. UI gesture
selection semantics remain as shown above.

## Behavior-to-test matrix

The case IDs below map the production contract to complementary public-route and
resource proofs. The evidence document records their execution and limitations.
Reference policy comes from `grab_targets_inner`, `grab_window`,
`adoption_window`, BasicLoop and the direct channel replace/erase scenarios.
Existing kernel proofs are listed separately so they cannot substitute for the
service/adapter/UI assertions.

| Cases | Required public route and expected confirmed result | Production and lower-layer proof |
| --- | --- | --- |
| G01–G02 | Grab button with sync enabled: an unselected initiator changes only itself; a selected initiator includes selected peers across source tracks. All targets share one effective cut. | `public_grab_selection_exact_lists_solo_replacing_and_reload_share_owned_service_outcomes`; packaged native/browser selected cross-track Grab; pooled source complete-cut oracles. |
| G03–G04 | Targeted Grab uses the target's rounded sync-cycle period and current cycle at settlement. Without a target, apply-N is clamped to at least one. Both play-after-record settings produce Playing or Stopped, respectively. | `gesture_policy_resolves_target_cycle_and_unsynced_finish_at_the_authoritative_cut`; public engine gesture oracle, both post-modes and self-clock; target pending-delay case. |
| G05–G06 | Unsynced button Grab uses only its initiator, retains the prefix through the live cut, continues Recording once, then completes at the target delay or next sync boundary into the configured post-mode. Include pending target transitions. | `public_gesture_continues_the_prefix_and_finishes_without_controller_polling`; `public_gesture_uses_the_targets_pending_clock_delay_at_its_effective_cut`; native reattachment and browser stall. |
| G07 | A per-loop default action with default-recording Grab targets that loop. The bundled keyboard script may choose its explicit action list; it must not gain another implicit selection expansion in the service. Pending default actions cancel under the reference policy. | `bundled_keyboard_and_lua_use_negotiated_grab_and_replacing_through_the_normal_runtime`; packaged Space/default-Grab, retained reference fixture. |
| G08 | Control/Lua Grab uses exactly its supplied ordered targets and the global musical policy. AdoptRingbuffers uses its explicit window/position/post-mode arguments. Neither expands selection. | Public adapter/service exact-list tests and packaged Lua; window arithmetic/raw adoption above u32; intentional explicit-list correction recorded above. |
| G09 | Missing sync length refuses UI/default Grab without changing targets or solo peers. Explicit raw adoption without sync retains the available range at position zero. Empty target lists are no-ops; an invalid member refuses a nonempty batch before mutation. | Public service invalid-member/solo refusal, engine atomic batch/admission/cancel scenarios and window arithmetic; retained reference preflight fixture. |
| G10–G11 | Startup, silent and missing-input histories produce explicitly padded requested durations; sparse MIDI retains complete boundary state. A source failure or discontinuous requested range refuses instead of fabricating startup silence. | `public_musical_grab_does_not_pad_density_eviction_as_recorded_history`; public startup gesture, source fault/reset and indexed MIDI strict-before-cut/state oracles. |
| G12 | Solo stops only peers on affected target tracks outside the admitted target set, atomically with Grab. A stale ID, route change, full owner pool or invalid final target changes no target or peer. | `input_batches_preflight_all_media_targets_and_solo_peers_at_one_production_cut`; public service solo/refusal and protocol late-refusal cases. |
| G13 | Audio-only, MIDI-only and mixed tracks share the policy above. Trigger tracks retain meaningful length/phase/transition behavior without requiring sample/packet resources. | Public bundled keyboard/Lua test parameterizes MIDI-only and trigger shapes; audio-only/mixed coordinator and packaged stereo/MIDI routes. |
| R01–R02 | Immediate and sync-scheduled Playing → Replacing preserve period, phase, cycle scheduling and controls. Replacing advances and repeats while suppressing ordinary loop playback; monitoring remains independently controlled. | Public protocol scheduled Replacing and adapter pending mutation; `replacing_maps_each_channel_window_and_preserves_raw_tails_manual_period_and_monitoring`; retained reference transition tests. |
| R03 | Every admitted positive tick overwrites all channels and complete MIDI packets. Positive empty MIDI input erases originals; zero-length input performs no edit. Preserve equal-frame order, SysEx and natural suffix-state effects. | Continuous writer packet/state oracle (including long SysEx), stereo COW replacement, packaged exact file comparison and repeated wraps. |
| R04–R05 | Stop exits to complete media and rewinds. Replacing → Playing preserves the playing-mode phase. Replacing → Recording starts a fresh growing recording; Recording → Replacing uses the completed recording as its fixed base. Cancellation before entry changes no media. | `prefix_recording_to_replacing_keeps_the_boundary_tick_and_active_exports`; public protocol cancellation/exit and long ordinary Recording-to-Replacing oracle. |
| R06 | Prefix continuation consumes the first live tick once, grows beyond the history horizon and may subsequently become a fixed replacement base. Old exports and sibling users retain their original bytes/state throughout. | Wrapped-prefix/loaded-PCM continuation and subsequent replacement; shared continuous MIDI resource tests. |
| R07–R08 | Writer pressure/exhaustion, clear/removal, slot reuse, source reset and terminal faults produce truthful final outcomes and release private owners. Required tick work and accepted exits do not depend on optional transfer credits. Unrelated playback/recording continues. | Public cancellation/source-fault tests, `a_failed_or_cancelled_growing_gesture_cannot_be_restarted_by_its_owned_finish`, sustained global credit stress and teardown. |
| X01–X02 | Active or scheduled writers obey the existing media/save/import/family/session conflict policy. Previously pinned immutable exports remain readable. Save/load persists resulting media/default-Grab settings and reloads loops stopped. | Public adapter immutable-export test, production service capture/reload/re-edit and packaged exact save/load/save workflows. |
| X03 | UI detach/reattach and controller loss preserve the service-owned admitted request and its outcome exactly once. Lua is not restarted or replayed. | Native socket canary drops the UI/Lua VM during prefix Recording; host finishes once, reattached UI observes it. Browser stalls the main thread during completion. |
| X04 | Normal startup negotiates enabled Grab/Replacing only after host, protocol, adapter and service support agree. Unsupported/old peers stay unsupported; warmup and capacity refusal remain per-request outcomes. | Normal native/browser constructors, production keyboard/Lua runtime, UI menu success/refusal tests and version/capability codec tests. |

The service/adapter policy group ports meaningful expectations from
`grab_policy_covers_targeted_selection_solo_and_immediate_completion`,
`production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`,
`expanded_loop_actions_route_modes_grab_and_balance` and
`fake_grab_preflights_every_target`. Dry/wet assertions remain under P06.
The packaged-app sequence exercises button, bundled keyboard and public
Lua routes against real samples/packets, not substitute internal operation calls.
Requested/effective cuts must include fixed-quantum alignment and any declared
host buffering. Admission, pending projection, completion, reconnect and resync
must identify the same accepted operation and effective cut.

## Replacing and continuation contracts

Replacing uses fixed-period overwrite and cursor repetition, without ordinary
playback from that loop. Input monitoring remains a separate route. Playing to
Replacing preserves phase; Stop rewinds according to the reference. MIDI empty
positive intervals erase originals, and edits have natural suffix-state effects.
No synthetic restoration packet is stored at a splice. With repeat-sync enabled,
a completed pass waits at the period end without advancing or editing audio.
Zero-width MIDI ticks preserve input continuity; the next sync pulse resumes the
same admitted owners at zero.

Unsynced Grab must seed a growing recording with the retained prefix and join
the live stream once at the cut. A normal Record transition that starts a fresh
empty take is insufficient. Stop and playback return complete immutable media;
old exports and sibling takes retain their original data.

CandidateMidi remains a bounded one-shot patch interface. The continuous route owns
source ticks and private roots separately and reserves final publication owners.
Global retained/live MIDI credits are checked before owner or writer admission.
Required index work and deferred retirement run independently of optional transfer
credits. A complete tick stages audio/MIDI, preflights ordered edits and commits a
paired cut. Exit consumes reserved owners without repairing an unchanged suffix.

Ordinary recordings prepare a private indexed prefix while the recorder remains
authoritative and keeps capturing. Premature entry refuses; Stop/cancel releases
preparation. At the accepted cut the prepared prefix joins the captured tail once.
A source fault or exhaustion terminates at the last complete cut and publishes
`capture_incomplete`; it also cancels the request's owned automatic finish. The
warning remains visible until fresh successful capture clears it.

Fixed writers also retain manual periods and per-channel raw windows. They write
only each channel's mapped overlap and preserve outside data, gains and preplay.
MIDI raw durations need not align to the quantum. Raw-zero state is taken strictly
before its mapped source offset, and density refusal preserves the preceding
complete cut. Switching between Playing and Replacing preserves phase/cycles
unless an explicit restart is requested. Growing prefix media still requires an
identity raw mapping; public Grab settlement must establish that new layout.

Firmware builds use the same histories and input transactions. An explicit
[engine policy](../../platforms/zynq7000/README.md) selects source/loop pools, history
duration and shared reservations. Both enabled and disabled policies are exercised
on both board memory profiles; [firmware evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/grab-replace-firmware.json)
records their actual memory, stack, media and teardown results.

## Production capacity and ownership

The default application prepares sixteen source slots and 128 loops at 48 kHz,
Q=32, stereo, 4096-byte pages and 256 MiB ordinary loop memory. Histories continue
while stopped, independently of input monitoring. Each source is observed once.

| Reservation / limit | Default |
| --- | --- |
| Ordinary media | 256 MiB, preserved |
| Thirty-second input audio rings | 256 MiB; raw stereo samples alone are 184,320,000 bytes |
| Shared audio owners and write workspace | 128 MiB + 32 KiB |
| Shared MIDI payload | 16 MiB; 65,536 bodies and records |
| Per-source MIDI retention | 2,048 events and 512 KiB serialized bytes, subject also to physical slab pages |
| Input callback admission | 256 events / 2,048 packet bytes per quantum; complete packets only |
| Simultaneous writers | Up to eight, with separate resource admission |
| Admitting application input transactions | One globally; another independent input action receives Busy |
| Audio history metadata | Less than 40 MiB, checked without allocation |
| MIDI history metadata/state/workspace | Less than 128 MiB, checked without allocation |

MIDI density/fragmentation can advance actual retained start before thirty seconds.
A source may pad legitimate initial warmup; pressure eviction or a broken capture
epoch cannot be certified as startup silence. A musical gesture beyond available coverage
then refuses without modifying any target or solo peer. Explicit raw adoption
keeps its existing requested-duration padding semantics for available continuous
media; it still refuses a faulted source.

Retained owners charge physical packet-page runs and indexed records, including
hidden clipped roots and deferred retirement. Credit is refunded only when that
storage retires. Finishing a writer releases its future tick reservation while
retaining the immutable cut's actual charge. Shared copies of one version do not
spend the same owner allowance twice. Protected source/staging/edit-path credits
keep independent live capture running even when retained snapshots exhaust all
remaining ownership capacity and optional maintenance is disabled.

Loaded immutable packets pin ordinary upload bases instead of consuming shared
packet payload pages. Source generation/routing/reset fences preserve old exports
and terminate active writers at the last complete cut. Slot removal retires each
pool once; all credits recover at shutdown.

Whole-host allocator measurements and complete callback distributions are in the
[evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/grab-replace.md). They include existing ordinary MIDI, processor,
graph and host-buffer costs beyond the history table. High-density deadline misses
are reported explicitly; finite storage/work admission is not hardware deadline
certification. Eight-channel processor shapes may need an explicitly larger
processor budget; the unchanged default refuses shapes that exceed it. Oversized
64-source/eight-channel histories refuse before allocation at the portable arena
limit.

## Capacity measurements

Run from the configured development environment; write output to local artifacts:

```sh
cargo run --release -p shoop_rt_engine --example input_capacity_benchmark -- 2 128 8 256
cargo run --release -p shoop_rt_engine --example input_capacity_benchmark -- 8 128 8 256
cargo run --release -p shoop_rt_engine --example input_capacity_benchmark -- 2 128 256 32
cargo run --release -p shoop_host_processors --features builtin-fx,oxisynth --example prepared_memory -- 2 512 256
cargo run --release -p shoop_host_processors --features builtin-fx,oxisynth --example prepared_memory -- 8 128 512
```

The high-density case can exceed callback deadlines despite finite work and
allocation safety. Include all callback samples, deadline misses and the complete
prepared host/processor costs when comparing configurations.

## Persistence and recovery

Active and scheduled Replacing follows the existing writer exclusion for save,
mutable imports, timing/family/configuration changes. Already pinned immutable
exports remain readable. Settled media uses existing PCM and exact-MIDI formats;
no history handles, credits, jobs or writer internals enter session documents.
Supported default-Grab settings round-trip, with fresh live IDs and stopped loops
on reload. Revision changes invalidate canonical import caches.

The socket host retains accepted RT operations when the UI or client-local Lua VM
is lost. Reattachment observes the original outcome without replay. Automatic
prefix completion belongs to the render owner and survives a browser main-thread
stall. Shared-memory editing/file flows are covered; shared-memory reattachment
remains the established recovery exclusion. Composite authored Replacing and
FX/dry-wet input editing remain the separately deferred P17 interaction; unrelated
running composites and processor tracks continue alongside primitive input edits.
