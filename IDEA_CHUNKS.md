# Audio chunks and reference-based rendering

The native prototype implementing the three selectable render paths is documented
in [the prototype guide](docs/design/chunks/PROTOTYPE.md). Its benchmark measurements and implementation
limitations take precedence over speculative cost estimates below. The prototype
reuses the binary persistent index and interleaved channel groups; four-way index
tuning and separate history arenas remain future options.


## Goal

Reduce audio copying and intermediate buffer traffic while keeping render work
bounded in the worst case. Additional sharing of recorded audio must not introduce
payload-copy work into ongoing capture.

This is a design proposal, not an implementation specification. It establishes
terminology, storage lifetimes, and correctness constraints. A second design pass
is recorded in [the index and borrowing design note](docs/design/chunks/DESIGN.md),
with an analytic model and a compiled safe Rust experiment. Open questions and
remaining validation requirements are explicit there.

## Motivation

The current shared audio history path stages whole pages for each append. With a
32-frame quantum, stereo float32 audio, and 4096-byte pages, an append supplies
256 bytes of new samples but performs approximately 8192 bytes of page copying,
plus sample patching and persistent-index maintenance.

An exclusive-ownership fast path could reduce typical costs, but would retain a
more expensive path when pages are shared with grabs or snapshots. The proposed
design instead produces fresh immutable chunks and shares completed chunks by
reference. Sharing changes storage occupancy rather than the amount of sample
data written by capture.

## Terminology

| Name | Meaning |
| --- | --- |
| `AudioChunk` | Sample storage for one quantum in the initial implementation. |
| `ChunkRef` | Identifies a persistent chunk, transient chunk, or implicit silence. |
| `ChunkSpan` | A chunk reference plus the sample range being used. |
| `AudioSequence` | An ordered sequence of chunk spans, carried between processors. |
| `PersistentAudioPool` | Indexed storage for immutable chunks retained across render iterations. |
| `RenderArena` | Indexed transient storage, reset after each render quantum. |

A quantum is the engine's fixed graph execution interval. A host callback can
contain multiple quanta. Sample ranges and channel layout must be unambiguous;
the exact descriptor representation remains to be designed.

## Storage model

### PersistentAudioPool

The persistent pool holds grab history, recorded audio, and audio retained by
loops, snapshots, and exports.

For each captured quantum, acquire fresh storage, write the samples, and seal the
chunk before publishing references to it. Published persistent chunks are
immutable. Recording and editing produce new chunks rather than modifying
existing shared chunks.

History maintains references to its recent chunks. Expiring history releases its
ownership; a grabbed loop or snapshot can keep the same samples alive. Continued
capture never copies an old payload merely because another owner retains it.

The pool uses prepared contiguous storage and custom allocation with blocks
identified by index. Chunks are not individual heap objects. Larger physical
slabs can contain independently managed quantum-sized slots. Start with exactly
one quantum per chunk and a prepared channel-group width. Variable-duration
chunks and general variable-size allocation are deferred.

Handles must distinguish reused slots, for example through generations. Pool
allocation and ownership metadata are also prepared in advance.

### RenderArena

The arena holds temporary results such as effect outputs, gain processing,
mixes, and other materialized signals needed only during the current quantum.

Allocation advances a cursor through prepared contiguous storage. Initially,
allocations remain valid until quantum completion; reclaiming individual outputs
after their final consumer is an optional later optimization.

Resetting the arena resets allocation state. It does not clear the entire sample
storage. Each producer initializes the samples it publishes. A mixer initializes
its destination before accumulating into it.

Transient references cannot escape the quantum. Epochs or equivalent validation
should detect accidental reuse after an arena reset.

Persistent processor state, including delay lines and reverb state, has its own
appropriate lifetime. A processor's current output can be transient even when
the processor itself is stateful.

## Connections and processing

Connections carry `AudioSequence` descriptors rather than owning or copying
sample buffers. Descriptor storage is preallocated and bounded as well.

Routing, fan-out, and playback selection can forward or select references.
Forwarding does not need to create persistent ownership for every graph edge:
existing owners can keep chunks alive for the duration of the quantum.

Processors materialize audio when computation requires it. Mixing and effects
still read samples and produce samples; references eliminate avoidable copies,
not the underlying DSP. Known silence can use an implicit reference without
allocating a sample payload.

`AudioSequence` has cheap silence and single-span cases, plus a flat, bounded
sequence of spans when required. It must not become an
unbounded graph of deferred mixes or nested reference sequences. Every processor
has a known bound on the number of spans it may consume and the amount of work
needed to gather contiguous input when required.

Simple operations may eventually be fused into a consumer, such as applying a
loop gain during mixing. Baseline fusion preserves signal semantics, including
ramps, smoothing, capture points, and relevant floating-point ordering. The
optional arithmetic-descriptor experiment below explicitly permits a different
interior ramp curve and must be evaluated separately.

