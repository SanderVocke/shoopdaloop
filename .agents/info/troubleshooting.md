# Troubleshooting

Read this only when application or test failures appear unrelated to the task.

## Missing host audio or MIDI

Headless environments often lack `/dev/snd`, an ALSA sequencer, JACK, or a default playback device. The complete deterministic suite can explicitly skip tests that require unavailable host facilities:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo nextest run --workspace --features shoop_engine/app_backend --profile ci
```

Do not use that variable when investigating a real JACK, CPAL, midir, or hardware path. Record which host facility is unavailable instead of claiming its tests ran.

## Native startup

The application starts with persisted audio settings and falls back to dummy/offline with a diagnostic when a saved driver is unavailable. Run it directly with `cargo run -p shoopdaloop`; no generated launcher or source-resource environment is required.

## Browser startup

Physical browser audio and Web MIDI require explicit user actions. Prefer HTTPS or localhost. Permission denial and unsupported APIs should leave the UI responsive and expose retry or unavailable state. Direct `file:` behavior varies by browser; use `?offline=1` when testing the explicit offline mode.
