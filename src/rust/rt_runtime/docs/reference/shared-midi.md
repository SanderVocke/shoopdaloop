# Shared MIDI history and independently editable takes

`resource::shared_midi::MidiStore` prepares immutable packet storage and a
persistent chunk index in a `ResourceStore`. It can also be registered with
`ResourceStore::prepare_midi_store`, behind ordinary resource IDs and snapshots.
All capacities and storage are allocated and touched during preparation.

## History and ownership

`append` accepts an explicit contiguous frame interval and complete MIDI packets.
Packets may cross physical pages, including SysEx. Equal-frame arrival order is
stable. Positive empty intervals advance time; zero-frame empty intervals do not.
Admission validates the whole input before publishing history. Frame positions
and chunk identities are monotonic `u64` values.

Coverage reflects both the time horizon and chunk pressure. When density forces
an old chunk out, coverage advances past its entire last frame: no partial set of
simultaneous originals is advertised as complete. A failed append leaves committed
history unchanged. The caller must continue observing input independently and
call `discontinuity(next, observed_state)` after a missed interval. This starts a
new complete segment; it never authorizes callback replay. Old history retirement
may need maintenance before new storage admission recovers.

`grab(start..end)` retains a root, clips/rebases event positions and owns the
projection strictly before the start. Originals at the start remain originals at
local zero. No packet body is copied by grab, fork or export. `fork` creates an
independent editable owner; `export` pins an immutable revision. Read indexes are
borrowed. Take edits invalidate old cursors; immutable export cursors remain valid.
Released handles invalidate immediately, before their bounded retirement finishes.

## Replacement and readiness

`prepare_replace` owns its callback input and validates a fixed-length interval of
at most one loop duration. A wrapping interval is one transaction. Empty input
erases originals in that interval. Outside bytes/order and duration are preserved.
There are no synthetic state restoration messages at edit joins. An inserted
controller affects the suffix; deleting a note-off can leave a note held.

Replacing logical zero refreshes initial state from the supplied projection before
the input interval, including originals earlier than zero in the same callback.
Packets exactly at zero remain originals. A zero-length edit keeps the revision,
contents, initial state and time unchanged.

The committed take remains usable during `Building`. `progress_edit` consumes at
most its requested event quantum, bounded by `Config::edit_steps`, using reserved
render work independent of ordinary background allowances. Unchanged prefix
checkpoints stay shared. Affected suffix checkpoints are rebuilt from the edited
stream; packet bodies stay shared. Total preparation can scale with that suffix.
There is no guarantee of readiness at the next callback.

After packet preparation, one separate bounded progress step clips the final root
along boundary index paths. It consumes no source/inserted records and does not
replay the suffix. Readiness includes this finalization; its index-work bound is
independent of suffix length. The immutable source pin keeps discarded original
subtrees alive, so immediate transaction cleanup visits only the changed paths.

Only `Ready` transactions can commit. Commit checks the expected revision and
publishes root, initial state, duration, index and revision together. Generic
resource commits also check that a full-loop upload has not superseded the target
version. Cancellation/refusal preserves the committed revision. A transaction owns
a source pin and a private revision; their charges return after retirement.

## Bounded work and capacity

Each checkpoint chunk contains at most eight descriptors and a full `MidiState`
projection before its first event. A seek uses binary chunk positioning and replays
at most eight packets, excluding originals at the target. EOF includes all earlier
events. State stores the existing channel/key protocol projection, including its
untracked-parameter diagnostic; it cannot reconstruct arbitrary vendor/device
effects from packets preceding the cut.

Configure history duration, chunk ceiling, owner slots, physical pages, incoming
events/bytes, packet size, transaction slots and progress quantum separately.
Chunks must be a power of two. Imported duration is independent of the history
horizon; its descriptor count must fit the chunk ceiling. Changed suffixes pack
eight descriptors per chunk. Capacity bounds fragmentation and may refuse an edit
even when its raw packet bytes would fit.

