# Diagnostic stream contract

Both engine and application protocols carry `profiling_protocol::Record` values.
The envelope owns framing, discovery, source lifecycle and generic controls;
report owners own payload schemas and validation. Type IDs are open integers.
Unknown type/version data remains opaque to intermediate peers and is skipped by
consumers lacking a decoder. Outer protocol versions still require an exact match.

The record ceiling remains 1,280 bytes. Diagnostic payload fragments contain at
most 1,024 bytes; encoded generic records are at most 1,200 bytes, leaving room
for the engine/app headers. An ordered multipart report has a nonzero sequence,
source ID/epoch, report-type ID, schema version, byte offset and total size.
Offsets must be contiguous. A new report begins at offset zero; it retires any
incomplete older report. Completed or older sequences cannot be replayed. A new
epoch retires all producer-owned state; epochs increase within an attachment.
Attachment/authority changes separately clear all client state.

At most eight catalog slots (including retired source tombstones) are registered, each with one active outgoing report and
one incoming assembly. A report and aggregate retained report bytes are capped
at 64 MiB; allocation grows only with accepted fragments, never an advertised
length. This preserves table-only audio views for up to 65,536 rows per interval,
as well as the existing graph (4,096 nodes/16,384 edges), 8,192 capture summaries
and paged detail bounds. Capacity refusal is explicit rather than silently
truncating an audio view. Platform providers additionally bound their own data;
the FreeRTOS task table is limited to 32 tasks. Client task assemblies are capped
at 64 KiB; audio assemblies reserve that much aggregate headroom for host data.
Task sampling generations change on re-enable so CPU deltas never span a paused
collection interval, even when intermediate status messages are coalesced.

Catalogs/status use the same bounded source set. Subscribe commands include an
interval; the source acknowledges its effective interval, availability and
outcome. Discover, snapshot, unsubscribe and payload-specific inspect requests
use generic routing. Inspection payloads are at most 128 bytes. Discovery is
bounded and repeatable. Unknown sources/unsupported operations receive an explicit
refusal. Optional collection starts disabled; the existing saved audio preference
is applied by the application after attachment. Effective demand is scoped to the
active service lease, which is currently exclusive; connection candidates do not
multiply collection work.

Collection, service delivery and client assembly losses are separate counters.
Whole supersedable snapshots can be coalesced, never individual fragments of an
active report. Source scheduling rotates with a fixed per-turn record budget;
controls, heartbeats and shutdown retain priority. An in-progress TCP record and
bytes already accepted by the network cannot be preempted. There is no hard
control-latency guarantee for a stalled connection.

Audio source IDs and payload IDs are declared by `rt_profiling::wire`. Raw records
and derived UI views have separate type IDs; graph generations/callback identities
remain audio payload fields. Host task sources are declared by their typed schema.
Graph resets do not reset host task histories. Timestamps identify their clock
domain; audio frame positions do not substitute for elapsed execution time.

Task CPU usage is `100 * delta(task runtime) / delta(total runtime)` for the current
single-core host. First samples, failed samples, reset epochs and ambiguous wraps
have unavailable CPU values. Runtime counters are scheduler accounting and may
include interrupt execution. Stack watermarks are historical minimum unused bytes,
not current usage. Kernel runtime accounting incurs a context-switch cost even
when task report collection is disabled. Sampling initially runs at 1 Hz;
watermark scanning and scheduling suspension are measured separately from render
kernel timings. No scheduler-event trace is collected.

Removing a provider cancels its demand/transfers and emits an unavailable descriptor
with a fresh epoch. Catalog slots are prepared for the service lifetime; removing
a provider does not permit unbounded source-ID churn. Discovery metadata may be
received while the initial session snapshot is synchronizing, but controls and
sampling demand require the normal Ready/authority handshake. Saved audio enable
preferences wait for discovery rather than guessing a producer epoch. Backend
connection changes, including loss, retire the audio source epoch independently
of graph generations and host task reporting.
