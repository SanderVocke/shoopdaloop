# Appendix A. State processor instructions {.unnumbered}

The base encodings are from RTL revision `cc410f27`, with the optional MIDI
expression extension from `462ce74e` (Appendix E) and the bounded composite
audio extension. Bit ranges
are inclusive and numbered from the least-significant bit. Fields named
`u16`, `u32` or `u64` are unsigned at that width. Signed audio samples and
gain values use two's-complement Q8.24. In the tables, “—” means the encoder
should write zero. For a byte-oriented copy of a packed word, put the
least-significant byte first.

## Instruction word {.unnumbered}

An SP instruction is 96 bits. Its opcode selects the operation.
`run_sync_count` counts the loop visits, including `ProcessLoop` and
`ProcessCompositeChild`. The full instruction count also includes inline
schedule rows. A work instruction can use a loop once its visit stamps it
for the current quantum.

| Bits | Field | Width |
| ---------------------- | ---------------------------------------------------------- | -------------------: |
| 7:0 | Opcode | 8 |
| 23:8 | A0 | 16 |
| 39:24 | A1 | 16 |
| 55:40 | A2 | 16 |
| 71:56 | A3 | 16 |
| 87:72 | A4 | 16 |
| 95:88 | Reserved except for MIDI Playback / State | 8 |

A RAM address in the next table is an entry index in the 129-bit node-state
RAM. Source and destination are JP scratch-buffer IDs unless the table says
port. Gen lo and gen hi are the low and high halves of the loop's 32-bit
generation.

| Opcode | Instruction | A0 | A1 | A2 | A3 | A4 |
| --------- | ------------------------- | ------------- | ------------- | ------------- | -------------- | ------------- |
| `0x00` | ProcessInput | Input port | Destination | — | — | — |
| `0x01` | ProcessPlayback | View addr | Position addr | Length addr | Enable addr | Destination |
| `0x02` | ProcessGain | Ramp addr | Source | Destination | — | — |
| `0x03` | ProcessMix | Left source | Right source | Destination | — | — |
| `0x04` | ProcessCapture | View addr | Enable addr | Source | — | — |
| `0x05` | ProcessOutput | Source | Output port | — | — | — |
| `0x06` | LogicalPlayback | Loop slot | Gen lo | Gen hi | View addr | Destination |
| `0x07` | LogicalCapture | Loop slot | Gen lo | Gen hi | View addr | Source |
| `0x08` | ProcessSilence | Destination | — | — | — | — |
| `0x09` | ProcessConstant | Value bits 15:0 | Value bits 31:16 | Destination | — | — |
| `0x0a` | ProcessArithmetic | Left source | Right source | Destination | Mode | — |
| `0x0b` | ProcessSmoothing | Loop slot | Gen lo | Gen hi | Control addr | Source |
| `0x0c` | ProcessLoop | Loop slot | Gen lo | Gen hi | Inline source slot or 0 | Inline flag |
| `0x0d` | ProcessHistorySmoothing | Loop slot | Control addr | Source | Destination | History index |
| `0x0e` | MIDI Input | Input port | Destination list | — | — | — |
| `0x0f` | MIDI Playback | Loop slot | Gen lo | Gen hi | Sequence and cursor | Channel mask |
| `0x10` | MIDI Append | Loop slot | Gen lo | Gen hi | Sequence | Source list |
| `0x11` | MIDI Merge | Left list | Right list | Destination list | — | — |
| `0x12` | MIDI Output | Source list | Output port | — | — | — |
| `0x13` | MIDI State | Owner slot | Gen lo | Gen hi | Sequence/input | Output mask/channel bits |
| `0x14` | MIDI PrepareCapture | Loop slot | Gen lo | Gen hi | Sequence | — |
| `0x15` | MIDI MergeOutput | Output port | Packed lists | Packed lists | Packed lists | Packed lists |
| `0x16` | ProcessCompositeChild | Child slot | Gen lo | Gen hi | Inline source slot or `0xffff` | Number of following schedule rows |
| `0x17` | ProcessInputMonitor | Recorder slot or `0xffff` | Gen lo | Gen hi | Ramp addr | — |

Other opcode values cause `INVALID_PROGRAM` (`0x03`). Additional operand
interpretation follows.

- **ProcessPlayback and ProcessCapture.** The view, position, length and
  enable operands address Word entries. A nonzero enable selects the
  operation. Positions and lengths in the raw playback instruction are in
  audio chunks. Disabled playback emits Silence, and disabled capture emits
  no descriptor. Enabled raw capture emits Capture.
