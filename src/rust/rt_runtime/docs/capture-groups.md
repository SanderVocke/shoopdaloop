# Prepared capture groups

[Resources](resources.md) · [MIDI indexing](midi-indexing.md) · [Planning](planning.md)

Capture groups coordinate fixed audio/MIDI members around one logical cut. This is generic storage/lifecycle machinery, not a loop clock, renderer, or device adapter.

## Preparation and claims

Create `ResourceStore::with_capture_groups(prepared_config, optional_midi_index_config, CaptureGroupConfig { groups, members_per_group, staging_bytes_per_group })` before rendering. Concrete group/member/staging products are checked against the common native/Wasm ceiling and storage is initialized/touched. Ordinary stores allocate no group rows or staging buffers.

Create resource IDs, then `bind_capture_group(&[CaptureMember { resource, format, midi_events, midi_bytes }, ...])`. Audio members require zero MIDI bounds; MIDI members declare the maximum downstream event count and total payload bytes per interval. Include recovery messages in these bounds when the producer can supply them. Audio staging capacity is `max_frames × channels × 4`; MIDI staging capacity is `12 × midi_events + midi_bytes`. Their sum must fit the declared group region. Nonempty membership, unique resources, formats and arithmetic are validated before any claim changes.

Claims last while the group is managed, including idle/frozen periods. Legacy remove, replacement commit, capture start/stop and raw appends cannot bypass them. A previously prepared upload may still be completed/cancelled but cannot replace claimed content. Binding a running unmanaged capture is refused. Read access and immutable leases remain available.

`release_capture_group` requires an idle group, releases its claims/identity, and preserves active frozen content and external leases. Explicitly remove resources later when their active content is no longer wanted. `capture_group_info` and `capture_group_usage` expose plain-data state/storage counters; the runtime owner can also query group usage between callbacks.

## Take lifecycle

`prepare_capture_take(group, initial_midi_states)` preflights every member, then reserves all take versions, MIDI index rows and final playback-lease slots. Initial states correspond to MIDI members in declaration order. This does not alter old active content or reserve unlimited future data pages. A MIDI member requires an enabled MIDI-index pool.

Preparation returns an opaque `CaptureTakeId`, combining the scoped group identity with a checked per-group take sequence. A cancelled or completed old take cannot retarget a newly prepared one, even at the same frame. Use that token for activation, cancellation and freeze; a stable group identity alone is not take authority.

- `cancel_capture_take(take)` accepts only a prepared take. It returns reserved lease/MIDI credits and retires versions incrementally, preserving old content.
- `activate_capture_take(take, F)` validates all members before replacing any active revision. Each member starts empty at F; old leases retain the prior revisions.
- `freeze_capture_take(take, F, incomplete, output)` requires the common next-frame F and no reserved interval. Supply exactly one empty `Option<Snapshot>` output slot per member. It makes the versions immutable and publishes their already reserved playback leases without a tail copy or whole-content/index build. Ordinary snapshot saturation and exhausted background allowance cannot consume these promises.

Generic freeze permits empty content. A domain's record-to-play operation must separately reject empty playback if required by its semantics. The caller must retain/release each returned snapshot exactly once. Required page/MIDI access is immediately usable through `ReadIndex`. Freeze does not emit MIDI restoration.

## Interval reservation and staging

At F, call `reserve_capture_interval(take, F, N)` before dependent samples render. N must be positive and fit prepared maximum frames. The operation validates common state, next-frame/duration/revision arithmetic, all members' page needs, and worst-case MIDI checkpoint capacity before reserving anything. It returns an opaque `CaptureIntervalId` with a checked sequence. No active content or musical revision is published by reservation.

Audio bytes are exact for N frames. MIDI reserves the member's declared full producer bound, not a guess based on actual content or interval length. This conservative strategy can refuse earlier than exact-content reservation; it is an explicit storage tradeoff, not a CPU quota. Unused reservations are returned on commit or cancellation.

