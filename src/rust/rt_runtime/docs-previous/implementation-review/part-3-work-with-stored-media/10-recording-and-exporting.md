# Chapter 10: capture truthful prefixes and retain an immutable export

Destination currently empty; reviewed hashes/checks are in `../media-baseline.json`.
Build a small public-API capture/export example rather than a general application.
The archive is coverage input, not a chapter to copy wholesale.

## Example behavior and assertions to implement

- Prepare an audio resource and start `begin_capture` at a known absolute frame.
  Append interleaved samples through required `RenderIo::capture_audio` at contiguous
  frames. Choose a first prefix that ends inside a page, acquire a snapshot, then
  append enough to mutate/fill that original tail and cross a page boundary.
- Export the earlier snapshot after later appends; assert its original bytes,
  duration, revision and actual capture-end `cut_frame` have not changed. A snapshot
  requested later than the last captured frame reports the actual prefix end,
  not an invented request-time recording duration. Currentness may become Stale
  while the snapshot remains readable.
- Stop capture, assert unchanged bytes/revision and inactive status, and read the
  full frozen take. Capture stop changes status, so an older status snapshot may
  be stale even without another data append.
- In a separate deliberately small-store branch, refuse a complete append before
  storing any of it. Assert the prior prefix, inactive capture and explicit
  incomplete reason. Reclaiming capacity must not silently resume that take after
  a gap; a new capture is an explicit operation. Keep an independent audio path
  running by handling this auxiliary refusal instead of propagating it from DSP.
- Add a tiny MIDI capture variant: complete packets with offsets in `[0,N)`, an
  empty positive-length block extending duration, and a held old-prefix snapshot.
  Assert resource-relative timestamps, equal-time order, exact serialized bytes,
  trailing silence and no partial refused packet. Channel-state indexing belongs
  to chapter12; do not pretend a legacy unindexed capture has bounded state seek.
- Release all leases on success, cancellation and refusal paths; remove unneeded
  content and check bounded reclamation/pool recovery off-render. Show how the
  returned request owns the advanced cursor/export buffer when using executor I/O.

## Distinctions to explain locally

Generic resource capture revisions advance at begin and successful positive-length
appends; stop does not advance them. They are not the loop library's take-level
musical revision clock, nor a partition-invariant count of recordings. Empty MIDI
bytes with positive elapsed frames still extend capture duration. Zero-frame API
calls and invalid-shape/timestamp refusals must not be described as successful
new content; not every invalid call halts a capture, whereas storage/work exhaustion
has the explicit incomplete-prefix behavior.

A running snapshot shares complete pages and copies only its partial mutable tail.
It needs a snapshot slot, a spare unreserved page when applicable and enough optional
copy budget. A refused snapshot leaves recording unchanged. Do not retry in a
callback until it succeeds. Required appends do not consume background transfer
credits; optional snapshots still do. Keep malformed input and interface faults
separate from normal storage exhaustion and from downstream MIDI sink overflow.

This chapter's single-resource transaction is **not** atomic stereo-resource-plus-
MIDI recording. Several sequential `begin_capture`/append/stop calls cannot promise
a common cut. Link chapter16 for prepared groups, final-lease promises and genuine
pre-render failure handling. Generic freeze may preserve an empty take; whether it
can play is a domain policy (the loop engine rejects empty record-to-play).

## Current source/checks

`resource/capture.rs`, `snapshot/currentness/read/release_snapshot` in `resource/mod.rs`
and tests `capture`, `resource_render`, `resource_execution`, `midi_index` establish
these behaviors. All relevant selected native tests were refreshed in debug/release;
no new chapter example or actual AudioWorklet was implemented by this review. Loop
facade revision/cut/export integration is separately recorded in Stage5/6 evidence.
Keep that attribution and historical timing/count data out of teaching prose.