- **LogicalPlayback and LogicalCapture.** SP obtains the mode and render
  position from the stamped loop record, or from the explicit record port in
  a fixture with `run_sync_count=0`. SP converts the frame position and length
  to chunks for audio playback. Playback emits Silence when the loop is
  inactive or at the held endpoint. The first quantum of a logical capture
  emits BeginCapture followed by Capture.
- **ProcessGain.** The RAM entry is a Ramp. SP copies the old ramp into the
  job and updates elapsed to `min(elapsed + Q, duration)` when enqueueing it.
- **ProcessInputMonitor.** After the sync pass, the compiler first clears
  a track's shared monitoring gain with A0=`0xffff`. It then emits one row
  for each primitive loop on the track. A loop that is recording sets the
  gain to unity, and other loops leave the gain as they found it. The usual
  gain jobs apply that value to each monitored input channel.
- **ProcessArithmetic.** A3 uses the JP arithmetic modes in Appendix B.
- **Smoothing.** For `0x0b`, the control Word supplies destination in bits
  15:0, history index in 31:16 and restart duration in frames in 63:32.
  For `0x0d`, A3 and A4 supply destination and history index directly.
  The control Word supplies only the duration in bits 63:32. The managed
  loop-RAM path checks the loop generation for `0x0b`. The `0x0d` form
  uses the compiled loop slot and validates the record and boundary stamp.
  SP derives playing from loop mode and restart from render position zero.
- **ProcessLoop.** In the RAM-source form, A3=A4=0 and SP reads the sync
  source slot, valid bit and source generation from loop RAM. In the inline
  form, A4=1 and A3 names the source slot, with `0xffff` meaning no source.
  The inline form still checks the target generation and current-boundary
  stamp of an external source. Source generation checking uses the
  RAM-source form. Bits 95:88 must be zero in either form.

<a name="composite-schedule-encoding"></a>

## Composite schedule rows {.unnumbered}

`ProcessCompositeChild` uses the inline sync-source form. Its A4 counts the
contiguous 96-bit rows following the instruction. Bit 88 is set when the child
has only handoff rows. Bits 95:89 are zero.
Each parent contributes one header followed by its spans in increasing start
order. Parents appear in the compiler's action priority order. The engine
reads those rows as data during the child visit and skips them on normal
instruction fetch. The parent must be a valid composite with matching generation
and a processed stamp for the same boundary.

| Row | Bits | Field |
| --- | --- | --- |
| Parent header | 15:0 | Parent loop slot |
| Parent header | 47:16 | Parent generation |
| Parent header | 53:48 | Low six bits of the span row count for this parent |
| Parent header | 69:54 | Image-wide compiled audio writer count when authored recording is present, otherwise zero |
| Parent header | 79:70 | High ten bits of the span row count |
| Parent header | 82:80 | Previous mode at a live handoff at the end of a pass (0 use spans, 2 no active span, 3 Play, 4 Stop, 5 Record) |
| Parent header | 95:83 | Number of previous schedule spans after the current spans for a guarded restart |
| Span | 31:0 | Inclusive start frame relative to the parent's pass |
| Span | 63:32 | Exclusive end frame relative to the parent's pass |
| Span | 65:64 | Mode (1 Play, 2 Stop, 3 Record) |
| Span | 95:66 | Reserved |

Each header's block fits within the rows declared by the instruction. A
header with no spans supplies the old ending mode of a dropped child during the
handoff. All headers and spans count against the existing program region
capacity. In the first quantum after a region change, SP marks
the previous position of a composite that reached its end without a repeat
hold. For a marked parent, the header can supply the old mode for that one
comparison against the new schedule.

## MIDI instruction operands {.unnumbered}

MIDI list IDs are separate from JP audio scratch-buffer IDs.
MIDI Playback uses the entire 96-bit word. A3 packs an 8-bit sequence
ID at 63:56 and an 8-bit cursor ID at 71:64. A4 is the 16-bit channel
mask at 87:72. Bits 95:88 carry the 8-bit destination list ID. Use zero
in bits 95:88 for the other opcodes except State. State has its own complete
96-bit layout in Appendix E, with channel-mask bits extending through 95:88.
PrepareCapture and Append use a u16 sequence ID. Append also uses a u16
source list ID. The playback sequence, cursor and
destination encodings limit each ID to 0–255. Instance capacities can
be smaller.

