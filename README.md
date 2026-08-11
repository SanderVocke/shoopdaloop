![Logo](./resources/logo-small.png)

[![Build](https://github.com/SanderVocke/shoopdaloop/actions/workflows/build_and_test_egui.yml/badge.svg)](https://github.com/SanderVocke/shoopdaloop/actions/workflows/build_and_test_egui.yml)
[![Docs](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml/badge.svg)](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml)

# ShoopDaLoop — Limitless Looping

ShoopDaLoop is a playful cross-platform (including web) live-looping application for audio and MIDI. It organizes loops into a track grid and supports free-form jamming, controller-driven workflows, and prepared performances.

The application is feature-complete for its current design. Releases before 1.0 remain development releases: test them thoroughly before relying on them in a performance.

[User and developer documentation](https://sandervocke.github.io/shoopdaloop/) is published from this repository.

## Current features

- Audio and MIDI loops in aligned track columns.
- A sync loop, synchronized or immediate transitions, fixed-cycle recording, selection, targeting, solo behavior, and retroactive grab recording.
- Regular tracks and dry/wet tracks with independent audio/MIDI topology.
- External FX/synth processing, built-in Tiny Synth/FX, and native Carla Rack/Patchbay hosting when native FX support is enabled.
- JACK, CPAL+midir, and dummy/offline drivers on desktop.
- Web Audio/AudioWorklet and permission-gated Web MIDI in supported browsers.
- Connection management for application and host audio/MIDI ports.
- Session load/save plus loop audio/MIDI import and export.
- Generated audio or MIDI click tracks.
- Embedded Lua scripting, keyboard control, and an APC Mini controller script.
- Native and browser settings with explicit save, validation, and recovery behavior.
- Optional Tracy profiling and capture on native builds.

## Screenshot

The repository currently retains an older interface screenshot while updated screenshots are prepared.

![Screenshot](docs/source/resources/screenshot.png)

## Run from source

```sh
# Native application with native FX support.
cargo run -p shoopdaloop_egui

# Native application without LV2/Carla dependencies.
cargo run -p shoopdaloop_egui --no-default-features
```

For browser development:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
cd src/rust/shoopdaloop_egui
trunk serve --open
```

See [INSTALL.md](INSTALL.md) for prerequisites and artifact details. The application-specific [technical README](src/rust/shoopdaloop_egui/README.md) documents settings, drivers, browser permissions, files, CI, and verification workflows.

## Builds and platforms

The main workflow builds Linux x86_64, Windows x86_64, macOS arm64, and WebAssembly in debug and release. Native outputs are unsigned application archives. Web outputs include a hosted bundle and a self-contained HTML file. Browser media and MIDI access depend on browser support, permissions, secure-context policy, and device availability.

## License and credits

Copyright © Sander Vocke (2023–present) and other credited contributors. See [LICENSE](LICENSE).

ShoopDaLoop is made possible by Rust, egui/eframe, JACK, CPAL, midir, libsndfile-compatible tooling, Lilv/LV2, Carla, omniLua, Tracy, and many other open-source projects represented in `Cargo.lock`.
