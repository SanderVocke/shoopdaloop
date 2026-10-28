# Independent prepared example DSP

A portable deterministic fixture for engine composition, not a plugin host. Its
only production dependency is `rt_runtime`; it has no loop, old-engine, backend,
UI, device or browser dependency.

## Signal and preset semantics

Audio inputs are live L/R then playback L/R; outputs are stereo. A prepared preset
selects per-output live/playback coefficients, optional stereo source exchange
and a fixed delay. MIDI has two input ports and one output: merge by frame offset,
then input-port index, preserving each port's packet order. MIDI is not delayed
by the audio delay. Capacities are checked; oversize packets/events or full output
return a compact error, never grow storage. A `GraphRuntime` host translates MIDI
`Full` into overflow diagnostics and reserved recovery while audio continues.
Drain its separate MIDI recovery sink once per output/callback; ordinary MIDI
stays suppressed while recovery is pending.

`prepare` validates limits, dense generation-checked preset slots, finite
coefficients and the initial preset. It allocates and pre-touches the preset bank,
delay ring, queues and cancellation flags off-render. Initial revision is 1.
A successful selection changes the whole prepared configuration and resets delay
history to silence at its exact sample; selecting the same preset is still a new
revision/reset. This is a typed operation with a terminal result, not a float ID.

## Native control and hosting

Destructure `Prepared` into `client`, `render`, `processor` and `meters`. Place the
processor in one public runtime graph. For every nonempty host callback:

1. Call `render.begin_callback(engine_frame, frames)` **once**.
2. Make the host's one actual engine/runtime processing call.
3. Call `render.finish_callback(processing_result)`, even on processing failure.
4. Propagate that processing result separately; never replay a failed callback.

The processor uses the supplied `ProcessBlock` absolute frame, including internal
loop-selected intervals. No new planner, musical clock or event scheduler exists.
The directive/acknowledgement mailbox is private to the two render-owned halves;
the client cannot access it or mutable DSP state. Host callbacks own both halves
and do not run begin/finish concurrently with processing.

`client.select` reserves one normal reliable return. `Ticket::cancel` is best
effort before dequeue. `client.cancel_pending(&ticket)` uses an independent
recovery credit for already-delivered future work and rejects tickets from another
component instance. There is one normal and one recovery slot, with at most two
transport dequeues/completions per callback. A stalled result reader does not stop
an admitted activation. Old ticket/completion handles may keep reusable flags
busy; poll/drop them off-render rather than allocate replacements.

Poll `client.try_completion` off-render. Its generic transport `frame` is callback
start; the component `Receipt::Applied` is authoritative actual sample/revision.
A late request refuses without clamping. Callback-end requests wait for the next
nonempty callback; zero-frame begin/finish calls do not drain or renew quotas.
An activation acknowledged before a later processing error remains applied.

Dropping the client causes future component work to refuse at the next callback;
undeliverable owning transport packets remain retained until stopped destruction.
The enclosing service, not this DSP, owns loop/device stop and MIDI recovery.
Destroy `processor` and `render` only after callbacks have stopped.

## Peak meters

`meters.read_peaks` independently reads/resets each channel's maximum published
magnitude. Publications occur at processing-interval ends. A racing publication
belongs to this or a later read; stereo fields are not a coherent transaction.
NaN samples do not contribute; infinite magnitudes saturate at `f32::MAX`, without
changing audio samples. Meter reads never submit reliable commands or affect DSP.