| Instruction | Job emission |
| -------------------------------- | ------------------------------------------------------------------- |
| MIDI Input | Input for the selected port and destination list |
| MIDI Playback | Read while playing inside content length, otherwise Empty. Sets restart for a pulse, a seek/reposition flag or position zero. |
| MIDI PrepareCapture | FinishCapture for an open writer leaving recording mode or being replaced by a new take. BeginCapture for recording without an open writer. |
| MIDI Append | Append while recording, with start equal to captured frames minus Q. |
| MIDI Merge | Merge the two source lists into the destination |
| MIDI Output | Output the supplied source list on the selected port |
| MIDI MergeOutput | Merge two to eight ready lists directly into an output port |
| MIDI State | Restore when active playback starts, restarts, or seeks. Cleanup when previously active playback ends. Requires matching prepared bindings and state-capable JP data jobs. |

Playback, PrepareCapture, Append, and State require managed sync and a current
stamp on their loop record. PrepareCapture precedes operations that use its
sequence and MIDI input observation for this quantum. State and Output retain
their ordering on the shared MIDI packet stream.

MergeOutput packs eight u8 list IDs in bits 87:24, in left-to-right merge order.
Bits 95:88 give the number of list IDs used. Each source list is sorted by the
same event key as a two-input merge.

## Node-state operands {.unnumbered}

| Type | Bit layout |
| ---------------- | ------------------------------------------------------------------------------------ |
| Word | Bit 128 = 0. Bits 63:0 hold the value. Set bits 127:64 to zero. |
| Ramp | Bit 128 = 1. Bits 31:0 start Q8.24, 63:32 target Q8.24, 95:64 duration in frames, 127:96 elapsed frames. |

### Loop record and plan operands {.unnumbered}

Loop operands use a separate 16-word loop record. Its positions, plan times
and capture lengths are in frames. Mode is `0` stopped, `1` playing,
`2` recording. Mode `3` is invalid. For each plan slot the flags contain valid
at bit 0, pulse timing at bit 1, operation at bits 3:2 (0 Stop, 1 Play, 2
Record, 3 Seek) and retrigger at bit 4. The record also supplies the plan's
effective frame, remaining pulse skips and requested position. The address
in loop RAM is `16 * loop_slot + word_index`. The configuration port selects
this RAM with `cfg_loop=1` and uses the low 64 bits of the data bus.

| Word | Layout |
| --------------- | ------------------------------------------------------------------------------------- |
| 0 | Valid [0], generation [32:1], mode [34:33], repeat [35], source valid [36], source slot [52:37], pulse [53], first capture [54], processed stamp valid [55], newer plan index [56], MIDI writer-open [57], MIDI seek/reposition [58], prior MIDI playback activity [59], composite snapshot [60], composite snapshot restart [61], explicit composite [62], composite wrap [63]. |
| 1 | Source generation [31:0], plan 0 flags [36:32], plan 1 flags [41:37], composite repeat wait [42], authored capture ownership [43], composite child [44], composite timing slot [45]. |
| 2, 3, 4 | Next position, content length, captured frames, respectively. For a composite timing slot word 3 is pass length and word 4 is the previous render position, or all ones when the previous render was stopped. Value 1 in bits 33:32 of word 4 marks a previous position at a live handoff at the end of a pass. |
| 5, 6, 7 | Cycle count, content revision, revision watermark, respectively. For a composite timing slot word 5 counts completed passes and word 6 is the unit length in frames. |
| 8, 9 | Render position and processed quantum frame, respectively. |
| 10–12 | Plan 0 effective frame, remaining pulse skips and requested position. |
| 13–15 | Plan 1 effective frame, remaining pulse skips and requested position. |

Preserve the composite and MIDI fields when editing plans. SP maintains
writer-open across quanta and clears the reposition flag at the start
of the next loop visit. Clear reserved bits. When a logical instruction uses the standalone
explicit-record port, the 164-bit response packs valid [0], generation
[32:1], mode [34:33], length [98:35], position [162:99] and first capture
[163]. The response position and length are measured in frames.

Sources. [Opcode constants](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/state_processor/rtl/state_processor_defs.svh) · [SP decoder and operand packing](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/state_processor/rtl/sp_engine.sv) · [Compiler lowering](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/src/rust/rt_fpga_rtl/src/target.rs) · [Loop RAM word and flag definitions](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/state_processor/rtl/sp_loop_defs.svh)

# Appendix B. Job processor descriptors {.unnumbered}

