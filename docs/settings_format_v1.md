# ShoopDaLoop egui settings format, version 1

## Status and identity

This document defines the first application-settings format for the egui application. It is independent from `.shoop` sessions and predecessor settings documents.

The application does not search for, read, import, or rewrite predecessor settings. A document without the `format: "shoop-egui-settings"` marker is rejected as a different format.

## Storage locations

Native builds resolve a configuration directory with:

```text
ProjectDirs::from("org", "ShoopDaLoop", "ShoopDaLoop egui").config_dir()
```

The settings file is `settings.json` below that directory. Typical locations are:

- Linux: `$XDG_CONFIG_HOME/shoopdaloopegui/settings.json`, falling back to `$HOME/.config/shoopdaloopegui/settings.json`.
- macOS: `$HOME/Library/Application Support/org.ShoopDaLoop.ShoopDaLoop-egui/settings.json`.
- Windows: `%APPDATA%\ShoopDaLoop\ShoopDaLoop egui\config\settings.json`.

The resolved path shown by the application is authoritative; environment variables and platform APIs may change the examples.

Browser builds store the same canonical JSON text in origin-scoped `localStorage` under `org.shoopdaloop.egui.settings`. Different schemes, hosts, ports, browser profiles, and private/direct-file policies may produce separate or unavailable stores. Settings are not synchronized across origins.

These identities are exclusive to the egui settings format.

## Version 1 document

The file is canonical UTF-8 JSON with a trailing newline:

```json
{
  "format": "shoop-egui-settings",
  "format_version": {
    "major": 1,
    "minor": 0
  },
  "document_version": 1,
  "writer_version": "0.0.0",
  "values": {
    "carla.hosting_mode": "in_process",
    "scripting.bundled.akai_apc_mini_mk1.enabled": false,
    "scripting.bundled.keyboard.enabled": true,
    "scripting.user_scripts": [
      {
        "value": "/home/user/controller.lua",
        "enabled": true
      }
    ],
    "tracks.new.default_audio_channels": 2,
    "tracks.new.default_midi": false
  }
}
```

Rules:

- `format` must equal `shoop-egui-settings`.
- `format_version.major` and `format_version.minor` are unsigned 16-bit integers. Version 1 readers require major 1 and reject a minor newer than they support.
- `document_version` is an unsigned 16-bit schema version. Version 1 readers dispatch it before decoding a version-specific DTO.
- `writer_version` records the writing application version for diagnostics. It does not control compatibility.
- `values` is an object keyed by stable dotted ASCII setting IDs. Registered values are JSON booleans, unsigned/signed integers, finite numbers, strings, or ordered string/toggle lists according to the definition.
- Object output is deterministic: fixed envelope field order and lexicographically sorted setting IDs. Pretty-printing is two-space indented and ends with one newline.
- Definitions, labels, descriptions, constraints, categories, and defaults live in application code and are not serialized.

## Registered values

The startup-composed registry is authoritative for known settings. Each definition supplies a stable typed key, default, category and ordering, label, description, editor constraints, and effect timing.

On load:

- A missing known key resolves to its registered default.
- A known value with the wrong JSON type or outside its registered constraints resolves to its default and produces a setting-specific warning.
- Unknown keys remain opaque JSON values and are preserved byte-semantically as JSON values across a same-version save. They are not exposed to consumers or the settings dialog.
- Duplicate registrations, invalid defaults, incompatible editor/type combinations, and typed reads using the wrong key type are programming errors rejected while composing/testing the registry.

Version 1 registers the cross-target track defaults and bundled-script toggles below. Native composition additionally registers the ordered user-script path list and audio-driver preferences. Browser composition omits machine-path and native-audio definitions and preserves them as unknown values if encountered.

| Key | Type | Default | Effect |
|---|---|---:|---|
| `tracks.new.default_audio_channels` | `u32` | `2` | Next Add Track dialog opened |
| `tracks.new.default_midi` | boolean | `false` | Next Add Track dialog opened |
| `scripting.bundled.keyboard.enabled` | boolean | `true` | After a successful Save |
| `scripting.bundled.akai_apc_mini_mk1.enabled` | boolean | `false` | After a successful Save |
| `scripting.user_scripts` | ordered string/toggle list | `[]` | After a successful Save |
| `audio.selected_driver` | string | `"dummy"` | Next native startup; changed by a successful confirmed Switch |
| `audio.dummy.sample_rate` | `u32` | `48000` | Confirmed dummy Switch |
| `audio.dummy.buffer_size` | `u32` | `256` | Confirmed dummy Switch |
| `audio.jack.client_name` | string | `"ShoopDaLoop"` | Confirmed JACK Switch |
| `audio.cpal.client_name` | string | `"ShoopDaLoop"` | Confirmed CPAL Switch |
| `audio.cpal.host` | string | `"default"` | Confirmed CPAL Switch |
| `audio.cpal.output_device` | string | `"default"` | Confirmed CPAL Switch |
| `audio.cpal.input_device` | string | `"default"` | Confirmed CPAL Switch |
| `audio.cpal.sample_rate` | `u32` | `0` | Confirmed CPAL Switch; zero requests device default |
| `audio.cpal.buffer_size` | `u32` | `0` | Confirmed CPAL Switch; zero requests device default |
| `audio.cpal.output_channels` | string | `"all"` | Confirmed CPAL Switch |
| `audio.cpal.input_channels` | string | `"all"` | Confirmed CPAL Switch |
| `audio.cpal.capture_ring_frames` | `u32` | `4096` | Confirmed CPAL Switch |
| `audio.cpal.midi_inputs` | string | `"all"` | Confirmed CPAL Switch; comma-separated selectors |
| `audio.cpal.midi_outputs` | string | `"all"` | Confirmed CPAL Switch; comma-separated selectors |
| `carla.hosting_mode` | string choice | `"in_process"` | Next native application start; allowed values are `"in_process"` and `"subprocess"` |

