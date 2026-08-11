# Shoop session and exact loop-media formats, version 1

## Status

This document freezes the first persistence format for the pure-egui application. It is a fresh design. QML-era `.shl`, `session.1`, tar archives, and JSON `.smf` are intentionally unsupported.

## Common rules

- All Shoop-native files (`.shoop`, `.shoop-audio`, and `.shoop-midi`) are ZIP64 containers and use Deflate lossless compression. Standard `.wav` and `.mid` exports retain their standard container formats.
- The root entry is `manifest.json`, UTF-8 JSON with deterministic object fields and sorted collections.
- Every manifest has `format`, `format_version: { major, minor }`, and `document_version`.
- Version 1 readers accept major 1 and known/defaultable minor additions. Unsupported older or newer majors are rejected before session mutation.
- Archive paths are relative, normalized ASCII paths. Duplicate names, traversal, undeclared payloads, mismatched lengths/hashes, and configured resource-limit violations are errors.
- Payload records contain an uncompressed byte length and lowercase SHA-256. ZIP CRC remains an independent transport check.
- Counts and indices are unsigned 32-bit values unless otherwise stated. The format imposes no lower channel-count ceiling; readers may apply explicit byte/resource budgets.
- Stable IDs are unsigned 64-bit values and must be non-zero and unique in their entity namespace.

## Session container (`.shoop`)

`manifest.json` uses `format: "shoop-session"` and version `{ major: 1, minor: 0 }`. It contains:

- writer application version, source sample rate, and defaultable `connection_model_version`;
- global performance controls;
- ordered sync/main track groups;
- tracks, loops, channels, ports, buses, global ports, internal links, and external autoconnect names;
- selected and targeted stable loop IDs;
- composite timelines and script-composite state;
- scripts, MIDI-control configuration, and session-local settings;
- FX chain descriptors and exact processor-state strings for Carla and Tiny Synth/FX;
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

Standard `.mid` is an interoperability format, not canonical session storage. Import resolves tempo maps to absolute time, merges tracks in stable source order, and preserves MIDI and SysEx bytes. Export uses SMPTE 30 fps with 255 subframes (7,650 ticks/second), includes duration/end-of-track information, and reports the measured maximum frame quantization. Select exact `.shoop-midi` when integer-frame identity is required.

## Exact loop audio (`.shoop-audio`)

The exact audio format is a ZIP64 container with `format: "shoop-audio"`, version `{ major: 1, minor: 0 }`, sample rate, ordered channel labels/roles, and one exact `f32le` payload per channel. It supports any channel count representable by `u32` and available resources.

Float WAV is the baseline standard cross-target audio format. The current native and browser adapter reads/writes float WAV and the exact Shoop format; no additional native sound-file adapter is selected in v1. Export presents an ordered channel selection, and import requires an explicit source-to-destination mapping (duplication is permitted). Direct channels are labeled `Direct N`; processed tracks expose ordered `Dry N` then `Wet N` audio destinations, and dry MIDI remains the only MIDI role. Dry-only, wet-only, and mixed/reordered exports are supported. Use `.shoop-audio` when exact arbitrary-channel output is required.

### Dry/wet processor topology

`DryWetExternal` stores independent `dry_audio_channels`, `wet_audio_channels`, and `dry_midi`. Public ports preserve Audio input/send/return/output and MIDI input/send roles plus exact confirmed host IDs. `Carla` stores its chain type and legacy equal-count `audio_channels`/`midi` fields; optional `dry_audio_channels` and `wet_audio_channels` preserve new unequal shapes. When those optional fields are absent, readers interpret both counts as the legacy `audio_channels` value.

`TinySynthFx` stores one `audio_channels` count. It always means the same number of dry inputs and wet outputs plus exactly one dry MIDI input; zero audio channels is valid. Its chain type is also `TinySynthFx`. The stable runtime processor ID is `tiny_synth_fx`, while **Tiny Synth/FX** is only its display label.