The 256-bit FIFO descriptor is the JP instruction. Media jobs require
`ENABLE_MEDIA`, and MIDI jobs require `ENABLE_MIDI`. JP forwards the whole
MIDI descriptor to `midi_data_jobs`.

| Bits | Field |
| ---------------------- | ------------------------------------------------------------------------------ |
| 7:0 | Job opcode |
| 23:8 | ID0 |
| 39:24 | ID1 |
| 63:40 | Reserved, zero |
| 127:64 | P0 |
| 191:128 | P1 |
| 255:192 | P2 |

## Audio job opcodes {.unnumbered}

| Opcode | Job | ID0 | ID1 | P0 | P1 | P2 |
| -------- | ----------------- | -------------- | -------------- | ----------------- | --------------- | --------------- |
| `0x00` | Input | Input port | Destination | — | — | — |
| `0x01` | Silence | Destination | — | — | — | — |
| `0x02` | ReadView | Destination | — | View ID | Position in chunks | Length in chunks |
| `0x03` | Gain | Source | Destination | Start and target | Duration and elapsed | — |
| `0x04` | Mix | Left source | Right source | Destination in 15:0 | — | — |
| `0x05` | Capture | Source | — | View ID | — | — |
| `0x06` | Output | Source | Output port | — | — | — |
| `0x07` | BeginCapture | — | — | View ID | — | — |
| `0x08` | Constant | Destination | — | Q8.24 value in 31:0 | — | — |
| `0x09` | Arithmetic | Left source | Right source | Destination and mode | — | — |
| `0x0a` | Smoothing | Source | Destination | History index and duration | Playing and restart | — |

| Job | Subfield layout |
| ------------------- | --------------------------------------------------------------------------------- |
| Gain | P0[31:0] start Q8.24, P0[63:32] target Q8.24. P1[31:0] duration frames, P1[63:32] elapsed frames. |
| Arithmetic | P0[15:0] destination buffer, P0[31:16] mode, P0[63:32] zero. |
| Smoothing | P0[31:0] history index, P0[63:32] restart duration in frames. P1[0] playing, P1[1] restart. Upper bits are zero. |

ReadView uses the run quantum as the requested sample count. JP converts the
two chunk operands back to frames before sending them to the audio store.
Capture appends one quantum from the source buffer. BeginCapture prepares the
recording view and transfers no samples.

In Arithmetic, L and R are the samples read from the left and right source
buffers. The modes below describe the coefficient calculation. Both source
buffer IDs must be valid and ready in every mode.

| Mode | Operation |
| ------------: | --------------------------------------------------------------------------------------- |
| 0 | Multiply L by R |
| 1 | Left balance, multiply L by `1 − max(R, 0)` |
| 2 | Right balance, multiply L by `1 + min(R, 0)` |
| 3 | One-minus, compute `1 − L` for a control coefficient in [0, 1] |

## MIDI job opcodes {.unnumbered}

| Opcode | Job | ID0 | ID1 | P0 | P1 | P2 |
| -------- | ----------------- | -------------- | -------------- | ----------------- | --------------- | --------------- |
| `0x20` | Input | Input port | Destination list | — | — | — |
| `0x21` | Empty | Destination list | — | — | — | — |
| `0x22` | Read | Sequence | Destination list | Loop position | Read options | Reader owner |
| `0x23` | BeginCapture | Sequence | — | — | — | — |
| `0x24` | Append | Sequence | Source list | Recording position | Frame count | — |
| `0x25` | FinishCapture | Sequence | — | — | — | — |
| `0x26` | Merge | Left list | Right list | Destination list | — | — |
| `0x27` | Output | Source list | Output port | — | — | — |
| `0x28` | Restore state | Sequence | — | Target loop position | Owner | — |
| `0x29` | Cleanup state | Sequence | — | — | Owner | — |
| `0x2a` | MergeOutput | Output port | Source count | Eight packed u8 list IDs | — | — |

All MIDI timing operands are in frames. Read and Append carry Q as
a 32-bit frame count. The upper half of Append P1 is zero. Merge's
destination is zero-extended from a u16 list ID.

| Read field | Contents |
| ---------------------- | ------------------------------------------------------------------------------ |
| P1[31:0] | Frame count Q |
| P1[39:32] | Reader cursor ID |
| P1[55:40] | Channel mask, bit 0 for MIDI channel 1 |
| P1[56] | Restart or reposition the reader |
| P1[63:57] | Zero |
| P2[31:0] | Owner loop slot. SP zero-extends its 16-bit slot. |
| P2[63:32] | Owner loop generation |