Stateful effects execute at their scheduled time. Retained processed recordings
contain the samples actually produced, rather than instructions to recompute
those samples through an effect later.

## Crossing the lifetime boundary

A recording or history entry must never retain a `RenderArena` reference.

At a capture point, either:

1. The producer writes directly into a fresh persistent chunk and seals it.
2. A transient result is copied into a fresh persistent chunk once.

Multiple recordings of the same signal at the same capture point can retain the
same persistent chunk. They should not independently copy its payload.

Capturing at different points in a signal chain can require different chunks.
The design must preserve what each recording captures; a later gain or effect
change must not retroactively alter previously captured samples.

Host output must be consumed or transferred into storage with a sufficient
lifetime before the arena is reset. Buffering adapters that retain samples across
quanta cannot retain arena references.

## Worst-case guarantees

The central capture guarantee is:

> For an admitted configuration, each distinct captured signal requires at most
> one capture copy per quantum, independent of the number of retained owners.
> Direct production into persistent storage avoids that capture copy entirely.

This counts capture copies separately from the producer's required DSP writes.
Shared recordings of one capture point do not duplicate its payload. Separate
edited or processed signals can require their own materialized chunks.

This does not imply constant total callback time. Playback, processing, routing,
metadata maintenance, and host I/O each require their own explicit bounds.

- Graph admission reserves enough arena sample storage, sequence descriptors,
  and processor scratch for the maximum work of one quantum. Exhaustion must
  not trigger a heap allocation or an unexpected fallback during rendering.
- Persistent retention admission reserves sufficient headroom for continued
  history, admitted recordings, metadata updates, and outstanding reclamation.
  Additional grabs or snapshots are refused before they threaten those
  guarantees. Finite storage cannot admit unlimited retained audio.
- Grabbing a long interval must not synchronously increment a reference count
  for every chunk. A bounded range-ownership or persistent-sequence operation
  must retain the exact capture cut.
- Deleting a long loop or releasing a snapshot must not synchronously free its
  entire sequence. Reclamation has a fixed work budget and prepared queue
  capacity, with admission accounting for any backlog.
- Index updates, reference operations, and allocation costs must be bounded.
  Avoid individual heap objects and unnecessary atomic reference-count traffic
  on graph connections.
- Operations that can fail must validate and reserve their requirements before
  publication. Multi-channel and multi-target changes preserve their existing
  atomicity; refusal cannot expose a partially applied recording or edit.

Dense metadata and predictable access patterns matter alongside payload volume.
Small chunks must not exchange page-copy amplification for excessive pointer
chasing, metadata copying, or reclamation bursts.

## Behavior to preserve

- Grabs retain the exact requested audio interval without waiting for a larger
  page to fill.
- Continued history capture and ring wrap cannot change retained audio.
- Loop edits, forks, snapshots, and exports retain their isolation guarantees.
- Playback positioning, wrapping, channel routing, gain, smoothing, and effect
  timing retain their established semantics.
- Render execution remains allocation-free with respect to the general heap.
- Resource accounting covers both sample storage and descriptor/index storage.

## Suggested implementation stages

1. Introduce immutable chunk storage for history and retained audio, with bounded
   allocation, range ownership, and reclamation. Preserve existing processor
   buffer interfaces initially if needed.
2. Carry `AudioSequence` references through playback and simple graph routing,
   using the `RenderArena` for materialized results.
3. Measure remaining traffic before adding operation fusion or intermediate
   storage reuse.

History-ring specialization and arithmetic descriptors are optional, deferred
experiments. Neither is part of these initial implementation stages. History
initially uses the common persistent pool and index machinery with circular
logical addressing.

Each stage must be assessed under maximum admitted sharing and retention, not
only an idle or unshared session. Validation should cover capture during grabs,
history wrap, retained exports, edits, bulk deletion, memory pressure, and
callbacks containing multiple quanta.

## Optional experiment: per-source history-ring specialization

Defer this specialization until after the core chunk design is implemented and
measured. Both custom source-local indexing and dedicated physical history
arenas are optional; the initial design requires neither. Existing models and
checks do not evaluate this specialization.

Each source's grab history has a predictable logical access pattern: append the
newest quantum and expire the oldest. A specialized source-local circular index
could exploit this pattern and improve locality while obtaining immutable
payload slots from the shared persistent pool. Its snapshot/range-ownership
mechanism must still support bounded grabs; a circular array alone does not make
a long grab cheap.

A further possibility is source-local circular allocation of physical segments.
Expiring a history entry releases only history's ownership. A retained loop or
snapshot can pin the payload, so the underlying storage does not necessarily
become free in FIFO order. Pinned segments cannot be reset or overwritten, and
one retained chunk should not unnecessarily immobilize a large segment.