Payload ownership is separate from checkpoint ownership. Each admitted owner
conservatively charges worst-case packets for the chunks touched by its window,
including invisible events in partial boundary chunks. The persistent root is
clipped along boundary paths without copying packet bodies. An empty take reserves
one chunk credit. Live history and callback scratch have reserved credits.
Owner admission may refuse before every metadata slot is occupied. This favors
predictable admission over dense memory use; it is not application memory policy.
`Config::capacity(prepared_runtime)` validates and reports the prepared shape
without allocating or reserving pages. `MidiStore::capacity` reports the same
shape after preparation. Checkpoint/index tables follow the total retained-chunk
credits permitted by the page reservation. Handle/initial-state arrays use the
smaller of configured view slots and those credits. Unreachable owners do not
multiply those tables; transaction slots retain their configured capacity.
Replacement admission reserves the larger original/result root and a source pin;
commit transfers credits to the result and retiring original. Shrinking roots
are clipped before publication, and their unused credits return after settlement.
The estimate reports the owner ceiling and
checkpoint/state and prepared table/scratch bytes separately;
common arena storage and allocator overhead are additional. `work` reports record,
state-copy, index-publication and reclamation work; ordinary resource index/render
counters also account virtual byte positioning and transfers.

Capture, packet reads, cuts, preparation steps and ready commits allocate/free
nothing and do not block. Their bounds depend on configured input limits, index
depth, eight-event chunks and the requested quantum. Virtual byte reads transfer
at most one configured page, rebasing headers even across split reads. Required
I/O uses render work; optional byte exports still obey background allowances.

The original Linux x86_64 eight-chunk stress fixture (16-byte pages, eight view slots, two
transactions, 48-byte packets and a 4,000-page reservation) reports 2,419,200
bytes of state projections and 3,550,384 bytes of prepared metadata/state/scratch
in total, plus 64,000 payload bytes. The current projection is 15,120 bytes.
These intentionally small pages expose fragmentation and cross-page behavior;
full checkpoint projections dominated that configuration. Those measurements
precede clipped root charging; current CI prints the prepared layout separately.
Storage grows with the history/retained-chunk/transaction ceilings even when few
packets have arrived. `Usage` distinguishes owner count from retained-chunk credits.

## Loaded bases, ordinary resources and teardown

Registered stores provide `append_midi_history`, `grab_midi`, `fork_midi` and
explicit resource replacement prepare/progress/commit/cancel operations. Each
ordinary snapshot owns an immutable export. Byte cursors, MIDI cursors, seek and
initial-state access use the existing resource interface. Snapshot retention,
source removal and superseding uploads preserve old exports.

For an ordinary immutable indexed MIDI upload, `prepare_midi_import` retains the
base; `progress_midi_import` builds descriptors and checkpoints in bounded steps;
`finish_midi_import` exposes the complete editable resource. Packet bodies remain
in the original immutable upload. Failed preparation can be cancelled, including
after partial progress. Capture snapshots and cross-store shared bases
refuse; shared snapshots fork within their originating store.

Final release queues bounded node, descriptor, page or base work. Resource
maintenance rotates between ordinary, shared-audio and shared-MIDI retirements.
After all owners and transactions settle, cleanup retires history and returns
the page reservation. Prepared objects remain allocated until host teardown.
These contracts establish bounded library operations, not physical-device
deadline certification or application P04/P05 behavior.

## Loop and persistence proof

The scalar resolver fixture is a fixed Q=1 reference. Production engine fixtures
exercise the ordinary graph/domain at Q=1/16/32/64/128 with whole, one-frame and
irregular host callbacks. They retain packet/sample/state oracles, common completed
cuts, fair bounded private preparation and coherent tick selection. Node-pair,
Adopt/Select, export/MIDI-initial, family and ConfigurationReplace paths retain
ordinary shared snapshot ownership.

The mixed fixed-engine sequence retains original and published exports, saves
session/exact/WAV data, reloads indexed bases, edits them independently and follows
family/configuration replacement with complete teardown. SMF retains its existing
quantized timing warning and drops realtime reset. Identity conversion and explicit
rate/Q conversion keep separate contracts. See the
[fixed shared-history contract](../../../shoop_loops/docs/shared-history.md) for
source generations, watermark faults, callback budgets, cleanup and measurement
scope. Application P04/P05 mode flags and parity gaps remain open.
