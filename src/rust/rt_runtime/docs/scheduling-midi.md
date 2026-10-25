# Scheduling and MIDI

[Overview](../README.md) · [Host integration](integration.md) · [Capacity and performance](performance.md)

## Exact-frame control

Submit `Command::Schedule { at, action, payload }` on `Lane::Normal`. `at` is an absolute engine sample frame, not a wall-clock time or block-relative offset.

| Action | Payload | Application |
|---|---|---|
| `Action::Parameter { node, parameter, value }` | Empty `Box<[u8]>`; finite value | Calls `set_parameter` before that frame's samples |
| `Action::Midi { node, port }` | One complete validated MIDI 1.0 packet | Injects into that node's MIDI input at that frame |
| `Action::Planner { planner, operation }` | Family-typed operation plus bounded borrowed bytes | Immediate or pending domain terminal result |
| `Action::Processor { node, operation }` | Family-typed operation plus bounded borrowed bytes | Immediate typed processor result |

The processor defines parameter IDs, units, ranges, and smoothing. The runtime provides timing, not interpolation.

A scheduled MIDI packet must fit `min(midi_bytes_per_port, budgets.transfer_bytes - resource_io_bytes)`. Larger packets are rejected. Staging shares control-side copy allowance; a packet waiting for this callback's remaining allowance is still **unadmitted**.

```text
Host callback: [ frame 1000 ................................... 1128 )
Schedule:                         gain = 0.25 at 1064

Processor calls: [ 1000 .. 1064 )  │  [ 1064 .. 1128 )
Gain:                  0.50       │        0.25
                                  ▲
                           set_parameter runs here
```

A frame equal to the block's end belongs to the next callback. An event arriving late is admitted at the current callback frame with lateness reported; accepted events are not silently shifted to fit a later block's work budget. Queueing, graph validation, and MIDI payload staging can delay **admission**, so submit with appropriate lead time.

### Two different acknowledgements

```text
submit Schedule
      │
      ▼
try_completion() → Outcome::Scheduled { effective_frame, late_by }
      │            "admitted", NOT "applied successfully"
      ▼
target frame reached → try_execution() → Execution { actual_frame, result, ... }
                                             │
                                      scheduler credit returns
                                      at a callback boundary
```

Always drain both streams. Unsupported parameter IDs return execution errors. A removed target returns an explicit failure rather than addressing a new generation. MIDI injection also requires the node to be in the active graph at execution; parameter changes can target an installed but inactive processor.

### Admission capacity

```text
outstanding event/result capacity = min(
    scheduled_events,
    budgets.events
)

Default: min(256, 64) = 64
```

Every admitted event reserves an external delivery/result slot, even when events share a frame or are far in the future. Unread execution results retain the reservation. Internal planner intervals consume no such slots; the legacy `budgets.subranges` field is ignored. Raising `scheduled_events` alone may therefore do nothing. Use a bounded lookahead window rather than filling the scheduler with an entire long sequence.

For equal timestamps, planner targets precede processor targets. Within a processor target, ordering is parameter, MIDI, then typed processor operation, followed by parameter/port and submission sequence. This preserves ordinary node/kind/port ordering; it is not a global “all parameters before all MIDI” rule across nodes.

A configured planner has one additional, independent recovery receipt; it does not consume the normal capacity above. See [Typed operations and recovery](planning.md#typed-operations-and-terminal-receipts) for pending tokens, queued cancellation, payload limits, and separate requested/delivery/terminal frames. `Execution::operation_result` is authoritative for the domain outcome; generic delivery success alone does not mean the domain action applied.

## MIDI buffers and ordering

```text
raw MIDI byte stream → HOST parses running status / separates realtime bytes
                     → complete packets → MidiBuffer → graph
```

| Rule | Consequence |
|---|---|
| Event count and byte count are separate capacities | Many short messages and one large SysEx stress different limits |
| `push` appends timestamp-ordered events | Rejects an earlier offset; equal timestamps are allowed |
| `insert` maintains stable timestamp order | Useful for fan-in; descriptor shifting has a performance cost |
| Offsets satisfy `0 <= offset < frames` | Processor offsets are rebased into each subrange and back to host offsets |
| Ordinary writer refusal is atomic | A failed write does not append a partial packet |
| SysEx must be complete and bounded | No running status or interleaved status bytes within a packet |

Graph-routed MIDI uses timestamp order, with canonical source ordering for ties. Scheduled local injections follow routed sources at equal timestamps. Host input buffers and processor output buffers must be prepared outside rendering.

## Overload recovery is part of host integration

A partially delivered note stream can strand voices. The runtime therefore uses reset delivery, not arbitrary note dropping, when routing overflows.

```text
internal input overflow
      └─► replace affected input with reserved resets
          └─► processor handles CC120 + CC123 on all 16 channels

reported MIDI overflow
      └─► request recovery on every host MIDI output
          └─► suppress ordinary host MIDI while that output is pending
              └─► host drains reserved recovery sink
                  └─► normal output resumes in a subsequent render
```

Each `MidiBuffer` reserves **32 messages / 96 bytes** beyond ordinary capacity for internal resets. CC120 (All Sound Off) precedes CC123 (All Notes Off) on each channel. Processors must handle these messages and keep doing audio work, including when their ordinary output writer refuses a message.

### Host delivery order, per callback

```text
1. runtime.process(audio, midi)
2. For each output port:
   a. clear its separately reserved, preallocated recovery sink
   b. runtime.drain_midi_recovery(port, sink, frames)
   c. deliver all accepted reset packets from that sink
   d. deliver ordinary output packets, if any
```

A sink with at least 32 ordinary event slots and 96 ordinary bytes can hold one full reset sequence. The host-drain API uses ordinary sink capacity; the sink's internal emergency reserve is not a substitute. Smaller sinks work but require more callbacks. Pre-reserve downstream delivery capacity too: successfully draining into a sink acknowledges those packets, so do not discard them afterwards.

Recovery attempts are capped by `budgets.events` **per output per callback**, shared across repeated drain calls. A full sink preserves the next reset packet. Repeated overflow does not restart an unfinished reset. Use `midi_recovery_pending(port)` to observe completion; audio continues while recovery is pending.

`Command::ResetMidi` may use recovery-lane admission when the normal lane is full. It requests host-output resets; it is not a general reset of arbitrary processor state. Internal reset delivery on routing overflow is separate and bounded by 32 messages per affected port.