Any source-local allocator must preserve stable chunk references, reserve space
for continued capture, and recycle segments only after their final owners leave.
Moving pinned samples elsewhere to make room would reintroduce the
sharing-dependent copies this design is intended to eliminate. Any future
adoption requires a separate decision based on worst-case pinning, locality, and
bookkeeping costs.

## Optional experiment: arithmetic descriptors

This feature is proposed for separate experimentation. It is not a decision for
the initial implementation, and the shared-chunk storage and reference-routing
design must work without it. Existing model results and borrowing checks do not
include or validate this feature. No new model or implementation is introduced
for it here.

The idea is to carry a bounded description of gain arithmetic alongside chunk
references, allowing consecutive gain stages to compose their descriptors before
touching samples. Copy is an identity operation. A constant gain is an envelope
with equal endpoints. Each quantum-local linear envelope is represented by its
actual first and last sample gains, `(start, end)`.

### Endpoint-preserving approximation

Ramps span whole sets of quanta. For this experiment, divide a longer ramp into
quantum-local envelopes and use the same sample domain for every descriptor
composed within a quantum.

Combining two gain envelopes requires only:

```text
combined.start = first.start * second.start
combined.end   = first.end   * second.end
```

Repeated composition keeps the descriptor the same size and requires two
multiplications per combination. It does not read or write audio samples.

The composed envelope is defined to be linear between the resulting endpoints.
This deliberately approximates the ideal product of overlapping linear ramps,
which can have a higher-order curve. The required invariant is that the first
and last sample gains match the ideal stack under the chosen floating-point
evaluation rules; intermediate gains are permitted to differ. Exact equivalence
of the interior samples to the existing stacked processing is not a requirement
of this optional experiment.

Use the graph quantum as the fixed approximation segment. Splitting an
`AudioSequence` into chunk spans, or changing host callback partitioning, must
not choose new approximation boundaries. A partial span evaluates the already
composed quantum envelope at its corresponding sample offsets.

### Materializing an envelope

Compose endpoint pairs throughout the eligible region. Calculate the sample
step only when samples are needed:

```text
gain = start
step = (end - start) / (sample_count - 1)
```

The sample kernel multiplies by `gain` and increments it by `step`. There is no
need to recompute interpolation for every sample. Honor the final endpoint
despite accumulated rounding. A one-sample quantum uses its intended single
gain directly, with no division or distinct first/last gains.

Composing endpoints avoids repeatedly deriving constants and slopes at each
node. Gain application can share a final sample pass with mixing, reducing
intermediate reads, writes, and clears as well as redundant gain arithmetic.

### Scope and bounds

Initially, limit the experiment to identity, constant gain, and the approximated
linear gain envelope. Defer general polynomial envelopes and arbitrary lazy
expression trees.

A mix still represents a sum of source contributions. It cannot generally be
collapsed into one chunk reference and one gain envelope. Keep contributor
counts bounded by the prepared graph, and materialize through an admitted mixer.
Fusion through mixes or reconverging paths is a separate extension, not necessary
to test consecutive gain stages.

Effects, capture points, and meter taps must observe the signal at their defined
position in the graph. Evaluate the relevant descriptor there; a consumer may
evaluate without allocating an intermediate buffer when its interface permits.
Stateful effects still execute at their scheduled time. Retained processed audio
contains the evaluated result, so subsequent changes to a live descriptor cannot
alter previously captured samples.

Choose eligible regions and their resource bounds during graph preparation.
Avoid a runtime expression stack whose growth triggers an unbudgeted fallback.
The baseline processor path remains available independently of this experiment.

### Separate evaluation before adoption

Future evaluation should cover endpoint accuracy, interior-curve and audible
differences, ramps continuing across quanta, one-sample quanta, partial spans,
host callback partitioning, capture and metering semantics, and floating-point
composition order. Performance measurements must include the maximum admitted
number of simultaneously active ramps. Any decision to adopt the approximation
follows that separate evaluation.

## Open questions

- Validate the proposed fixed-fanout index and its numeric work budget on target
  hardware; choose four versus eight entries using cache-sensitive measurements.
- Finalize channel-group widths and pool layout; use planar samples at processor
  boundaries and account for format conversion at persistence boundaries.
- Prove descriptor bounds for partial spans, imported audio, and gathers.
- Extend the borrowing prototype to the actual multi-output processor interface
  and atomic audio/MIDI capture transactions.
- Complete admission formulas for command staging and reclamation backlog, in
  addition to the capacity illustrations provided by the model.
- Identify which processors forward references and which materialize outputs.
- If the deferred history-ring experiment is pursued, evaluate custom indexing
  and physical segmentation against the common persistent pool and index,
  including worst-case pinning. This is not a gate for the initial design.
- Evaluate the optional arithmetic-descriptor experiment independently before
  deciding whether to adopt its endpoint-preserving ramp approximation.

MIDI storage and processing are outside the scope of this audio design.
