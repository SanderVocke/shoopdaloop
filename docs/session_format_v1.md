# Shoop session and exact loop-media formats, version 1

## Status

This document freezes the first persistence format for the pure-egui application. It is a fresh design. QML-era `.shl`, `session.1`, tar archives, and JSON `.smf` are intentionally unsupported.

## Common rules

- All files begin as ZIP64 containers and use Deflate lossless compression.
- The root entry is `manifest.json`, UTF-8 JSON with deterministic object fields and sorted collections.
- Every manifest has `format`, `format_version: { major, minor }`, and `document_version`.
- Version 1 readers accept major 1 and known/defaultable minor additions. Unsupported older or newer majors are rejected before session mutation.
- Archive paths are relative, normalized ASCII paths. Duplicate names, traversal, undeclared payloads, mismatched lengths/hashes, and configured resource-limit violations are errors.
- Payload records contain an uncompressed byte length and lowercase SHA-256. ZIP CRC remains an independent transport check.
- Counts and indices are unsigned 32-bit values unless otherwise stated. The format imposes no lower channel-count ceiling; readers may apply explicit byte/resource budgets.
- Stable IDs are unsigned 64-bit values and must be non-zero and unique in their entity namespace.

## Session container (`.shoop`)

`manifest.json` uses `format: "shoop-session"` and version `{ major: 1, minor: 0 }`. It contains:

- writer application version and source sample rate;
- global performance controls;
- ordered sync/main track groups;
- tracks, loops, channels, ports, buses, global ports, internal links, and external autoconnect names;
- selected and targeted stable loop IDs;
- composite timelines and script-composite state;
- scripts, MIDI-control configuration, and session-local settings;
- FX chain descriptors and exact opaque Carla state strings;
- captured FX-state records referenced by recorded channels;
- a sorted media index.

Transient loop mode/position, queued transitions, meters, driver/device handles, permissions, xruns, task state, dialogs, and machine-wide settings are not session data. Loaded loops start stopped.

### Audio payload

Each audio channel has its own `media/audio/<content-id>.f32le` entry. The bytes are the exact little-endian IEEE-754 `f32::to_bits()` sequence. The media index records frame count, byte count, and hash. Per-channel entries avoid aggregate codec channel limits and permit content sharing.

### MIDI payload

Each MIDI channel has a `media/midi/<content-id>.json` entry containing a `shoop-midi` document. Session channel metadata independently records loop length, start offset, preplay, mode, gain, and connected ports.

## Exact loop MIDI (`.shoop-midi`)

The exact format is also a ZIP64 container. Its manifest uses `format: "shoop-midi"` and version `{ major: 1, minor: 0 }` and contains:

- `sample_rate: u32`;
- `length_frames: u64`, independent of the last event;
- ordered `start_state` byte messages;
- ordered timeline events `{ frame: u64, order: u32, data: byte-array }`.

Events are relative to loop/channel time. Equal-frame event ordering is determined by `order`. Negative engine sentinel timestamps are never serialized. At the same sample rate, timestamps, duration, start state, ordering, and bytes are exact.

Standard `.mid` is an interoperability format, not canonical session storage. Import resolves tempo maps to absolute time and preserves stable event ordering. Export uses the highest practical standard timebase and reports the resulting maximum timing quantization.

## Exact loop audio (`.shoop-audio`)

The exact audio format is a ZIP64 container with `format: "shoop-audio"`, version `{ major: 1, minor: 0 }`, sample rate, ordered channel labels/roles, and one exact `f32le` payload per channel. It supports any channel count representable by `u32` and available resources.

Float WAV is the baseline standard cross-target audio format. Standard formats may impose their own channel/sample representation limits; the UI must advertise those limits and recommend `.shoop-audio` when exact arbitrary-channel output is required.

## Sample-rate conversion

A source-rate mismatch always requires confirmation before mutation.

- Enclosing durations and loop/data lengths use checked rational ceiling.
- Event positions and signed offsets use checked nearest conversion with ties away from zero.
- MIDI events that collide retain original `order`; converted events are clamped below a non-zero converted duration only when required.
- Audio channels are independently high-quality resampled to their declared converted frame count.
- Preplay, ringbuffer sizes, composite delays, and every other sample-domain value use the documented category rule.
- Conversion must not infer duration from media tails or introduce a spurious additional sync cycle.

## Transaction and safety contract

Decode, decompression, hashes, versions, schema/references, capabilities, and optional resampling finish before commit. Backend loading uses begin/chunk/finalize/commit/abort generations. Failure or cancellation leaves the previous session usable.

Saving captures scalar state and all settled channel content from one validated generation. Playing is not a content mutation and must continue. Recording, replacement, loading, clearing, or grab adoption yields an explicit wait/retry/cancel or rejection rather than a mixed-generation save.

Native output uses a temporary sibling and atomic replacement. Browser output is fully validated before download publication, or closes a transactional writable handle. Archive/codec/resampling/filesystem work never runs in the realtime callback.