Consumers supply each member once with `stage_capture_audio(interval, member, samples)` or `stage_capture_midi(interval, member, input)`. Audio is interleaved and must match the complete reserved shape. MIDI is a validated arena with offsets in `[0,N)` and packet/event/byte limits within the promise. Each contribution is copied into prepared staging; no input borrow survives the call. Different signal nodes may stage different members.

`commit_capture_interval` first checks that every member is ready and its reservation remains valid. It then appends all actual staged data, updates indexes, publishes every member at the same new duration/cut, and returns unused credit in one exclusive store call. Other processors cannot observe intermediate commits or index rotations. Old partial snapshot tails remain immutable. A missing member yields `Incomplete` without changing published content. `cancel_capture_interval` returns all provisional page credit and resets staged lengths/readiness without clearing buffers or publishing data. Cancelled/consumed tokens cannot affect another interval.

Exceeding a producer promise or malformed staging after reservation is an interface/programming fault, not normal physical exhaustion to report retroactively at F. The transaction remains cancellable, but dependent audio already rendered is not rolled back. Storage exhaustion belongs in preflight; normal failure handling must decide/freeze the preceding common prefix before crossing F.

## Runtime capabilities and time

Planner operation/plan/finish contexts now contain `PlannerResourceIo`. It dereferences to existing `ResourceIo` for compatible read/optional-publication methods and additionally exposes `capture()` returning `CaptureBoundaryIo`. That capability provides prepared take/interval lifecycle methods but no arbitrary upload, resource creation/removal, reclamation or budget reset.

The runtime stamps the capability with its current boundary and ceiling. Activation/freeze must use that frame; reservation must start there and end no later than the ceiling; commit must happen at the reserved end boundary. Operation and finish contexts have a zero-width planning window, so they cannot reserve samples beyond their boundary. Plan contexts use the next external-action/host ceiling. Standalone store owners can construct `planner_io(frame, ceiling)` explicitly and are responsible for those authoritative values.

Ordinary processors retain `ResourceIo`, which cannot acquire planner lifecycle authority. `ResourceIo::render()` provides `stage_audio`/`stage_midi` plus existing required reads and unmanaged single-resource appends. Managed members refuse those raw append methods. Planner `finish` commits staged contributions after traversal and before advancing domain state through the interval.

A planner must handle preflight refusal before rendering: release other provisional reservations, latch a previously unlatched group failure, re-preview affected decisions, then commit the stable boundary. Termination follows from the finite prepared group/activation set, not a retry counter. The generic store does not implement musical arbitration or silently stop unrelated audio.

## Accounting and current limits

Group data work never consumes/replenishes background allowances. Lifecycle, reservation and staging/commit operations contribute to render diagnostics. The current safe-borrow implementation uses a fixed 64-byte stack copy between group staging and resource pages: actual media data therefore incurs three counted copies overall (input to staging, staging to stack, stack to pages). Checkpoint-state copies have their own counters. This is a correctness-first implementation, not a zero-copy or performance-improvement claim.

Optional active-capture snapshots still use background allowances and may need private tail pages; group final leases are a separate prior reservation. Retirement stays incremental. Semantic frame/revision/token exhaustion is explicit; diagnostic overflow cannot change results.

Store and synthetic-planner tests cover exact-boundary failure and interval partitions. A generated independent ownership/content model checks 2,048 actions across 64-page and 16-page stores, including conservative reservations, optional private-tail snapshots, stale currentness, cancellation, retained old revisions, and complete pool recovery. Real control-transport disconnects are tested at fourteen lifecycle cuts, with a recovery stop queued before disconnect. This does not imply automatic musical stop on transport disconnect.

After callbacks have stopped, `GraphRuntime::into_resources()` consumes the runtime off-render and returns its store, destroying the other runtime owners. It does not automatically cancel domain reservations or release leases known only to the planner: settle those first, or retain their handles for explicit cleanup. Retained active resource content remains owned by the returned store until removed or the store is destroyed. The disconnect tests verify off-render processor destruction and full pool recovery after explicit cleanup.

Full loop integration, MIDI restoration emission, actual loop AudioWorklet execution, and final delivery evidence remain separate requirements. Generic Stage 4 verification is not end-to-end loop acceptance.
