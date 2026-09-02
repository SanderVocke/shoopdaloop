# Shoop session and exact loop-media formats, version 1

## Status

This document defines the first application persistence format. Predecessor `.shl`, `session.1`, tar archives, and JSON `.smf` are intentionally unsupported.

## Common rules

- All Shoop-native files (`.shoop`, `.shoop-audio`, and `.shoop-midi`) are ZIP64 containers and use Deflate lossless compression. Standard `.wav` and `.mid` exports retain their standard container formats.
- The root entry is `manifest.json`, UTF-8 JSON with deterministic object fields and sorted collections.
- Every manifest has `format`, `format_version: { major, minor }`, and `document_version`.
- Format-major 1 readers write session `document_version: 13`. Version 6 is accepted through an explicit migration that assigns zero capture alignment to every channel, versions 6 and 7 migrate the former processor advance to Manual processor-latency mode, versions 6–8 assign regular default playback to every track, pre-mixer documents receive a disconnected default Master, buses without controls receive `0 dB`/center/unmuted, version 9 or 10 Built-in FX tracks migrate from fixed stereo/no-MIDI/state-v1 to matching channels plus a disconnected MIDI input and state-v2 defaults, and versions through 12 derive bus display order from their bus list. Older and newer document versions are rejected before session mutation.
- Archive paths are relative, normalized ASCII paths. Duplicate names, traversal, undeclared payloads, mismatched lengths/hashes, and configured resource-limit violations are errors.
- Payload records contain an uncompressed byte length and lowercase SHA-256. ZIP CRC remains an independent transport check.
- Counts and indices are unsigned 32-bit values unless otherwise stated. The format imposes no lower channel-count ceiling; readers may apply explicit byte/resource budgets.
- Stable IDs are unsigned 64-bit values and must be non-zero and unique in their entity namespace.

## Session container (`.shoop`)

`manifest.json` uses `format: "shoop-session"` and version `{ major: 1, minor: 0 }`. It contains:

- writer application version, source sample rate, and defaultable `connection_model_version`;
- global performance controls;
- ordered sync/main track groups;
- tracks, loops, channels, ports, zero or more named arbitrary-channel buses, their explicit visual order, explicit track-output-to-bus routes, global ports, internal links, and external autoconnect names;
- selected and targeted stable loop IDs;
- composite timelines and script-composite state;
- script bundle descriptors, MIDI-control configuration, and session-local settings;
- FX chain descriptors and exact processor-state strings for Carla, Built-in FX, and Built-in Synth;
- variable matching-channel Built-in FX and fixed Built-in Synth topologies, including rack controls, MIDI assignments, and the OxiSynth preset/additive sends;
- captured FX-state records referenced by recorded channels;
- each track's default playback mode plus independent recording-alignment and processor-latency adjustment modes and their signed manual/trim inputs; automatic observations are transient;
- one signed capture alignment per channel (introduced in document version 7), allowing Dry and Wet annotations to differ;
- a sorted media index.

Global performance controls include the script-composite track-input auto-arm policy. It is saved as `auto_arm_track_inputs`; documents without the field default it on. Auto-arm's transient per-track demand and ownership are not persisted.

Transient loop mode/position, queued transitions, bus and track meters, driver/device handles, permissions, xruns, task state, dialogs, and machine-wide settings are not session data. Loaded loops start stopped. Every bus stores semantic `gain_db`, `balance`, and `muted` fields separately from its canonical unity/unmuted output-port transport records, preventing controls from being applied twice during replacement. `bus_display_order` is a permutation containing each bus ID exactly once and affects presentation only; bus records remain canonical by stable identity.

### Audio payload

Each audio channel has its own `media/audio/<content-id>.f32le` entry. The bytes are the exact little-endian IEEE-754 `f32::to_bits()` sequence. The media index records frame count, byte count, and hash. Per-channel entries avoid aggregate codec channel limits and permit content sharing.

### Script bundle payloads

Session document version 3 stores each script as `{ id, name, entrypoint, enabled }`; it never stores source inline or records a machine path. The manifest has a sorted script-resource index. Every record declares its owner script ID, normalized relative path, resource kind (`lua`, `markdown`, or `image`), exact uncompressed byte count, lowercase SHA-256, and archive path `scripts/<script-id>/<relative-path>`. The entrypoint must be a declared Lua resource. Resource names are scoped by owner, so different scripts may use the same relative names without collision.

