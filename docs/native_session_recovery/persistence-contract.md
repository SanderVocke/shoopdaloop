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
| MIDI control bindings (`id`, `message`, `action`, `target_id`) and session settings (`key`, typed `value`) | Save empty collections; reject nonempty collections, matching reference `session_bundle_to_backend` | These reserved document workflows have no current runtime implementation; machine preferences stay local |
| File names, URLs, picker state, output bytes and task progress | Existing platform workflows and truthful completion/cancel/error states | Client-owned; never place filesystem paths or codecs in engine commands |

## Finite budgets

| Resource | Per-job limit / admission rule |
| --- | --- |
| Metadata | 8 MiB, including the persistence manifest |
| Decoded media | 256 MiB aggregate audio/MIDI, independent of the old 32 MiB media-view message |
| Input/output file | 512 MiB compressed/encoded bytes; reject before decoding/output allocation beyond this budget |
| Core document shape | At most 64 tracks and 256 primitive loops, with ≤2 direct audio channels and ≤1 MIDI channel per loop; fixed stereo Master and supported routes |
| Transfer objects | 4096; object zero is the sole metadata object, followed by ordered audio/MIDI objects |
| Native wire preparation | Validate/index ≤256 KiB per client connection poll before streaming; cancellation and detach discard pending validation and upload |
| Native wire turns | At most 64 persistence records per connection turn (≤64 KiB of media), with at most 64 upload records awaiting replies; ordinary commands retain their 16-record limit. Outbound accounting counts emitted records and bounds encoder cleanup separately. This batches file work across platform timer wakeups without increasing RT buffer ownership |
| Wire chunk | 1024 bytes; fits the existing 1280-byte carriers with identities/envelope |
| MIDI | At most 1,000,000 events per job, additionally bounded by decoded bytes and runtime packet/index capacities |
| MIDI initial/preroll state | At most 16,384 short channel or System Reset packets per loop; parameter history that remains untracked after the complete prefix and vendor/system initial state refuse before activation (P11) |
| Wire MIDI read index | At most 8 MiB plus one offset per object; built only for byte delivery, alongside retained immutable media |
| Adapter read assembly | ≤16 outstanding bulk records per read. Audio fills final channels on arrival; MIDI has one ≤256 MiB byte builder and decodes ≤256 KiB per pump, with one complete ≤1 MiB packet atomic. Event/packet allocation remains bounded by one million events and the media budget |
| Engine upload window | At most 64 queued writes when WIDE_CONTENT_UPLOAD is advertised, otherwise eight; cap the window by reliable credits. Both use eight prepared runtime buffers. Chunks divide a resource page and are at most 1024 bytes. The prepared callback validation budget reserves at least one such MIDI chunk; queue depth does not increase callback work budgets |
| Adapter preparation | Validate ≤256 KiB per pump before issuing engine preparation; one ≤1 MiB MIDI packet is atomic. Cache byte counts only with a proof retaining the immutable content. Capture checks remaining aggregate capacity before allocating each medium |
| Browser progression | Main-thread service/client pump on engine replies, every 4 ms and on UI turns, sharing one elapsed-time clock. Each turn retains the existing record/media work bounds; dropping the runtime clears the interval |
| Concurrent host persistence work | One staging or capture job; normal controls/snapshots continue with bounded pump work |
| Client file input | One platform reader plus one queued raw input, each ≤512 MiB; one codec task. Cancellation fences callbacks and checks file chunks. Browser URL cancellation wakes the pending future; a native reader keeps its slot until it exits or the HTTP timeout fires. A new input refuses while that slot is closing |
| Archive expansion | At most 4097 entries and `5 × 256 MiB + 8 MiB` expanded bytes; metadata manifests remain 8 MiB. The factor allows decimal MIDI JSON overhead. Check declarations before extraction and actual shapes/counts while decoding |
| Native codec lifetime | One active task per client; process-wide pool of two workers with eight queued jobs. Dropped clients cancel queued work and discard draining results. Overload refuses explicitly |
| Platform output | One outstanding writer/download per UI root, retained across runtime changes; task IDs remain unique across client runtimes and completion is client-local even after backend loss |
| Codec scratch | Per codec job: Retain at most one 512 MiB input/output, 256 MiB decoded image, 256 MiB conversion image and the expanded-byte ceiling of encoded payload scratch. Exact MIDI JSON streams from ZIP into charged packet/event builders; JSON strings are capped at 8 MiB. PCM extraction uses at most one additional 256 MiB entry; WAV deinterleaves directly into its final channels |
| Source audio shape | At most 256 channels before mapping to the current core; empty and wider sources refuse explicitly. Source labels/roles share the metadata budget |
| MIDI file decoding | Packet ≤1 MiB, event ≤1 million, initial packets ≤16,384 per channel, all charged before vector growth. Lazy SMF parsing also bounds source tracks and meta events; tempo lookups use a sorted cumulative map |
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
  download handoff is not proof of a durable filesystem write. The browser adapter
  distinguishes a download-link click from dialog cancellation/close and releases
  the temporary Blob URL after the handoff event turn. Failed/cancelled
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

The adapter stages a private mirror instead of removing live tracks. It uses
prepared loop node-pair sources, immutable exports and configuration transactions,
but first establish an engine transaction that preflights complete content, graph,
timing and retirement ownership. The existing `ConfigurationActivate` requires
already-stopped edited owners; a scheduled stop alone is insufficient. Stage 2
adds `ConfigurationReplace` for a complete fresh-owner cut at callback start:
media/graph/epoch preflight precedes publication, old playback may retire and MIDI
cleanup is requested atomically. Active/prepared recording is refused unchanged.
Engine node presets are prepared under a token/next epoch and become active during
processing only after that epoch is published; cancellation must discard presets
before another graph can publish. The engine wire job and adapter replacement are
implemented, along with service capture, retained reads, fenced admission and
host-owned completion. Client file conversion and platform workflows use the public service route;
packaged deployment and final integration validation remain pending. No rejected preparation may
leave old playback faded/stopped or mappings
partially replaced.

Validation evidence is recorded in the plan as milestones complete. Protocol tests
alone do not establish engine atomicity, capture coherence or restored UI support.
