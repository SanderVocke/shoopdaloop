# MIDI execution through SP and JP

The state processor (SP) owns musical time and loop modes. Its compiled work pass
places MIDI and audio jobs in one finite queue. The existing job processor (JP)
drains that queue; MIDI services receive explicit operands, not access to SP.
The event store, history, state interpreter and incremental comparison algorithms
remain reusable software components. Rust functions remain direct computations.

## Execution contract

A quantum first settles all loop states in the existing synchronization pass.
The work pass orders recovery, capture completion/start, input observation,
capture append, old-owner cleanup, ownership arbitration (established owners
first), restoration/read windows, ordered output, cursor completion and bounded
maintenance. Each channel operation names a stable prepared context slot and
its loop identity. Musical positions and lengths come from SP. Jobs resolve
content through the context at execution time, so a completed recording can be
played by a later job in the same queue without a stale view handle.

Queue capacity is reserved for the maximum admitted work before SP advances.
Enqueuing work expresses intent. Writer, restoration and output progress change
only when execution succeeds. Output acceptance drives destination state,
incremental differences, activity and cursor advancement. MIDI capacity/refusal
is reported separately from a fatal audio/pipeline fault. All captures obtain
pre-event starting snapshots before input observation. Cleanup for every old
owner precedes restoration for any new owner. Output selection retains the
source context and orders by timestamp then stable context identity, including
across destinations.

Grouped edits use prepared reservations and generation checks. At a completed
quantum boundary, publication changes bindings and SP lengths/revisions together.
Exports retain independent view references. Retirement and transfer preparation
are bounded and cannot consume capture/history reservations.

## Migration baseline

The starting source is `0e26c783b2bd1662fd156483e5658cf8b795430d`.
The old standalone Rust session is frozen under `midi::oracle` with `cfg(test)`;
it cannot be selected in a production build. Existing MIDI, pedal, content,
allocation and independent native loop tests remain behavioral requirements.
The old standalone RTL and its Vivado measurements remain historical evidence.
The reference/compiler baseline suite passed locally. A descriptive release-mode
3,000-event/128-binding workload is retained outside the repository during work.

## Rust implementation and hardware-mapping review

`LogicalStateProgram::with_midi` appends MIDI operations to the compiled audio
work pass. It sorts bindings by stable context slot once, during preparation.
Allocator relocation changes loop operands and leaves context identities intact.
Bindings carry input/destination/channel ownership in prepared MIDI resources.
Application MIDI graph wiring is still disabled. A compiled mixed image is a
supported reference entry point; the existing application host remains separate.

The common JP dispatcher borrows the MIDI execution resources just as audio jobs
borrow their memory implementation. No MIDI service reads SP loop tables. SP
sends positions/modes in jobs; content handles are resolved at execution, after
capture completion. `MidiSession` now owns services and contexts, not scheduling.
Its context mode/cursor records executed work and checks synchronization with SP;
it does not advance musical loop time independently.

| Compiled phase | JP operation | Work and hardware mapping |
| --- | --- | --- |
| Begin | Recover destinations; reset per-quantum ownership/head state | At most 32 accepted recovery packets per destination; small finite status/ownership tables. |
| Capture | Finish an old writer, or begin a new one | Fixed descriptor work and shared pre-event snapshot seed; block releases use deferred retirement. |
| Observe | Observe input and advance history | Bounded admitted input count; checkpoint/replay and interpreter retain existing finite work limits. |
| Append | Append this channel's input window, then its silent duration | At most the admitted input count; event block/offset lookup; duration stays 64-bit. |
| Cleanup | Continue this context's cleanup | Accepted-output transition progress; bounded channel/note/pedal enumeration. |
| Claim established/new | Reserve a channel mask at a destination | Small bit operations; no duplicate loop-mode decision or second loop clock. |
| Playback | Restore as needed and expose a timestamp-bounded read cursor | Incremental comparisons, immutable target, one head per context; no full sequence sort/copy. |
| Output | Merge heads and submit packets | Timestamp then context order, retaining source identity; only accepted packets commit progress. |
| Complete | Publish executed mode and cursor progress | Small context update, no content copy. |
| Maintain | Release retired content and renew transfer grants | Explicit maintenance budget. |

The conservative queue reservation is **four global jobs plus eight per bound
MIDI channel**, in addition to the audio program's own bound. Capture reserves
two mutually exclusive possibilities; actual emission is usually smaller. The
128-binding reference needs at most 1,028 additional entries. A rejected capacity
or malformed binding/order is detected before musical state advances. Prepared
processing, image switching and control publication remain allocation-free.

Scalable event blocks, views, snapshots, instrument states, activity and comparison
maps map to DDR. A hardware job can operate on one selected context and a small
working state; it need not instantiate an interpreter per loop. Small queue/
working tables can use BRAM with explicit capacity accounting. Sequential
memory-access rules belong to the RTL service port, not the Rust interpreter.

Address calculations mostly use base + bounded index * constant stride. Register
selection/index values only as wide as validated capacities allow, while retaining
64-bit musical timestamps. In hardware, factor repeated address calculations,
register long paths and choose sharing or parallelism according to throughput and
area. The Rust code uses normal functions, loops, arrays and a bounded binary heap;
it does not emulate those circuits. The hardware merge may use bounded head scans
rather than Rust's heap if it preserves ordering and accepted-output semantics.
Bulk state updates already have explicit interpreter progress; ordinary small
Rust computations do not need extra state machines. DDR latency and shared-JP
service time must be measured separately from clock closure.

### Descriptive CPU check

One release-mode run of the existing 128-binding, 3,000-events-per-loop workload
on this cloud machine, before/after the shared path:

| Measurement | Frozen path | Shared path |
| --- | ---: | ---: |
| Prepared MIDI storage | 34,457,072 bytes | 34,457,072 bytes |
| Median quantum | 19.8 µs | 43.8 µs |
| 99th percentile quantum | 332.7 µs | 403.2 µs |
| Full 30-second recording workload | 1.05 s | 2.72 s |
| Full playback workload | 2.42 s | 3.48 s |
| Optional 128-target grab | 122.5 µs | 126.3 µs |

The added constant cost comes from compiled per-channel work and queue/dispatcher
traffic, now explicit for every quantum. Storage algorithms and prepared MIDI
bytes remain unchanged; the common queue additionally reserves the above finite
job storage. It is a real software cost of this architecture, not evidence that
memory lookups or whole-state comparisons grew. The median is about 6.6% of a
32-frame/48-kHz quantum's 667-µs time allowance for this MIDI-heavy fixture, which
has only a trivial audio path. This does not qualify a full audio session's CPU
budget or its worst case. Cloud timing extremes are noisy; no wall-clock or
128-simultaneous-grab threshold is used. Job/work/allocation checks provide the
repeatable evidence. Further optimization can target inactive scheduling and
job dispatch if needed, without changing the storage model or imitating RTL.