Script payloads are immutable bytes and are not extracted when loaded. Native and browser runtimes provide them directly to `shoop_file.load`, `dialog.markdown_file`, and the Markdown image loader. Absolute paths, empty or dot components, backslashes, traversal, case-colliding or duplicate normalized paths, owner/path mismatches, undeclared entries, unsupported types, malformed hashes, and cross-script lookups are rejected before session commit.

Source-only session documents from document versions 1 and 2 migrate in memory to a `main.lua` one-entry bundle. New saves always write document version 3. Unsupported future document versions fail before payload construction or application mutation.

When a filesystem script is included in a session, the application keeps the currently running Lua source and recursively captures regular Markdown and PNG files below the Lua file's parent. It does not follow directory symlinks and rejects escaping file symlinks. The conversion uses limits of 16 MiB per file, 64 MiB and 10,000 files per script, and 256 MiB aggregate script resources. A scan/read/limit/staleness failure leaves ownership unchanged. Source-only scripts produce an entrypoint-only bundle, while an existing bundle is reused when converting away from and back to session ownership.

### MIDI payload

Each MIDI channel has a `media/midi/<content-id>.json` entry containing a `shoop-midi` document. Session channel metadata independently records loop length, start offset, signed capture alignment, preplay, mode, gain, and connected ports.

## Exact loop MIDI (`.shoop-midi`)

The exact format is also a ZIP64 container. Its manifest uses `format: "shoop-midi"` and version `{ major: 1, minor: 0 }` and contains:

- `sample_rate: u32`;
- `length_frames: u64`, independent of the last event;
- ordered `start_state` byte messages;
- ordered timeline events `{ frame: u64, order: u32, data: byte-array }`.

Events are relative to loop/channel time. Equal-frame event ordering is determined by `order`. Negative engine sentinel timestamps are never serialized. At the same sample rate, timestamps, duration, start state, ordering, and bytes are exact.

Standard `.mid` is an interoperability format, not canonical session storage. Import resolves tempo maps to absolute time, merges tracks in stable source order, and preserves MIDI and SysEx bytes. Normal export applies the channel's capture alignment and emits the logical loop window. Export uses SMPTE 30 fps with 255 subframes (7,650 ticks/second), includes duration/end-of-track information, and reports the measured maximum frame quantization. Select exact `.shoop-midi` when integer-frame identity is required.

## Exact loop audio (`.shoop-audio`)

The exact audio format is a ZIP64 container with `format: "shoop-audio"`, version `{ major: 1, minor: 0 }`, sample rate, ordered channel labels/roles, and one exact `f32le` payload per channel. It supports any channel count representable by `u32` and available resources.

Float WAV is the baseline standard cross-target audio format. Normal WAV and exact Shoop audio export apply the channel's capture alignment and emit the logical loop window. The current native and browser adapter reads/writes float WAV and the exact Shoop format; no additional native sound-file adapter is selected in v1. Export presents an ordered channel selection, and import requires an explicit source-to-destination mapping (duplication is permitted). Direct channels are labeled `Direct N`; processed tracks expose ordered `Dry N` then `Wet N` audio destinations, and dry MIDI remains the only MIDI role. Dry-only, wet-only, and mixed/reordered exports are supported. Use `.shoop-audio` when exact arbitrary-channel output is required.

### Dry/wet processor topology

`DryWetExternal` stores independent `dry_audio_channels`, `wet_audio_channels`, and `dry_midi`. Public ports preserve Audio input/send/return/output and MIDI input/send roles plus exact confirmed host IDs. `Carla` stores its chain type and legacy equal-count `audio_channels`/`midi` fields; optional `dry_audio_channels` and `wet_audio_channels` preserve new unequal shapes. When those optional fields are absent, readers interpret both counts as the legacy `audio_channels` value.

`BuiltInFx` stores a positive `audio_channels` count. It means that many dry audio inputs, the same number of wet audio outputs, and exactly one dry MIDI input. Its chain type and stable runtime processor ID are `BuiltInFx`/`builtin_fx`; **Built-in FX** is the display label. The chain's `internal_state` must be canonical Built-in FX state. Mono uses mono processing, exactly two channels use stereo-specific behavior, and larger counts remain isolated independent channels.

Each track stores `default_playback_mode` as `regular` or `dry_through_wet`. Dry-through-wet is valid only for non-sync processed tracks that support wet playback. Built-in FX, `DryWetExternal`, Carla, and OxiSynth tracks with at least one wet audio channel support it; direct, trigger, zero-wet, and sync tracks must use regular playback. The value belongs to the track rather than its loops or composite events. Mode-less regular-composite events mean dynamic default playback, while script-composite event modes remain explicit.