The owner identifies the loop that last used the cursor. The functional
unit compares it with the saved owner alongside the view handle. Input
produces a list for the selected port. Read applies the mask to channel
messages and passes supported system messages. Append transfers all
messages in the supplied source list. Output sends that list in its
stored order. For state-enabled output, playback source IDs identify the
cursor's sequence for cleanup ownership; live-input source IDs do not acquire
loop ownership. Both ordinary and corrective accepted packets update current
state and relevant comparisons. Restore and Cleanup use matching prepared
bindings; owner P1 packs slot [31:0] and generation [63:32]. Their generated packets
use offset zero in the RTL. Appendix E defines the state control and memory ABI.

Sources. [SP descriptor packing](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/state_processor/rtl/sp_engine.sv) · [JP dispatch](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/job_processor/rtl/job_processor.sv) · [MIDI data-job dispatch](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/rtl/midi_data_jobs.sv)

# Appendix C. MIDI control commands and packet formats {.unnumbered}

The mixed MIDI wrapper exposes a six-bit `midi_command_op` and
separate 64-bit arguments. Base event commands use A0–A2, with A3/A4
encoded as zero. The optional state commands in Appendix E also use A3
for field seeds; A4 remains zero. The caller submits control between quanta and holds
the opcode and arguments until ready. Successful Replace, Duplicate and
Clear hold the reply behind the publication handshake described in the
MIDI chapter.

## Control opcodes {.unnumbered}

Opcode numbers are decimal.

| Op | Command | A0 | A1 | A2 | Result fields |
| ------: | ----------------- | ----------------- | --------------- | --------------- | ---------------------------- |
| 0 | Format | — | — | — | Clears sequence and cursor tables and prepares a fresh arena. |
| 5 | Replace | Sequence | Completed view handle | — | Published handle, duration, event count. Zero handle clears the sequence. |
| 6 | Duplicate | Source sequence | Target sequence | — | Published handle, duration, event count. Shares the source view. |
| 7 | Clear | Sequence | — | — | Zero handle, duration and event count. |
| 8 | Begin import | — | — | — | Owned writer handle. |
| 9 | Append import event | Writer handle | View-relative frame | Packed message in 31:0 | Appends one packet. A2[63:32] is zero. |
| 10 | Finish import | Writer handle | Duration | — | Seals the writer. |
| 11 | Export | Sequence | — | — | Owned completed view handle, duration, event count. |
| 12 | Read view event | View handle | Event index | — | View-relative frame, insertion order in count, packet and length. Length zero marks the end. |
| 13 | Release | Owned view handle | — | — | Drops one independent reference. |
| 15 | Create range | Source view handle | Start frame | End frame | Owned range handle. Bounds use source-relative frames, start inclusive and end exclusive. |
| 20 | Maintenance | Maximum steps | — | — | Performs the requested work, capped at 64 steps. |

Prepared state-capable images additionally accept controls 21–24 and 31
(Appendix E). Other control opcodes return Unsupported on a prepared image.
Except for Format, these controls require a prepared image. The
caller supplies a completed, quantum-aligned view when publishing
replacement content. Export and Create range confer independent
ownership. Release only handles references the caller owns.

A view handle packs generation in bits 63:32 and slot in bits 31:0.
Generation zero is invalid. A sequence index is a separate on-chip
slot that holds a view handle. The command response fields are
`error` (4 bits), `handle` and `frame` (64 bits each), `count` (32),
`packet` (24), and `length` (2). Read only the result fields relevant
to the requested command.

## Quantum interface {.unnumbered}

The isolated data-job unit has separate prepare, finish, job and control
handshakes. When idle, it considers requests in that order. Prepare
takes `quantum_frames` (u32), `event_address` (u64 byte address), and
`event_count` (u32). It returns `prepare_done/error`. Jobs take one
256-bit descriptor and return `job_done/error`. Finish has no
per-request operands. It uses the instance's MIDI maintenance budget
and returns `finish_done/error`. The mixed wrapper drives Prepare and
Finish around the shared SP/JP quantum and exposes `midi_error_code`.

| Error | Meaning |
| -----------: | ---------------------------------------------------------------------------------------- |
| 0 | Success |
| 1 | Invalid event or input window |
| 2 | Unsupported operation or packet |
| 3 | Capacity exhausted |
| 4 | Invalid handle, ID or lifecycle |
| 5 | Busy |
| 6 | Overflow |
| 7 | Memory fault |
| 8 | Incomplete content |
| 9 | Output refused |