Loop channels remain ordered dry audio, wet audio, then optional dry MIDI and carry `mode: "dry"` or `mode: "wet"`. Processed tracks store the current state string in `fx_chain.internal_state`. A wet recording may reference an automatic `fx_states` entry through `recording_fx_state_id`; that entry's chain type must match the current track. Only referenced automatic take states are written.

Carla state remains opaque UTF-8 and byte-for-byte significant, including newlines, Unicode, and NUL characters. Tiny Synth/FX state uses `shoop-tiny-synth-fx:1:<gain-bits>:<base64>`: eight lowercase hexadecimal IEEE-754 bits encode finite master gain in `-60..=0 dB`, and unpadded base64 encodes at most 256 KiB of canonical `tinyviolin::AudioProcessor::serialize_state` bytes. Unknown envelope versions, malformed base64, out-of-range gain, oversized payloads, or a payload rejected by `AudioProcessor::load_state` are errors. Voices, oscillator phase, effect tails, panic history, and editor visibility are intentionally transient.

Native and browser runtimes instantiate Tiny Synth/FX transactionally before publishing a replacement session. Native External and advertised Carla processors follow the existing native-only path. A runtime whose processor catalog does not contain the required identity rejects the document before backend mutation; browser builds therefore reject External and Carla without interrupting AudioWorklet progress, but preserve Tiny Synth/FX without flattening it.

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

Native output uses a temporary sibling, flushes it, and atomically renames it; reads and writes run outside the GUI/application actor after picker selection. Browser upload/download uses asynchronous `rfd` file handles and Blob/download fallback according to browser capability. Picker handles, paths, and browser objects never enter `AppSnapshot` or a session document. Platform failures are reported back as typed task errors.

Archive/codec/resampling/filesystem work never runs in `process()`. Native session compression runs on a worker thread. Browser codec work runs on the UI/control side while the AudioWorklet independently continues bounded render callbacks; session transfer uses 2 KiB generation-tagged chunks and a 256 MiB transfer ceiling.

## Limits, recovery, and compatibility

Default archive limits are 1,000,000 entries and 16 GiB total declared uncompressed payload. Each declared size is checked before allocation; actual practical memory and browser transfer limits may be lower and fail explicitly. AudioWorklet recording storage remains hard-bounded to the documented 120 seconds per channel. The format itself uses `u32` channel counts and has no ten-channel persistence limit; the physical Web Audio device boundary remains negotiated separately and the engine deterministically mixes all loop channels to its stereo destination.

Malformed paths, duplicate entries, unknown/undeclared payloads, count/size overflow, CRC/SHA mismatch, unsupported version/capability, and interrupted staged replacement fail without publishing a partial session. Retry by correcting/selecting another file. Cancellation before commit leaves the prior model/backend mapping intact. A save request made during recording/replacement is explicitly rejected until content settles; playing does not block saving and is not transitioned.

The current egui application can instantiate direct and Tiny Synth/FX sync/main track topology plus source-bearing session scripts on native and browser targets. Native builds additionally instantiate External and advertised Carla Rack/Patchbay/Patchbay16x dry/wet tracks, preserve role-bearing media and links across driver switches, and restore current and compatible recorded-take processor state before publication. Script source is syntax-checked before commit, activated only after the shared session replacement commits, and captured exactly on save. Version-1 documents now write `connection_model_version: 1`; a missing/zero value identifies a pre-normalized document. On browser load only, that marker migrates the former implicit Web Audio mapping to explicit default routes. New documents persist exact confirmed host IDs, including intentional disconnections, so session replacement removes startup defaults before restoring saved links. Deferred buses, generic MIDI-control configuration, and session-local settings remain codec-representable but cause a capability error if runtime instantiation would be required. Unknown or unavailable track processors likewise fail transactionally rather than flattening to direct topology.

QML-era `.shl`, `session.1`, tar/JSON/FLAC archives, and JSON `.smf` are not sniffed or migrated. They produce an unsupported-format error and leave the running session unchanged.