`OxiSynth` stores no variable channel fields. It always means exactly two dry audio inputs, exactly two wet audio outputs, and one dry MIDI input. The dry audio inputs preserve the standard stereo track shape but their samples are ignored by the synth. Its chain type and stable runtime processor ID remain `OxiSynth`/`oxisynth`; **Built-in Synth** is the display label. The chain's `internal_state` must contain valid version-2 OxiSynth state, and no automatic recorded-take `fx_state` is written. Session document version 13 is current; version 6 receives the explicit zero-alignment migration, versions 6 and 7 migrate their processor advance to Manual mode, versions 6–8 receive regular track default playback, pre-mixer documents receive disconnected mixer defaults, buses without controls receive neutral controls, version 9 or 10 receives the Built-in FX migration described above, and versions through 12 derive bus display order.

`global_ports` contains either no global FX control port in legacy version-1 documents or exactly one canonical **Global FX Control MIDI In** port. Its shape is MIDI input, external input/internal output connectability, unity gain, unmuted, passthrough-muted, no internal links, and zero capture frames. New saves include it with exact external endpoint identities. A legacy omission migrates to a disconnected canonical port; conflicting IDs, multiple ports, or another shape are rejected before backend mutation. Runtime pending controller values are transient and are not serialized.

Loop channels remain ordered dry audio, wet audio, then optional dry MIDI and carry `mode: "dry"` or `mode: "wet"`. Processed tracks store the current state string in `fx_chain.internal_state`. A wet recording may reference an automatic `fx_states` entry through `recording_fx_state_id`; that entry's chain type must match the current track. Only referenced automatic take states are written.

Built-in Synth `fx_chain` records may contain a defaultable `midi_cc_assignments` list owned by ShoopDaLoop. Each entry identifies `reverb_send` or `chorus_send`, a zero-based source MIDI channel in `0..=15`, and a controller in `0..=127`. Built-in FX records use a separate defaultable `builtin_fx_midi_cc_assignments` list whose target is one of the 23 continuous rack parameters. Within each processor, targets and channel/controller sources must each be unique; assignments owned by the other processor type are invalid. A missing list means no assignments. These mappings belong only to the current track chain and are not copied into automatic recorded-take `fx_states`.

Built-in FX state is a canonical colon-delimited `shoop-builtin-fx:2` envelope. In fixed rack order it stores each stage enable, Drive/Modulation/Reverb type tag, and every continuous parameter as a lowercase eight-hex-digit finite IEEE-754 value. Boolean fields are exactly `0` or `1`; type tags are `saturation|overdrive|distortion|fuzz`, `tremolo|flanger|phaser`, and `room|hall|plate`; numeric fields must fall within the documented editor ranges. Fields are ordered Compressor, Drive, EQ, Chorus, Modulation, Reverb and no extra fields are accepted. Version-9/10 state `shoop-builtin-fx:1:<reverb-enabled>` migrates by preserving that boolean, selecting Room/Amount 0.2/neutral Tone, disabling all added stages, applying their documented defaults, and creating no assignments. Effect tails, smoothers/LFO phase, and editor visibility are transient. Automatic recorded-take state may store the same control envelope but never assignment lists.

New Carla state uses `shoop-carla-native-state:2:<chain>:<base64>` around the exact NUL-free state returned by Carla Native `get_state`, capped at 16 MiB; `<chain>` is `rack`, `patchbay`, or `patchbay16` and a mismatch is rejected before mutation. Readers retain compatibility with the untagged development-era `shoop-carla-native-state:1:<base64>` representation. They also accept the former LV2 JSON object only when it contains the `http://kxstudio.sf.net/ns/carla/chunk` property with Atom String type and exactly one trailing NUL; they decode that payload before calling Carla Native `set_state`. Malformed, oversized, wrong-type, wrong-chain, missing-NUL, or interior-NUL state is rejected before mutation. OxiSynth state uses `shoop-oxisynth:2:<soundfont-id>:<bank>:<program>:<reverb-send-bits>:<chorus-send-bits>`. The logical SoundFont ID is `timgm6mb`; bank/program must identify an embedded preset, and each send is a canonical lowercase eight-hex-digit finite IEEE-754 value in `0..=1`. Each normalized send contributes `value * 200` SoundFont generator units in addition to preset-authored sends. Malformed fields, unknown versions or SoundFonts, unavailable presets, and invalid sends are rejected before mutation. OxiSynth voices, live controllers, oscillator phase, effect tails, Panic history, and editor visibility are transient.