## Packed messages {.unnumbered}

A packed u32 message has status in bits 7:0, data bytes in 15:8 and
23:16, byte length (1–3) in 25:24, and zero padding in 31:26. Use
zero for unused data bytes. The low nibble `n` of channel-voice status
specifies the zero-based channel. MIDI data bytes are seven bits.

| Status | Message | Bytes including status | Data bytes in order |
| ------------- | ------------------------------- | ------------: | --------------------------------------- |
| `0x8n` | Note Off | 3 | Note, velocity |
| `0x9n` | Note On | 3 | Note, velocity |
| `0xan` | Polyphonic key pressure | 3 | Note, pressure |
| `0xbn` | Control change | 3 | Controller, value |
| `0xcn` | Program change | 2 | Program |
| `0xdn` | Channel pressure | 2 | Pressure |
| `0xen` | Pitch bend | 3 | Low seven bits, then high seven bits |
| `0xf1` | MIDI Time Code Quarter Frame | 2 | Message type and values |
| `0xf2` | Song Position Pointer | 3 | Low seven bits, then high seven bits |
| `0xf3` | Song Select | 2 | Song number |
| `0xf6` | Tune Request | 1 | — |
| `0xf8` | Timing Clock | 1 | — |
| `0xfa` | Start | 1 | — |
| `0xfb` | Continue | 1 | — |
| `0xfc` | Stop | 1 | — |
| `0xfe` | Active Sensing | 1 | — |
| `0xff` | System Reset | 1 | — |

Other status bytes, SysEx, and byte-stream running status are outside
the packet interface. System messages are event data. For example, the
MIDI Stop byte `0xfc` is distinct from an SP Stop plan. With expression state
enabled, observed System Reset `0xff` also updates the musical-state model
as specified in the state chapter.

## Event record layouts {.unnumbered}

All three records are 16 bytes. Word numbers index the four u32 fields
from the start of each record. Words marked unused should be encoded as zero.

| Word | Staged input | Scratch list | Stored view record |
| ------------: | -------------------------- | -------------------------- | ---------------------------------------- |
| 0 | Input port | Quantum offset | Timestamp low 32 bits |
| 1 | Quantum offset | Source ID | Timestamp high 32 bits |
| 2 | Packed message | Insertion order | Insertion order |
| 3 | Ignored padding | Packed message | Packed message |

Scratch source ID equals the cursor index for playback or
`0x10000 | input_port` for live input. Stored-view timestamps use the
view's time origin. The store subtracts that origin on reads, so a range
can share the original records while returning rebased time.

Sources. [Current protocol](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/PROTOCOL.md) · [Command and record dispatch](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/rtl/midi_data_jobs.sv) · [Packet validator](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/midi/rtl/midi_functions.svh)

# Appendix D. Internal MIDI content-store commands {.unnumbered}

The MIDI data-job unit issues these four-bit commands to `midi_store`.
They share the DDR word interface with the data-job unit. The content
controller executes them directly.

`view`, A0, A1 and `source` are u64, `count` is u32, `packet` is 24 bits,
and `length` is two bits. A time bound is in frames relative to the
supplied view. Byte addresses are in the logical memory mapping.

| Op | Operation | View | A0 | A1 | Other fields |
| ------: | ---------------------- | ----------------- | ------------------ | -------------- | ------------------------- |
| 0 | Format arena | — | — | — | — |
| 4 | Create writer | — | Event limit, zero for default | — | — |
| 5 | Append one event | Writer | Event frame | — | Packet and length |
| 6 | Finalize writer | Writer | Final duration | — | — |
| 7 | Create range view | Source view | Start frame | Exclusive end frame | — |
| 8 | Retain view | Completed view | — | — | — |
| 9 | Release view | View handle | — | — | — |
| 10 | Read event | View handle | Event index | — | — |
| 11 | Retire content | — | Work budget in 31:0 | — | — |
| 12 | Describe view | View handle | — | — | — |
| 13 | Update writer duration | Writer | New duration | — | — |
| 14 | Discard writer prefix | Writer | Number of leading events | — | — |
| 15 | Append list | Writer | Recording start | Exclusive recording end | Source byte address and event count |

