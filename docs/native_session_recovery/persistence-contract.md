# Core persistence contract

Implementation contract for [the persistence plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_SESSION_PERSISTENCE_MEDIA_IO.md).
The core slice is implemented and validated;
[acceptance evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/session-persistence.md) records its scope.
The [feature backlog](feature-parity.md#missing-features) assigns remaining
save/load/store/export requirements to Carla (P07), latency compensation (P09),
take alignment (P10), duplication/MIDI state (P11) and session scripts/resources
(P16). Historical P01/P02 are no longer standalone backlog items.

## Shared audio reconciliation

Prepared shared audio uses the same `ResourceId`, immutable `Snapshot`, read index
and byte cursor as uploaded PCM. Capturing a snapshot pins an immutable audio
export; session capture never borrows a mutable take's current read index.
`NodePairSnapshot`, `Adopt`/`Select`, media-ledger exports and
`ConfigurationReplace` remain the publication and replacement authorities.
An upload may replace a shared resource without changing already pinned exports.

Loaded PCM can become an editable COW resource while retaining its uploaded
snapshot as an immutable base. Editing copies touched pages; untouched bytes and
old exports remain readable, including after resource removal. Logical loop
length is independent of the rolling-history horizon. Prepared page, node,
transaction and owner quotas still govern admission. Registered audio stores are
lifetime-scoped and shared snapshots fork within their originating store.

Session/audio codecs continue to receive logical samples and metadata, so the
archive and protocol formats do not encode sharing, page addresses or edit roots.
Tests cover grabbed and imported sources through adoption, playback, full
configuration replacement, retained exports and bounded teardown, plus session,
exact-audio and WAV round trips for original and edited revisions. Application Grab and Replacing use these same formats and immutable owners;
[completed application evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/grab-replace.md) covers P04/P05 exact
save/load/re-edit across native/browser hosts and configured firmware.
Active or scheduled Replacing obeys the recording conflict policy. Default Grab
persists, while histories, accepted input jobs and private writers do not. Reload
uses fresh identities and stopped loops; old exports retain their original data.

## Field and capability matrix

| Document/live data | Core behavior | Ownership and refusal |
| --- | --- | --- |
| Sample rate and processing quantum | Confirm resampling/retiming once, then convert audio, MIDI and timing together into the prepared format | Client codecs; recheck rate/Q and content before commit; never switch the device implicitly |
| Direct audio/MIDI/trigger tracks and primitive loops | Preserve names, order, topology, media, supported controls and sync relationships | Service maps document IDs to fresh live IDs; RT owns playback/resources |
| Loop/channel length, gain, offsets, capture alignment and preplay | Preserve supported stored values and exact media timing | Validate every value; reject an unsupported non-default value before mutation; P12 timing edits, including nonzero pre-play, are supported; P09/P10 editing retains its own gates |
| Selection, target and global policies | Preserve document values supported by the core | Service-owned; reject unsupported recording/default-playback policies |
| Ordinary buses, ports and mixer routes | Preserve supported routing/control values and connection intent | Resolve external ports by exact identity; report missing destinations and retain intent |
| Playback/recording position and pending musical commands | Do not persist transient execution | Load loops stopped; cancel/fence prior-session work at commit; never replay a saved operation |
| Core composites | Preserve authored identities, original timing basis, frozen geometry, nested references, explicit sync and repeat intent; load stopped with fresh runtime identities | See the [composite contract](composites.md) and [acceptance evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/composites.md); captures exclude pending candidates and refuse active/scheduled/future recording |
| Built-in FX/OxiSynth, External dry/wet, bus FX and dependent composite events | Preserve processor state/mappings, recorded settings, role media and authored modes | Supported under P03/P06/P08/P17 and the built-in portion of P07; prepare privately and publish atomically, without flattening or dropping state |
| Carla and general latency/take controls | Refuse complete documents requiring unavailable capabilities | Remaining storage, capture, load and applicable export work belongs to P07/P09/P10 |
| Session-owned scripts/resources | Refuse documents requiring unavailable ownership/resource handling | P16 remains open; VM stays client-side and is never serialized/restarted |
| MIDI control bindings (`id`, `message`, `action`, `target_id`) and session settings (`key`, typed `value`) | Save empty collections; reject nonempty collections, matching reference `session_bundle_to_backend` | These reserved document workflows have no current runtime implementation; machine preferences stay local |
| File names, URLs, picker state, output bytes and task progress | Existing platform workflows and truthful completion/cancel/error states | Client-owned; never place filesystem paths or codecs in engine commands |

## Finite budgets

| Resource | Per-job limit / admission rule |
| --- | --- |
| Metadata | 8 MiB, including the persistence manifest |
| Decoded media | 256 MiB aggregate audio/MIDI, independent of the old 32 MiB media-view message |
| Input/output file | 512 MiB compressed/encoded bytes; reject before decoding/output allocation beyond this budget |
| Core document shape | At most 64 tracks and 256 primitive loops, with ≤2 direct audio channels and ≤1 MIDI channel per loop; zero-or-more ordinary buses within the negotiated pool (at most eight buses of eight channels), and supported routes |
| Transfer objects | 4096; object zero is the sole metadata object, followed by ordered audio/MIDI objects |
| Native wire preparation | Validate/index ≤256 KiB per client connection poll before streaming; cancellation and detach discard pending validation and upload |
| Native wire turns | At most 64 persistence records per connection turn (≤64 KiB of media), with at most 64 upload records awaiting replies; ordinary commands retain their 16-record limit. Outbound accounting counts emitted records and bounds encoder cleanup separately. This batches file work across platform timer wakeups without increasing RT buffer ownership |
| Wire chunk | 1024 bytes; fits the existing 1280-byte carriers with identities/envelope |
| MIDI | At most 1,000,000 events per job, additionally bounded by decoded bytes and runtime packet/index capacities |
| MIDI initial/preroll state | At most 16,384 short channel or System Reset packets per loop; parameter history that remains untracked after the complete prefix and vendor/system initial state refuse before activation (P11) |
| Wire MIDI read index | At most 8 MiB plus one offset per object; built only for byte delivery, alongside retained immutable media |
| Adapter read assembly | ≤16 outstanding bulk records per read. Audio fills final channels on arrival; MIDI has one ≤256 MiB byte builder and decodes ≤256 KiB per pump, with one complete ≤1 MiB packet atomic. Event/packet allocation remains bounded by one million events and the media budget |
| Engine upload window | At most 64 queued writes when WIDE_CONTENT_UPLOAD is advertised, otherwise eight; cap the window by reliable credits. Both use a fixed pool of 32 prepared runtime buffers. Chunks divide a resource page and are at most 1024 bytes. Preparation adds at most 32 KiB of upload copy allowance per callback, separately from existing render/read reservations, and reserves validation for one MIDI chunk. Queue depth alone does not change these finite budgets |
| Adapter preparation | Validate ≤256 KiB per pump before issuing engine preparation; one ≤1 MiB MIDI packet is atomic. Cache byte counts only with a proof retaining the immutable content. Capture checks remaining aggregate capacity before allocating each medium |
| Browser progression | Main-thread service/client pump on engine replies, every 4 ms and on UI turns, sharing one elapsed-time clock. Each turn retains the existing record/media work bounds; dropping the runtime clears the interval |
| Browser codec transport | Sixteen in-flight worker envelopes, each ≤64 KiB. Image envelopes contain at most sixteen bounded persistence parts; send/receive media totals at most 256 KiB per poll. Cooperative image validation and assembly each step at most 256 KiB; codecs remain in the dedicated worker |
| Concurrent host persistence work | One staging or capture job; normal controls/snapshots continue with bounded pump work |
| Media-read handoff | Bulk reads wait for all previously queued read cancellations to finish. Ordinary commands continue while cancellation retires old exports; a later cancellation cannot consume a new view's read |
| Client file input | One platform reader plus one queued raw input, each ≤512 MiB; one codec task. Cancellation fences callbacks and checks file chunks. Browser URL cancellation wakes the pending future; a native reader keeps its slot until it exits or the HTTP timeout fires. A new input refuses while that slot is closing |
| Archive expansion | At most 4097 entries and `5 × 256 MiB + 8 MiB` expanded bytes; metadata manifests remain 8 MiB. The factor allows decimal MIDI JSON overhead. Check declarations before extraction and actual shapes/counts while decoding |
| Native codec lifetime | One active task per client; process-wide pool of two workers with eight queued jobs. Dropped clients cancel queued work and discard draining results. Overload refuses explicitly |
| Platform output | One outstanding writer/download per UI root, retained across runtime changes; task IDs remain unique across client runtimes and completion is client-local even after backend loss |
| Mutation completion | Connected load/import/New Session tasks wait for the confirmed state revision before reporting readiness. A known committed result remains successful after connection loss, with a reconnect diagnostic; an unconfirmed operation is never replayed |
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
- Load/import: read/decode, resolve channel mapping, plan exact timing, ask for
  conversion consent if needed, recheck
  the target, stage, commit and await the confirmed result. Decline/cancel before
  commit leaves playback/content intact. New Session uses safe replacement too.
- Audio import/export keeps reference channel selection/role order and mapping;
  importing one medium preserves the other. Only reference-prescribed offset resets
  and explicitly requested length updates apply. Exact MIDI preserves event bytes,
  frame positions and ordering; SMF export reports quantization/format loss.
- Unsupported session fields produce complete useful capability diagnostics before
  mutation. They are not omitted from save or stripped during load. Session script
  persistence remains a named follow-up. Audio/MIDI click generation
  and insertion use the restored bounded import path. System/Master audio preview
  is complete under P15; see the [preview contract](preview.md).

## Loop copying

Clone uses the first empty primitive slot below the source on the same track,
adding an aligned row when necessary. Clone into replaces an explicit same-track
destination. The service captures and loads complete primitive audio/MIDI role
media and recorded processor state, then copies name, gain/balance, sync and repeat
settings. No file provider or codec round trip is required. The result keeps its
slot identity, selection and targeting, and starts stopped at position zero.

P03 Direct-track regular and scripted Play/Stop/Record definitions, including
nesting, use the existing prepared-composite editor. Copies have fresh event and
prepared-version ownership and preserve timing basis, frozen geometry and child
references. Editing either definition is independent; changing a shared child
affects both. A composite can replace another P03 composite or an empty primitive.
Occupied cross-kind conversion and P17 definition duplication refuse before mutation.
Affected recording, future composite writes, unsettled edits, cycles and conflicting
active parent/child ownership also refuse without stopping that recording.

Copying is a host-owned sequence: capture/validate, add slots, Stop, apply content
or activate the definition, then apply controls and metadata. Existing content is
never cleared first. Failure before content/definition commit preserves it, although
the destination may remain stopped and confirmed empty row additions may remain.
After commit, a later failure reports that content was copied and identifies the
remaining step; acknowledged media is not rolled back. Unknown engine outcomes
are reported as unknown. UI reconnect reconciles the same accepted operation
without replaying capture, row creation or application. Host/engine crash recovery
and expanded MIDI state/preroll/seek/history support remain outside this contract.

## Engine integration boundary

Session loads use one rational dependency plan and one anchored
audio/MIDI conversion from the original media. Lengths round to the nearest
positive quantum, with ties upward and empty loops remaining empty. Source-first
propagation preserves exact cycle multiples. MIDI timestamps retain individual
frame precision. Signed offsets, capture alignment and stored prefix/suffix history
follow the same origin map; hardware calibration scales only with sample rate.
Differing recorded quantum provenance alone does not require conversion.

For loop `i`, first compute its exact ideal duration `I_i = L_i * target_rate /
source_rate_i`. Process sync sources before their dependents. A dependent of `s`
uses `I_i * converted_length_s / I_s` before rounding to the grid; a zero-length
source contributes no correction. Each child's correction already contains all
upstream corrections, so it is applied once. Empty loops stay empty and nonempty
loops get at least one quantum. Reject missing links, dependency cycles and checked
arithmetic overflow before mutation. Convert each loop's original media once using
its final length ratio, rather than resampling at each dependency edge.

Sample-rate changes or a nonidentity timing map require the same Convert/Cancel
warning: resampling and retiming slightly affect audio and loop timings. No preview
is required. Consent covers the complete affected dependency family; cancellation
before admission leaves it unchanged.

A live import first defines its intended length using the normal keep/update
choice. Only its automatic grid correction propagates to retained channels and
sync descendants. The client captures changed loops individually, preserving
unrelated recording. The family manifest carries rate/Q, loop identities, links,
old lengths and revisions; the service and adapter check these again before the
engine's single tick publication. Audio, MIDI, periods and mapped dependent
playheads change together. Current gains, names and routes are preserved.
Affected active/prepared captures or stale preparation refuse the entire cut.
Cancelled preparation releases staged owners; an admitted commit remains engine
owned across controller loss. Application protocol version 20 carries resolved
sync links and target format, and RT protocol version 7 carries bounded family
staging and authoritative content revisions. Typed local and encoded transports
use the same operations.

The adapter stages a private mirror instead of removing live tracks. It uses
prepared loop node-pair sources, immutable exports and configuration transactions,
with an engine transaction that preflights complete content, graph,
timing and retirement ownership. The existing `ConfigurationActivate` requires
already-stopped edited owners; a scheduled stop alone is insufficient.
`ConfigurationReplace` provides for a complete fresh-owner cut at callback start:
media/graph/epoch preflight precedes publication, old playback may retire and MIDI
cleanup is requested atomically. Active/prepared recording is refused unchanged.
Engine node presets are prepared under a token/next epoch and become active during
processing only after that epoch is published; cancellation must discard presets
before another graph can publish. The engine wire job and adapter replacement are
implemented, along with service capture, retained reads, fenced admission and
host-owned completion. Client file conversion and platform workflows use the public service route;
packaged native and browser deployment validation is recorded in the
[acceptance evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/session-persistence.md). No rejected preparation may
leave old playback faded/stopped or mappings
partially replaced.

Validation evidence is recorded in the [acceptance evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/session-persistence.md). Protocol tests
alone do not establish engine atomicity, capture coherence or restored UI support.

## Native driver transitions

Confirmed native driver changes reuse immutable capture and prepared restoration,
with public-ID preservation instead of ordinary load's fresh live IDs. The host
owns conversion and rollback captures independently of a client's bulk transfers.
The shared conversion layer supplies the same timing and media rules to session
files and native driver transitions. Preference persistence has a separate retry
result; see [audio-driver switching](audio-driver-switching.md).
