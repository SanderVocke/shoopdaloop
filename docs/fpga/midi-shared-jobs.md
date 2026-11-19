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
| Median quantum | 19.8 µs | 63.9 µs |
| 99th percentile quantum | 332.7 µs | 625.7 µs |
| Full 30-second recording workload | 1.05 s | 3.52 s |
| Full playback workload | 2.42 s | 6.85 s |
| Optional 128-target grab | 122.5 µs | 85.4 µs |

These single cloud runs had different host load; the final run overlapped the
workspace suite. Their timing difference does not isolate algorithmic cost.
The added bounded work consists of compiled per-channel work and queue/dispatcher
traffic, now explicit for every quantum. Inactive ownership/playback jobs are
skipped during recording and stopped modes. Storage algorithms and prepared MIDI
bytes remain unchanged; the common queue additionally reserves the above finite
job storage. SP omits claim/read jobs for channels that are not playing,
which avoids unnecessary dispatch during recording or stopped operation. It is
a real software cost of this architecture, not evidence that
memory lookups or whole-state comparisons grew. The median is about 9.6% of a
32-frame/48-kHz quantum's 667-µs time allowance for this MIDI-heavy fixture, which
has only a trivial audio path. This does not qualify a full audio session's CPU
budget or its worst case. Cloud timing extremes are noisy; no wall-clock or
128-simultaneous-grab threshold is used. Job/work/allocation checks provide the
repeatable evidence. Further optimization can target inactive scheduling and
job dispatch if needed, without changing the storage model or imitating RTL.

## Shared SP/JP hardware

The production composition is `midi_audio_pipeline_axi`. Its compiled SP executes
one mixed program, and the ordinary JP dispatches audio and distinct MIDI jobs
from one queue. `midi_job_service` supplies the prepared event/history/state
operations behind JP. It has no command that runs an entire quantum and accepts
no external table of settled loop records. The older `midi_runtime` and wrapper
remain historical component fixtures; the new composition does not instantiate
them.

`midi_audio_pipeline` exposes a word-memory boundary for MIDI simulation;
`midi_audio_pipeline_axi` connects it to a second ordered AXI3 DDR port. Audio
keeps its existing sample and metadata caches. MIDI has no cache. The two DDR
ranges must be disjoint; arbitration at the DDR controller is part of the SoC
embedding. Both 32- and 64-bit MIDI AXI endpoints have functional tests. Large
states, snapshots, comparisons, history/checkpoints, views and event payloads
live in DDR. The small primary context table and per-quantum merge-head table
fit in BRAM. One head per context is working data, reset at each quantum.

Prepared context indices are stable across program relocation, and every channel
job also carries the loop identity/generation checked against its binding.
Capture completion resolves its view before later playback jobs consume it.
Only accepted output advances cursors, activity and comparison state. The MIDI
result is separate from fatal audio/pipeline errors. Fatal errors fence future
quanta, notify the MIDI services and leave explicit recovery draining available.

An explicit `with_midi(&[])` Rust program keeps input/history work running with
no bound MIDI loops. Context removal requires stopped, fully cleaned-up execution;
exports retain their own content reference. Remove the context from compiled work
before reusing its slot for another loop generation.

### Content publication

The internal control boundary is available between quanta. Group grab, replace,
duplicate and clear first prepare/publish MIDI ownership. The service then raises
`midi_publication_valid` and holds both its completion and the next quantum.
The embedding validates the target loops, updates their SP metadata through the
existing loop-configuration interface, and acknowledges `midi_publication_commit`.
Only then does the control completion become visible and processing resume.
This keeps one SP owner of loop mode, length and revisions. Program loading and
this metadata handoff must be serialized by the embedding. The Rust `Pipeline`
content methods perform the corresponding validation/publication directly.
Application transport/board MIDI enablement remains outside this profile.

### Mapping and work

Rust keeps ordinary functions and a bounded merge heap. RTL uses a small head RAM
and bounded scans. Activity initialization copies only owned MIDI channels plus
the shared reset flag; inactive activity needs no clear because the next start
overwrites every field that can be used. Bindings still own the same complete
musical state. Bounded arena offsets use 32 bits after validating capacities;
arena bases and musical timestamps remain 64-bit. Generation checks precede
addressing retained handles. This avoids unnecessary wide arithmetic and DDR
transfers without making Rust simulate hardware registers.

The 64-stereo-plus-MIDI compiled profile has 1,030 instructions, 1,158 worst-case
jobs and 384 audio buffers. It uses 2,048-entry program/job memories. Audio read
hints have a separate 256-entry bound: at most one SP and one JP hint per each
of 128 audio reads. MIDI jobs cannot enlarge the audio hint queue. Existing
profiles retain their original default hint capacity.

A delayed-memory two-loop compiled test compares exact jobs, audio samples,
MIDI output and accepted destination state at all five quantum sizes. It also
covers partial output/recovery, fenced grouped history publication/clear and a
fatal audio fault with recovery. Existing full MIDI service scenarios cover
transfers, controller/pedal state, multiple inputs, capacity, 128 bindings and
ownership. AXI tests retain a virtual arena above 4 GiB to check that narrowing
offsets does not truncate arena bases.