Opcodes 1–3 are unsupported. Read event returns the frame, order, packet
and length. Length zero indicates an out-of-range index. Describe
view exposes duration, event and block counts, status and references.
Append list reads records in the scratch format in Appendix C, using
their quantum offsets and packed messages. It adds each offset to the
recording start and advances duration to the exclusive recording end.
The caller provides a 16-byte-aligned source after the store's content
region.

## View descriptor in DDR {.unnumbered}

The MIDI arena contains these regions in address order. V is the view
capacity, T is the block-table capacity per view, B is the number of
blocks, E is the event records per block, R is the retirement capacity,
and C is the scratch-event capacity. The input array is staged
separately after the arena and must fit the enabled memory window.

| Region | Bytes |
| ---------------------------- | ----------------------------------------------------------------- |
| View descriptors | V × 64 |
| Per-view block tables | V × T × 4 |
| Block references | B × 4 |
| Free block stack | B × 4 |
| Stored event records | B × E × 16 |
| Retirement entries | R × 8 |
| Scratch alignment padding | 0–15 |
| Shared scratch records | C × 16 |
| Optional expression state | Appendix E size; absent from raw-only builds |

Each view descriptor is 16 u32 words. The per-view block table supplies
the physical block IDs in view order. Append and read locate a record
using the view's first-event offset plus its event index.

| Words | Fields |
| --------------- | ------------------------------------------------------------------------------------------ |
| 0–2 | Generation, reference count, status |
| 3–5 | Block count, first-event offset, event count |
| 6–7 | Time origin, u64, low word first |
| 8–9 | Duration, u64, low word first |
| 10–11 | Reserved, zero |
| 12–13 | Next insertion order, event capacity |
| 14–15 | Reserved, zero |

Status values are 0 empty, 1 recording, 2 complete, 3 incomplete,
and 4 retired. Releasing the final view reference queues deferred
retirement. Ranges hold references to shared blocks until their own
retirement.

