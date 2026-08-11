# Installation

ShoopDaLoop is distributed as unsigned native application archives and browser artifacts. Development builds are also straightforward to run from source.

## Build artifacts

The cross-platform workflow produces:

- Linux x86_64: `.tar.gz` archives containing `shoopdaloop_egui`.
- Windows x86_64: `.zip` archives containing `shoopdaloop_egui.exe`.
- macOS arm64: `.tar.gz` archives containing `ShoopDaLoop egui.app`.
- WebAssembly: a hosted bundle `.zip` and a self-contained `.html` file.

Native archives do not bundle a complete native-library dependency closure and are not installers. They are unsigned; the operating system may require explicit approval before first launch. Release and workflow artifacts should be treated as development software and tested before performance use.

The hosted web bundle should be served over HTTPS or `localhost`. The self-contained HTML can be opened directly, but browser security policy may restrict audio, MIDI, or storage on `file:` URLs.

## Build from source

Install the Rust toolchain selected by `rust-toolchain.toml`, then install the native development libraries required for your target.

### Linux

The Ubuntu CI build uses:

```sh
sudo apt-get update
sudo apt-get install --yes \
  libasound2-dev libjack-jackd2-dev liblilv-dev libgl1-mesa-dev \
  libx11-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxcursor-dev libxi-dev libxkbcommon-dev libxrandr-dev libwayland-dev
```

Equivalent packages may be used on other distributions.

### macOS

Install Xcode command-line tools, Rust, Lilv, and pkg-config. The CI runner uses:

```sh
brew install lilv pkg-config
```

### Windows

Install Visual Studio 2022 Build Tools with the MSVC C++ toolchain, Rust for `x86_64-pc-windows-msvc`, and native Lilv plus pkg-config development files. Ensure `PKG_CONFIG` and `PKG_CONFIG_PATH` resolve the Lilv installation. The GitHub-hosted runner installs those two packages through its provided dependency manager.

### Native commands

From the repository root:

```sh
# Native drivers and LV2/Carla hosting.
cargo build -p shoopdaloop_egui
cargo run -p shoopdaloop_egui

# Build and run without native FX dependencies.
cargo build -p shoopdaloop_egui --no-default-features
cargo run -p shoopdaloop_egui --no-default-features
```

The executable is written to `target/debug/` or `target/release/`. No generated launcher is needed.

On first native launch, ShoopDaLoop uses the dummy/offline driver. Open **Settings → Audio** to configure JACK or CPAL+midir and confirm a runtime switch.

## Browser build

Install the WebAssembly target and the Trunk version used by CI:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
cd src/rust/shoopdaloop_egui
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
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1
```

`SHOOP_ALLOW_MISSING_BACKENDS=1` skips only tests that require unavailable host audio/MIDI facilities; deterministic software-backed tests continue to run.
