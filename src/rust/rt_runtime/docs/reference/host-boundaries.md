# Host boundaries

[Book](../README.md) · [Reference](README.md)

The runtime processes borrowed buffers; it is not a device driver, browser application, MIDI stream decoder or backend migration. Native/Wasm hosts own adaptation, bounded ingress, device lifetime and stopped destruction.

## Single-owner preparation and callbacks

Prepare/touch processor state, runtime stores, host buffers and reusable payload owners off-render. Fix signatures and limits before processing. Move the render owner to its callback context; move the control endpoint to its single producer. Do not concurrently borrow a live runtime's store/catalog/diagnostics from another thread.

For nonempty calls, supply the expected absolute frame, bounded frame count and exact host port shape with sufficient channel lengths. MIDI offsets are relative to this call; processor intervals see their own relative offsets. Zero calls are inert, not a way to admit/flush work. Invalid shape/frame calls can clear bounded output without advancing; later processing faults can advance time and publish state before returning an error.

After a fault, follow explicit domain policy. Never replay frames, fabricate finish/receipts or assume silence means nothing changed. [The callback adapter](../part-1-process-audio/04-connecting-an-audio-callback.md) demonstrates only its own FailOnce/NoPlanner continuation.

No-heap claims apply to scoped successful ownership-preserving paths, not arbitrary payload construction/drop, locks, I/O, third-party DSP or JavaScript allocation/GC. Finite native/Wasm storage is not a device-deadline certificate.

## Complete MIDI packets and recovery delivery

Decode incoming streams outside processing into complete validated MIDI 1 packets. Handle fragmentation/running status/SysEx framing before bounded admission; do not make a processor wait for another fragment. Event count and payload bytes are independent capacities, and offsets must lie inside the host block. Stored MIDI serialization is a different framing layer from live packets.

Owned MIDI buffers reserve internal reset storage separately from normal events. Overflow can request recovery while audio continues; ordinary affected output remains suppressed while host recovery is pending. The host must deliver the complete 32-packet reset sequence, not merely observe a counter or drop excess resets.

After processing, use `midi_recovery_pending`/`drain_midi_recovery` with a prepared downstream sink, at most once per output per callback. Full sinks retain the exact next reset packet; the drainer acknowledges only successful insertion and continues on later nonempty callbacks. Repeated overflow does not restart an unfinished sequence. Do not merge ordinary output ahead of pending recovery or confuse internal reset storage with downstream capacity.

The [MIDI example](../../examples/midi_packets.rs) exercises full-sink retry and all reset packets. Generic recovery does not reconstruct arbitrary synthesizer voices/effects/device state, perform loop restoration or certify that a stopped external device received cleanup.

## Independent Wasm instances are separate owners

A main-thread Wasm instance and an AudioWorklet/Worker instance do not share Rust state merely because they load the same module. Do not send pointers, Vec/Box layouts, trait objects, borrowed slices, local StageIds or Copy payloads as an implicit cross-instance protocol.

A real bridge needs checked encoding, bounded lengths/queues, local owner pools, generation/identity routing and reliable return credits. Preserve all frame bits rather than assuming every Rust `u64` fits a JavaScript number. Decoder validation and local catalog materialization still precede runtime validation/publication. A prepared local descriptor is not an authenticated wire message.

Transfer/queue credits bound a cooperating producer's protocol, not the browser's entire MessagePort queue against a hostile sender. Structured cloning, message scheduling and GC are outside Rust callback guarantees. No shared-memory assumption, MessagePort bypass or production codec is supplied by the generic planner/resource APIs.

## What the existing host fixture proves

[`tests/rt_runtime_host`](../../../../../tests/rt_runtime_host/README.md) is a separate test-only workspace with native/offline/two-thread and real worklet scenarios. Its `RTR1`/`RTJ1` formats, checked-in journal and recorded browser measurements belong to its existing **NoPlanner** scenario. A journal replay is not a new browser run.

It is not a loop host, generic planner/typed-operation wire recipe, production device adapter or proof of unrestricted live construction. Package-Wasm tests and offline loop examples are also not independent loop AudioWorklet evidence. The separate [loop fixture](../../../../../tests/loop_engine_host/README.md) supplies a small record/play/FX scenario through direct, primitive-wire and bounded two-thread drivers plus actual non-isolated offline/live AudioWorklets. Its codec preserves full frame bits and separate admission/domain outcomes across independent ordinary Wasm memories. The browser checks actual downstream stereo output, full MIDI/receipts, canonical semantic timeline journals, fixed memory, specific Rust-origin processor/heap failures and successful stopped pool recovery. Live commands are staged before an acknowledged engine-clock start. A separate prepared graph descriptor receiver has native facade and public-Wasm binding coverage. An opt-in native command bridge and public Node/Chromium bindings stage/activate/release graphs with bounded owning results. Additional actual offline/live graph sessions now transfer descriptors/commands/owning returns through MessagePort and compare downstream samples and complete MIDI/semantic/activation results with native replay. Their audible one-sample Ready checkpoint and explicit engine pause/resume do not prove continuous wall-clock live editing or deadlines. Native bounded two-thread graph payload transport remains open; graph binding observations remain distinct from those downstream Web Audio sessions. Broader live-ingress, program/resource payload, export and composite scenarios remain host work. No loop benchmark or production browser adapter is supplied. Native replay and mere Wasm compilation remain distinct from fresh browser execution.

Use the fixture's actual commands/provenance in its README and [repository host reference](../../../../../docs/rt_runtime/HOST_FIXTURE.md). Do not retrofit new domain semantics into an old journal or report unperformed platform/benchmark campaigns as passes.

## Shutdown crosses the device boundary too

For graceful shutdown, stop new application work, explicitly settle/cancel requests, freeze actual prefixes, deliver needed MIDI cleanup and collect owning/execution results while callbacks can still run. Then stop/join the callback owner before endpoint/processor/store destruction.

For intentional abandonment/fault, stop first and retain the ledger needed for explicit storage recovery. Disconnected queues and Pending requests are not promised invented terminal outcomes. `into_resources` drops other owners; `into_planner_and_resources` additionally preserves the planner ledger, not device state.

The loop facade's stopped permanent quiescence is separately attributed policy, not generic rollback or permission to resume a consumed engine. Rust destruction after a device stops cannot send it reset messages. [Shutdown ownership](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md) distinguishes these responsibilities.