Sources. [Content controller and its ports](https://github.com/SanderVocke/shoopdaloop/blob/cc410f27/fpga/midi/rtl/midi_store.sv) · [Data-job caller](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/rtl/midi_data_jobs.sv) · [DDR layout and view protocol](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/PROTOCOL.md)

<a id="midi-expression-state-abi"></a>

# Appendix E. MIDI expression-state ABI {.unnumbered}

This extension is described at RTL revision `462ce74e`. It requires
`MIDI_STATE_SUPPORTED=1` on the mixed top, or `STATE_SUPPORTED=1` on
`midi_data_jobs`. It adds state data after the scratch arena; the event-record
formats in Appendix C do not change. After formatting the selected image,
command 31 reports ABI `0x45580100` in `response_count`. On a prepared raw-only
image the state controls/jobs return Unsupported. Before preparation, non-Format
controls return Invalid lifecycle as usual.

## MIDI State instruction, opcode `0x13` {.unnumbered}

This instruction uses all 96 bits; it is not the ordinary five-u16 encoding.

| Bits | Field |
| --- | --- |
| 7:0 | Opcode `0x13` |
| 23:8 | Owner loop slot, u16 |
| 55:24 | Owner loop generation, u32 |
| 63:56 | MIDI sequence, u8 |
| 71:64 | Direct source input, u8 |
| 79:72 | Output-port mask, u8; bit 0 selects output 0 |
| 95:80 | MIDI-channel mask, u16; bit 0 selects MIDI channel 1 |

Place all PrepareCapture instructions before all State instructions. The loader
installs the same binding through command 21. SP validates the owner against
loop RAM and uses the sequence and loop's render state for job emission; it does
not install the input/output/channel binding from this instruction each run.
A State instruction can emit at most one Restore or Cleanup job per quantum.
Loop word 0 bit 59 remembers prior MIDI playback activity. Preserve
it alongside bits 57 and 58 when updating loop plans.

## State control commands {.unnumbered}

Opcode numbers are decimal. Arguments are separate u64 values. Set unused
arguments, including A4, to zero. An owner packs slot in 31:0 and generation in
63:32. Sequence and port IDs remain separate namespaces. Prepared output masks
must be nonzero and fit the configured outputs; channel masks must be nonzero.
The state implementation supports up to 128 sequences and eight outputs.

| Op | Command | A0 | A1 | A2 | A3 | Result |
| ---: | --- | --- | --- | --- | --- | --- |
| 21 | Bind state | Sequence | Owner | Packed source/scope | — | Error |
| 22 | Seed state field | Space | Port ID or view handle | Field index | u32 field word | Error |
| 23 | Set controller policy | Output port | CC mask 63:0 | CC mask 127:64 | — | Error |
| 24 | Read state field | Space | Port ID or view handle | Field index | — | Field word in `response_count` |
| 31 | Query ABI | — | — | — | — | `0x45580100` in `response_count` |

For Bind A2, bits 7:0 select the input, 15:8 are the output-port mask, and
31:16 are the channel mask; upper bits are zero. Space 0 means input, 1 output,
and 2 view snapshot. Snapshot access uses the full validated view handle. Seeding
writes the model; it does not transmit MIDI packets to establish physical
instrument state. The caller must make those assumptions truthful.

Seed an output before installing bindings; output seeds return Busy once any
binding is installed. Input seeds can establish held-source state before
recording. Snapshot seeds require an empty recording writer whose snapshot has
not been sealed by capture. The first seed in a new channel clears that
channel's other fields to unknown. Completed snapshots cannot be edited.
Controller policy rejects bits outside the ordinary CC ranges in the musical
behaviour chapter and always includes CC64/66. Changing it refreshes comparisons
between quanta. These controls are serialized with quantum execution.

## State DDR layout {.unnumbered}

Let I = configured inputs, O = outputs, V = views and S = sequences. Starting
immediately after the shared scratch records, allocate these state regions in
order. Offsets here are relative to that state-region base, not the start of the
whole MIDI arena.

| Region | Slots | Bytes per slot |
| --- | ---: | ---: |
| Observed inputs | I | 32,768 |
| Current outputs | O | 32,768 |
| View starting snapshots | V | 32,768 |
| Private seek target | 1 | 32,768 |
| Sequence/output comparisons plus private seek comparison | S × O + 1 | 2,048 |

The extension's size is `32768 × (I + O + V + 1) + 2048 × (S × O + 1)`
bytes. Comparison slot `sequence × O + output` holds that pair's bitmap and
cleanup activity; the final slot is private seek workspace. On-chip metadata
protects view channels/lifetimes, bindings, directories and counts. It is not
part of these DDR records. Reformatting invalidates the existing contents.

Each state has 16 channels with 512 u32 words per channel. Its field index is
`channel × 512 + local_field`. The byte address within a state is `field × 4`.
Channels are zero-based in this calculation.

| Local field | Meaning | Value bits, apart from known bit 31 |
| ---: | --- | --- |
| 0–127 | Key's logical note state | Velocity [6:0], key down [7], sostenuto capture [8], sustain hold [9], per-note pressure [22:16], pressure known [23] |
| 128–255 | CC0–127 | 7-bit controller value [6:0]; only ordinary CCs have modeled state |
| 256 | Pitch bend | 14-bit value [13:0] |
| 257 | Channel pressure | 7-bit value [6:0] |
| 258–511 | Padding | Reserved |

Bit 31 means the field is known. A known note with velocity zero represents
silence; an unknown word is not a known zero. Per-note pressure is meaningful
for an active voice with bit 23 set. Use zero for unused/reserved bits. Model
accesses treat fields on a channel without valid snapshot context as unknown.

| Offset within 2 KiB comparison slot | Meaning |
| --- | --- |
| 0–1023 | 256 u32 difference words. Field f is bit `f mod 32` in word `f / 32`. |
| 1024–1055 | Eight u32 summaries. Summary bit w says difference word w is nonempty. A clear bit makes that word logically empty regardless of stale DDR payload. |
| 1056–1375 | Sixteen channel ownership records, five u32 words each: four note bitmaps, then pedal flags (bit 0 sustain, bit 1 sostenuto). |
| 1376–2047 | Reserved |

A difference describes a target field that is known and disagrees with the
modeled current output. Note comparisons include key/hold relationships and
known active-voice pressure. Difference counts and owner-activity counts/channel
hints live on chip. They allow clean restore and empty-owner cleanup to avoid
DDR traversal. The Rust reference uses equivalent logical maps with u64 words;
its in-memory packing is not this RTL u32 DDR ABI.

Sources. [SP packer](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/src/rust/rt_fpga_rtl/src/component.rs) · [SP state-job emission](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/state_processor/rtl/sp_engine.sv) · [Control and output dispatch](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/rtl/midi_data_jobs.sv) · [State word accesses](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/rtl/midi_expression_state.sv) · [State protocol](https://github.com/SanderVocke/shoopdaloop/blob/462ce74e/fpga/midi/PROTOCOL.md)
