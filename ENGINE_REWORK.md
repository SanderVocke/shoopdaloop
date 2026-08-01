# Engine control boundary

Last updated 2026-08-01.

## Current contract

The application-facing engine boundary uses three separate mechanisms with intentionally different semantics:

1. **Per-object state mirrors** for ordinary reads.
2. **Sequenced fire-and-forget commands** for mutations.
3. **Pending stable controls** for asynchronously created objects.

Ordinary polling does not wait for an audio cycle, query the process thread, or require a globally consistent graph snapshot. A read may observe independently updated fields from nearby process points. Code that requires exact read-after-write ordering must use an explicit `CommandSequence` fence.

## State mirrors

Loops, audio/MIDI channels, and audio/MIDI ports each own an `Arc` mirror shared with their application handle.

- Scalar fields use atomics.
- Floating-point fields use their `f32` bit representation in `AtomicU32`.
- Peaks use process-side atomic max and frontend-side `swap(0)` consumption.
- Event counters use atomic accumulation and frontend-side `swap(0)` consumption.
- Gauges such as active notes use ordinary atomic loads.
- Channel data-dirty state compares a process-published content sequence with a frontend-owned acknowledgement sequence.
- User-facing scalar setters write the accepted desired value into the same atomic mirror after queue admission. This prevents asynchronous initialization/save flows from reverting to an older published value; the process side republishes the authoritative value when the command executes.

The former bulk object snapshot queues and global queued-cycle trust mechanism have been removed. Engine-wide counters and graph-staleness atomics remain because they are independently useful and are not object-state snapshots.

## Commands and barriers

Application mutations enqueue a boxed command and return `Result<CommandSequence, SendError>`.

- Queue admission failure is never silently discarded.
- A full queue logs a warning and retries after space becomes available; parked engines are pumped so retry does not deadlock before driver activation.
- Payload ownership is preserved across retries.
- Commands execute FIFO.
- Scalar commands do not arm graph rebuilding; topology commands do.
- Process-side operation errors are logged or published asynchronously where no operation-status object exists yet.

`wait_for_command` is an explicit barrier for tests and workflows that truly require exact ordering. The graph scheduler also retains named response waits for topology description and prepared-schedule ownership transfer. These exceptional paths must not be used by periodic polling.

## Pending object controls

Loop, channel, and session-port handles contain a typed `Arc<ObjectControl<Id, Mirror>>` with:

- session identity;
- lifecycle (`Pending`, `Ready`, `Failed`, or `Closed`);
- atomically published engine identity;
- creation command sequence;
- asynchronous error text;
- the per-object mirror.

Creation returns after successful enqueue, before an arena index exists. Follow-up commands capture controls and resolve identities only when they execute, so configuration and connections can be queued immediately in FIFO order. Cross-session relationships are rejected. Dropping an otherwise unreferenced pending handle cancels creation through the command's weak reference.

JACK registration records retain the same audio/MIDI port control. Callbacks resolve the index only when the control is `Ready`; unresolved, failed, and closed registrations are ignored safely. Decoupled MIDI ports remain on their own stable `PortId` path because they do not belong to the session arena.

## Complex data and external connections

Audio/MIDI channel content and dummy output capture currently use temporary mutex-backed shared stores. This removes cycle rendezvous from getters but is not the final realtime design: process-side locking, allocation, and full-content copying remain. Detailed risks and future directions are tracked in `REMAINING_ISSUES.md`.

External connection state is cached. Session-backed ports register requests in one cache; at a controlled cadence a worker takes one backend lock, bulk-enumerates the requested ports, and publishes replacement maps. Cache generations prevent stale workers from overwriting explicit mutations. The engine-update thread forwards cached maps to GUI objects asynchronously. GUI connection getters read local cached state rather than entering the backend thread through a blocking Qt call. User-triggered connect/disconnect remains a separate synchronous operation and optimistically updates local cache state.

## FX chains

FX chain ports are created once when the chain is created, retained as pending handles, and cloned by getters. Repeated getters do not mutate topology. Audio input/output and MIDI input/output capabilities are counted separately. Driver sample rate and buffer size are atomically published for plugin creation rather than queried from the process thread.

## Verification and deferred work

`PLAN.md` records the staged implementation and verification gates. `API_INVENTORY.md` records the audited API surface. `REMAINING_ISSUES.md` is the authoritative list of temporary mutexes, process-thread allocations/copies, exceptional waits, lifecycle/removal gaps, and environment-limited tests.