Native and browser runtimes instantiate Built-in FX and Built-in Synth transactionally before publishing a replacement session. Native External and advertised Carla processors follow the existing native-only path. A runtime whose processor catalog does not contain the required identity rejects the document before backend mutation; browser builds therefore reject External and Carla without interrupting AudioWorklet progress and preserve both built-ins without flattening them.

## Sample-rate conversion

A source-rate mismatch always requires confirmation before mutation.

- Enclosing durations and loop/data lengths use checked rational ceiling.
- Event positions, signed start/capture offsets, recording and processor manual/trim values use checked nearest conversion with ties away from zero.
- MIDI events that collide retain original `order`; converted events are clamped below a non-zero converted duration only when required.
- Audio channels are independently high-quality resampled to their declared converted frame count; compensated audio/MIDI payloads are padded when rounding would otherwise leave the converted logical window incomplete.
- Preplay, ringbuffer sizes, composite delays, and every other sample-domain value use the documented category rule.
- Conversion must not infer duration from media tails or introduce a spurious additional sync cycle.

## Transaction and safety contract

Decode, decompression, hashes, versions, schema/references, capabilities, and optional resampling finish before commit. Backend loading uses begin/chunk/finalize/commit/abort generations. Failure or cancellation leaves the previous session usable.

Saving captures alignment state and all settled channel content from one validated generation. Playing is not a content mutation and must continue. Recording, replacement, loading, clearing, or grab adoption yields an explicit wait/retry/cancel or rejection rather than a mixed-generation save.

Native output uses a temporary sibling, flushes it, and atomically renames it; reads and writes run outside the GUI/application actor after picker selection. Browser upload/download uses asynchronous `rfd` file handles and Blob/download fallback according to browser capability. Picker handles, paths, and browser objects never enter `AppSnapshot` or a session document. Platform failures are reported back as typed task errors.

Archive/codec/resampling/filesystem work never runs in `process()`. Native session compression runs on a worker thread. Browser codec work runs on the UI/control side while the AudioWorklet independently continues bounded render callbacks; session transfer uses 2 KiB generation-tagged chunks and a 256 MiB transfer ceiling.

## Limits, recovery, and compatibility

Default archive limits are 1,000,000 entries and 16 GiB total declared uncompressed payload. Each declared size is checked before allocation; actual practical memory and browser transfer limits may be lower and fail explicitly. The runtime/session mixer additionally allows at most 64 buses, 64 channels per bus, 256 aggregate bus channels/output ports, 4,096 mixer routes, 4,096 bus host links, and 128 UTF-8 bytes per trimmed bus name. AudioWorklet recording storage remains hard-bounded to the documented 120 seconds per channel. The physical Web Audio device boundary remains negotiated separately and the engine deterministically mixes all loop channels to its stereo destination.

Malformed paths, duplicate entries, unknown/undeclared payloads, count/size overflow, CRC/SHA mismatch, unsupported version/capability, and interrupted staged replacement fail without publishing a partial session. Retry by correcting/selecting another file. Cancellation before commit leaves the prior model/backend mapping intact. A save request made during recording/replacement is explicitly rejected until content settles; playing does not block saving and is not transitioned.

A new application session creates one disconnected stereo Master, but Master is removable and a current document may contain zero buses. Current documents restore every bus's name, positive channel shape, stable IDs, controls, explicit mixer routes, exact host links, and visual order. Mono, stereo, and larger buses use deterministic labels; balance must remain centered unless the bus is stereo. Mixer routes reference stable track output-port and bus-channel IDs. The application can instantiate direct, Built-in FX, and Built-in Synth sync/main track topology plus bundled session scripts on native and browser targets. Native builds additionally instantiate External and advertised Carla Rack/Patchbay/Patchbay16x dry/wet tracks, preserve role-bearing media and links across driver switches, and restore current and compatible recorded-take processor state before publication. Script bundle resources and source syntax are checked before commit, activated only after shared session replacement commits, and captured exactly on save. Lua API compatibility is independent of this session format version. Documents persist exact confirmed host IDs, including intentional disconnections, so replacement removes startup defaults before restoring saved links. Editable bus processors, generic MIDI-control configuration, and session-local settings remain codec-representable but cause a capability error if runtime instantiation would be required. Unknown or unavailable track processors likewise fail transactionally rather than flattening to direct topology.

Predecessor `.shl`, `session.1`, tar/JSON/FLAC archives, and JSON `.smf` are not sniffed or migrated. They produce an unsupported-format error and leave the running session unchanged.