`carla.hosting_mode` is registered only by native composition. It is a global machine preference and never enters `.shoop` session settings. Native startup validates and applies it through the backend adapter before constructing `NativeBackend` or any FX chain. Ordinary **Save** persists a changed value but does not migrate or restart running chains; the Settings dialog marks it restart-required. Missing or invalid values fall back to in-process hosting with a diagnostic. Subprocess mode launches one independently supervised worker per Carla chain using the packaged main executable's hidden worker entry.

These optional keys do not change document version 1. Each driver keeps an independent configured profile. Saving a profile does not change the running backend. A confirmed runtime Switch updates `audio.selected_driver` only after backend commit and persists the complete draft before reporting full success. Native startup attempts the saved driver/configuration first and falls back to dummy/offline with a diagnostic if it is unavailable; fallback does not overwrite the preference.

An ordered string/toggle list is a JSON array. Each entry is exactly an object with a non-empty unique `value` string and an `enabled` boolean. Array order is retained. The generic editor supports editing, toggling, adding, removing, and resetting entries. Invalid or duplicate entries reject a draft; malformed stored values produce a diagnostic and use the registered default.

The track defaults do not change an existing track, an already-open Add Track draft, or session data. Bundled script toggles reconcile running scripts only after a successful durable save; a failed write leaves the active revision and runtime unchanged. Native user-script settings contain machine paths only and never enter `.shoop` session state. Both bundled scripts remain discoverable on native and browser targets; only `keyboard.lua` runs by default.

## Version checks and migration

Readers parse only the envelope first. No values are applied until format and version dispatch succeeds.

- An unknown format is an unsupported-format error.
- An unsupported major, newer minor, older document without a registered migration, or future document version is an unsupported-version error.
- A supported older document is decoded into its version-specific DTO and passed through every registered pure `Vn -> Vn+1` migration in order. Runtime consumers receive only the current resolved model.
- A migration either returns one complete next-version DTO or fails without publishing values or writing storage.
- Adding an optional setting normally does not require a document-version change because missing keys default and unknown keys are retained. Change the document version when the envelope or representation of existing values changes.
- Format and document versions are independent from `.shoop` session versions and predecessor schema names.

There is no pre-v1 egui settings format and therefore no production migration into v1. The ordered dispatcher is tested independently so a future v2 can add a concrete v1-to-v2 step without changing runtime consumers.

## Loading, saving, and recovery

A missing document is a normal first run and publishes registered defaults.

Malformed, unsupported, unreadable, or storage-unavailable input publishes defaults plus an actionable diagnostic. The source is not automatically rewritten. An unsupported future document must never be normalized by an older application. The settings dialog requires an explicit recovery/reset action before replacing rejected source data.

The application has one Settings dialog. Registered categories are tabs. Native builds include a **Scripts** tab with bundled toggles, user-file management, lifecycle, documentation, logs, and MIDI diagnostics. Browser builds include the same bundled toggles and runtime diagnostics but omit the native user-path definition and Add-file action.

Dialog edits are drafts. Cancel or close discards them. Save validates the complete draft, merges known values into the retained current-version document, and preserves unknown keys. Native script files are read and syntax-checked before a script-settings draft is accepted. Running scripts are reconciled only after persistence publishes the committed revision; runtime-only Stop, Restart, and Reload actions do not mutate the draft.

Native save:

1. Serialize the complete canonical document.
2. Create a temporary file in the destination directory.
3. Write and flush the content.
4. Atomically replace `settings.json` and synchronize the containing directory where supported.
5. Publish the new immutable settings snapshot only after success.

Native writing runs outside application and audio actors. A failed write leaves the previous active snapshot and destination bytes unchanged and removes temporary output.

Browser save serializes first, calls `localStorage.setItem` once, and publishes only after it succeeds. Security, availability, and quota exceptions are typed failures and leave active values unchanged.

Settings contain ordinary user preferences only. Audio values are configured selectors, not resolved device handles or negotiated runtime state. Secrets, credentials, session topology/content, transient transport/UI task state, device handles, and backend runtime state do not belong in this document.
