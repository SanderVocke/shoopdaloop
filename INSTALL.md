# Installation

ShoopDaLoop is distributed as unsigned native application archives and browser artifacts. Development builds are also straightforward to run from source.

## Build artifacts

The cross-platform workflow produces:

- Linux x86_64: `.tar.gz` archives containing `shoopdaloop`.
- Windows x86_64: `.zip` archives containing `shoopdaloop.exe`.
- macOS arm64: `.tar.gz` archives containing `ShoopDaLoop.app`.
- WebAssembly: a hosted bundle `.zip` and a self-contained `.html` file.

Native archives bundle the pinned Carla Native runtime used by hosted Rack/Patchbay tracks, including its UI and plugin discovery/bridge helpers. They are not installers and remain unsigned; the operating system may require explicit approval before first launch. Release and workflow artifacts should be treated as development software and tested before performance use.

The hosted web bundle should be served over HTTPS or `localhost`. The self-contained HTML can be opened directly, but browser security policy may restrict audio, MIDI, or storage on `file:` URLs.

## Build from source

Install the Rust toolchain selected by `rust-toolchain.toml`, then install the native development libraries required for your target.

### Linux

The Ubuntu CI build uses:

```sh
sudo apt-get update
sudo apt-get install --yes \
  libasound2-dev libjack-jackd2-dev libgl1-mesa-dev libx11-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxcursor-dev libxi-dev libxkbcommon-dev libxrandr-dev libwayland-dev
```

Equivalent packages may be used on other distributions.

### macOS

Install Xcode command-line tools and Rust. No Carla, Lilv, or LV2 SDK is needed to compile ShoopDaLoop.

### Windows

Install Visual Studio 2022 Build Tools with the MSVC C++ toolchain and Rust for `x86_64-pc-windows-msvc`. No Carla, Lilv, or LV2 SDK is needed to compile ShoopDaLoop.

### Native commands

```sh
cargo build -p shoopdaloop -p shoop_host
cargo run -p shoopdaloop
# Independent recoverable session host and UI siblings:
target/debug/shoopdaloop --session-transport socket
# Dummy driver, without CPAL/Rodio:
cargo build -p shoopdaloop -p shoop_host --no-default-features
```

Both executables must be present beside each other for external-host modes. Native
packages include them, the built-ins and fonts. CPAL+midir is enabled by default;
disabling default features selects the dummy host. Choose audio settings for the
next host launch; live switching is temporarily unavailable. Existing JACK profiles
are mapped to CPAL with diagnostics. See the [application README](src/rust/shoopdaloop/README.md)
for independent host/UI roles, registry selection, close choices and browser behavior.

The [feature parity ledger](docs/native_session_recovery/feature-parity.md) records
all temporary gaps and preserved restoration evidence. Carla/FX, session content
persistence and click-preview capabilities are not currently available in production.
Carla runtime sources and build recipes remain available for subsequent parity work.

## Browser build

Install the WebAssembly target and the Trunk version used by CI:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
cd src/rust/shoopdaloop
trunk serve --open
```

Use the application's explicit audio and Web MIDI enable actions. Microphone, output-only audio, and Web MIDI permissions are independent.

A release bundle can be built with:

```sh
trunk build --release
python3 build_single_file_app.py dist
```

## Verify a source checkout

```sh
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
# Requires cargo-nextest 0.9.116.
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo nextest run --workspace --features shoop_engine/app_backend --profile ci
```

`SHOOP_ALLOW_MISSING_BACKENDS=1` skips only tests that require unavailable host audio/MIDI facilities; deterministic software-backed tests continue to run.
