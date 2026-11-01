# Core persistence contract

Implementation contract for [the persistence plan](../../PLAN_SESSION_PERSISTENCE_MEDIA_IO.md).
This records decisions, not completed feature support. P01/P02/P11 remain open.

## Field and capability matrix

| Document/live data | Core behavior | Ownership and refusal |
| --- | --- | --- |
| Sample rate | Save host rate; confirm conversion and resample audio, MIDI and timing before loading at another rate | Client codecs; recheck host rate before commit; never switch device implicitly |
| Direct audio/MIDI/trigger tracks and primitive loops | Preserve names, order, topology, media, supported controls and sync relationships | Service maps document IDs to fresh live IDs; RT owns playback/resources |
| Loop/channel length, gain, offsets, capture alignment and preplay | Preserve supported stored values and exact media timing | Validate every value; reject an unsupported non-default value before mutation; this does not enable P09/P10/P12 editing |
| Selection, target and global policies | Preserve document values supported by the core | Service-owned; reject unsupported recording/default-playback policies |
| Master, ports and mixer routes | Preserve supported routing/control values and connection intent | Resolve external ports by exact identity; report missing destinations and retain intent |
| Playback/recording position and pending musical commands | Do not persist transient execution | Load loops stopped; cancel/fence prior-session work at commit; never replay a saved operation |
| FX/synth/dry-wet, composites, non-Master buses | Refuse complete documents requiring unavailable capabilities | P03/P06/P07/P08/P13 remain open; no flattening or dropped state |
| Session-owned scripts/resources | Refuse documents requiring unavailable ownership/resource handling | P16 remains open; VM stays client-side and is never serialized/restarted |
| MIDI control/settings fields | Classify every field against current public state before conversion | Preserve implemented session fields; reject unsupported non-default session fields; machine preferences stay local |
| File names, URLs, picker state, output bytes and task progress | Existing platform workflows and truthful completion/cancel/error states | Client-owned; never place filesystem paths or codecs in engine commands |

## Finite budgets

| Resource | Per-job limit / admission rule |
| --- | --- |
| Metadata | 8 MiB, including the persistence manifest |
| Decoded media | 256 MiB aggregate audio/MIDI, independent of the old 32 MiB media-view message |
| Input/output file | 512 MiB compressed/encoded bytes; reject before decoding/output allocation beyond this budget |
| Transfer objects | 4096; object zero is the sole metadata object, followed by ordered audio/MIDI objects |
| Wire chunk | 1024 bytes; fits the existing 1280-byte carriers with identities/envelope |
| MIDI | At most 1,000,000 events per job, additionally bounded by decoded bytes and runtime packet/index capacities |
| Concurrent host persistence work | One staging or capture job; normal controls/snapshots continue with bounded pump work |
| Archive expansion | Declared entry counts and cumulative expanded bytes checked before payload extraction; actual decoded shapes/counts checked again |
| RT content and activation | Preflight currently free pages/leases/indexes, logical/physical slots, graph/topology candidates and reliable receipts; count retained old content and capture pins |

These are ceilings, not guaranteed engine capacity. A candidate that cannot coexist
with current content is refused before changing current state. Existing prepared
recording-pool capacity is unchanged. Account separately for client encoded bytes,
decoded staging, codec/resampling scratch and host ingress copies; no stage may
infer an unbounded budget from the archive codec's default 16 GiB limit. Admission
and cleanup must account for this peak, not just final PCM size.

## Transfer and mutation lifecycle

Every persistence record has the active connection authority and a key containing
a nonzero transfer ID and session epoch. A job's target is a session revision or
exact loop ID/content revision. Every upload record is checked against its owner;
the service also checks the current epoch and target at begin and commit.

1. `Begin` declares all metadata/media bytes and object counts. Reserve one job and
   its aggregate budget before acknowledging. Keep this separate from mutation
   admission. Reused transfer IDs cannot address another attempt in the same scope.
2. `Object`, ordered `Chunk`s and `FinishObject` fill each declared object. Validate
   whole records before consuming anything. Exact retransmission of the last chunk
   acknowledges the existing offset; altered, older, out-of-order and oversized
   chunks are refused. `Finish` requires exact per-object and aggregate totals.
3. Validate decoded metadata, references, media, capabilities and final engine
   headroom while the old session remains active. Ready ingress is not a promise
   that engine preparation has finished. Keep the candidate private until ready.
4. `CommitPersistence` is an ordinary fenced command with the existing operation
   ID/admission/result machinery. Reserve result and RT retirement ownership before
   acceptance. Its target/revision must still match; accepted mutation becomes
   host-owned and completes exactly once across UI loss.
5. Before commit admission, cancellation/loss releases staged ownership. After
   admission, cancel cannot claim rollback: reconcile the durable operation result.
   Publication changes the session epoch so old-loop commands, uploads and late
   task callbacks cannot address the new session accidentally.

Capture uses the same bounded object descriptions and read offsets in the reverse
direction. Settle the accepted-command watermark, reject active recording/replacing
again at the cut, and pin matching application metadata and engine content versions.
Subsequent playback, selection changes or content edits do not change the capture.
Release/cancel can reclaim read-only captures on client loss. Reads must not starve
normal service work or silently restart against newer content.

## User-visible workflow semantics

- Save/export: request capture, read immutable objects, encode on the client,
  request platform output, then report the platform's actual completion. A browser
  download handoff is not proof of a durable filesystem write. Failed/cancelled
  native output preserves any previously saved file and allows retry.
- Load/import: read/decode, validate, ask any sample-rate/channel questions, recheck
  the target, stage, commit and await the confirmed result. Decline/cancel before
  commit leaves playback/content intact. New Session uses safe replacement too.
- Audio import/export keeps reference channel selection/role order and mapping;
  importing one medium preserves the other. Only reference-prescribed offset resets
  and explicitly requested length updates apply. Exact MIDI preserves event bytes,
  frame positions and ordering; SMF export reports quantization/format loss.
- Unsupported session fields produce complete useful capability diagnostics before
  mutation. They are not omitted from save or stripped during load. Session script
  persistence, duplication and generated click insertion remain named follow-ups.

## Engine integration boundary

The current adapter's remove-then-create helper cannot satisfy atomic load. Reuse
prepared loop node-pair sources, immutable exports and configuration transactions,
but first establish an engine transaction that preflights complete content, graph,
timing and retirement ownership. Current configuration activation requires already
stopped edited owners; a scheduled stop alone is insufficient. Integrating a safe
cut for live-session replacement is Stage 2 work, not supplied by the transfer
receiver. No rejected preparation may leave old playback faded/stopped or mappings
partially replaced.

Validation evidence is recorded in the plan as milestones complete. Protocol tests
alone do not establish engine atomicity, capture coherence or restored UI support.
