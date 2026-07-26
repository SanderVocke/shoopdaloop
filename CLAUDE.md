# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`AGENTS.md` + `.agents/*.md` hold the same build/test basics for other agent
tools. Read `.agents/troubleshooting.md` when the app or tests misbehave at
runtime in ways unrelated to your change.

## Toolchain

**A nightly toolchain is mandatory.** `.cargo/config.toml` sets
`[unstable] bindeps = true` because `backend` depends on `refilling_pool` and
`backend_rust` as `artifact = "staticlib"`. On stable, even `cargo metadata`
fails with ``artifact = … requires `-Z bindeps` ``. Use `cargo +nightly …` unless
nightly is the default toolchain.

`cargo fmt --all --check` is CI-enforced (also nightly rustfmt). No clippy in CI.

## Build

Cargo drives everything, C++ included: `cargo +nightly build`. The `backend`
crate's `build.rs` invokes CMake on `src/backend` (Ninja generator) and installs
into its `OUT_DIR`.

C++ dependencies are *found*, not fetched. `python scripts/vcpkg_prebuild.py`
builds them (including Qt) from `vcpkg/vcpkg.json` into `build/vcpkg_installed`
and emits `build/build-env-[debug|release].[sh|ps1|elv]` to source before
building. Otherwise supply them yourself (`QMAKE` if autodetection fails).

Before finishing a task, run `cargo fmt --all` then build with
`RUSTFLAGS="-D warnings"`. A failure inside an *external* dependency means the
dev environment is not set up — report and stop rather than chasing it.

`--features coverage` (CI-only) threads coverage instrumentation through to the
CMake build. The `prebuild` feature, which every crate declares and propagates,
short-circuits all `build.rs` C++/CMake work and `#[cfg]`s out most of
`frontend`; nothing in-repo currently enables it.

## Test

Three levels, all needed:

- **C++ (Catch2)** — `test_runner`, a side effect of building `backend`, nested
  under `target/`. Single test: `test_runner "[AudioMidiLoop][audio]"`.
  Sources in `src/backend/test/{unit,integration}`.
- **Rust** — `cargo +nightly test`. Single: `cargo +nightly test -p frontend <name>`.
  CI uses `cargo-nextest`.
- **QML (Qt Quick Test)** — application-level, closest to system tests. Run via
  the app itself: `shoopdaloop --self-test`. Files default to
  `<qml_dir>/test/**/tst*.qml`. Narrow with `-f <glob>` (files), `--filter <regex>`
  (test cases), `-l` to list without running, `--junit-xml <path>` for a report.

`docs/source/developers.software.rst` is stale in two ways that mislead: it says
the C++ tests use `boost_ext::ut` (they are Catch2, per `vcpkg/vcpkg.json` and
`src/backend/test/CMakeLists.txt`), and it describes the build as
`pyproject.toml` + `py-build-cmake` + PySide (it is Cargo + CMake, no Python
packaging). The README credits `boost::ut` too.

## Running the app

`src/rust/shoopdaloop/build.rs` generates a dev launcher next to the binary
(`target/<profile>/shoopdaloop_dev.sh` on Linux, `.bat` on Windows) that sets the
dynamic-lib search path and points `SHOOP_CONFIG` at the dev config. That config
resolves `qml_dir`/`lua_dir`/`schemas_dir` into the *source tree*, so QML and Lua
edits take effect without rebuilding.

**No launcher is generated on macOS** — `build.rs` only handles
`target_os = "windows"` and `target_os = "linux"`, falling through to an empty
path. `INSTALL.md` claims macOS is covered; it is not. On macOS, set the
environment and `SHOOP_CONFIG` by hand and run `target/<profile>/shoopdaloop`.

Useful flags: `-b dummy` (dummy audio driver, no JACK), `-e` (developer UI),
`-d PORT` (QML debug server), `--quit-after N`, `--monkey-tester`,
`--test-grab-screens DIR`, `--no-crash-handling`.

`QT_QPA_PLATFORM=offscreen` when there is no usable display.

Logging: `SHOOP_LOG=debug` globally, or per unit —
`SHOOP_LOG=Frontend.Loop=trace,Main=debug`. Unit names come from
`shoop_log_unit!` (see below); unknown names are warned about and ignored.

## Architecture

Layers: a C++ real-time backend behind a C API, a Rust middle layer exposing
QObjects to QML via cxx-qt, a QML UI, and user-facing Lua scripts.

The backend (`src/backend`, C++) owns all real-time audio/MIDI processing, the
port/channel/FX graph, the JACK and LV2 drivers, and basic loop transitions. Its
public surface is `libshoopdaloop_backend.h` (C). Session-level composition,
persistence, multi-cycle transition scheduling, and composite loops live in Rust
and QML instead — the split is pragmatic, not principled.

### Crate map (`src/rust/*`, workspace members)

Dependencies flow one way: `shoopdaloop` → `frontend` → `{backend_bindings,
cxx_qt_lib_shoop, config, crashhandling, midi_processing}` → `common`.

- `shoopdaloop` — binary: CLI (`cli_args.rs`), config resolution, QApplication
  and QML engine setup, self-test wiring (`lib_impl.rs`).
- `frontend` — the bulk of the port. All QML-facing QObjects, Lua engine, SMF
  I/O, MIDI I/O, waveform/sequence rendering, the engine update thread.
- `backend_bindings` — hand-written safe Rust wrappers over the C API; `bindgen`
  output is checked in under `src/codegen`.
- `backend` — builds the C++ backend via CMake; exposes link dirs to downstream
  `build.rs` files. Re-exports nothing of substance.
- `backend_rust` — Rust code compiled *into* the C++ backend via `cxx` (MIDI
  state tracking / diffing). This is where the C++ → Rust port lands.
- `refilling_pool` — lock-free buffer pool, also consumed from C++ via `cxx`.
- `cxx_qt_lib_shoop` — Qt bindings cxx-qt-lib lacks: `QQuickItem`, `QThread`,
  `QTimer`, `QSharedPointer`/`QWeakPointer`, `connect`, metatype and QVariant
  helpers, QML singleton registration.
- `config` — resolves install-vs-dev layout (`shoop-config.toml`, see
  `distribution/<os>/`) into concrete `qml_dir`/`lua_dir`/`schemas_dir`/etc.
- `common` — logging framework, env/path utilities.
- `macros` — `deny_calls_not_on_object_thread` / `deny_calls_on_object_thread`.
- `midi_processing`, `crashhandling` (minidumper + crash-handler),
  `qt_header_bindings`, `packaging` (the `package` binary that builds
  redistributables).

### The Gui/Backend QObject pair

The central pattern in `frontend`. Nearly every engine concept exists as two
QObjects: `qobj_<thing>_gui.rs` lives on the GUI thread and is what QML
instantiates; `qobj_<thing>_backend.rs` is created by its GUI counterpart's
`initialize_impl` and immediately `qobject_move_to_thread`'d onto one shared
update thread.

That thread is the process-wide singleton in `engine_update_thread.rs`
(`UpdateThread`, `qobj_update_thread.rs`), which emits `update()`. It is driven by
the QQuickWindow's `frameSwapped()` (wired in `qobj_qmlengine.rs`; disable with
`--dont-refresh-with-gui`), with a 25ms `QTimer` as backup against stalls
(`--max-backend-refresh-interval-ms`). Backend objects connect to it with
`DIRECT_CONNECTION`; GUI↔backend traffic uses `QUEUED_CONNECTION` in both
directions. So GUI-thread code never touches backend state synchronously — it
emits a `backend_*` signal, and properties come back asynchronously.

Thread affinity is enforced at runtime, not by the type system: annotate methods
with `#[macros::deny_calls_not_on_object_thread]` (or its inverse) and they panic
when called from the wrong thread.

### cxx-qt file convention

Each QObject is a pair of files: `qobj_x_bridge.rs` holds only the
`#[cxx_qt::bridge]` module (properties, signals, `#[qinvokable]`s, C++ includes)
and the `Rust` struct; `qobj_x.rs` holds the `impl`. Registration is centralised
in `frontend/src/init.rs` under the QML module `ShoopDaLoop.Rust`, exposed as
`ShoopRust*` names. Types must be registered *before* QML singletons.

Log units are declared per file with `shoop_log_unit!("Frontend.Loop")`, which
also `#[ctor]`-registers the name so `SHOOP_LOG` can target it. Then use
`common::logging::macros::{debug, info, …}`. Files with a per-instance identity
shadow those macros locally to prefix `instance_identifier` — hence the
`raw_debug`/`raw_error` import aliases.

### QML and Lua

`src/qml` is loaded from disk, not compiled in.
`applications/shoopdaloop_main.qml` is the default main window; `-m` selects
others. Thin QML wrappers (`Loop.qml` → `ShoopRustLoopGui`) add QML-side
properties and logging to Rust types. `TestRunner.qml` + `test/Shoop*TestCase.qml`
back the self-tests.

Sessions are JSON validated against `src/session_schemas/schemas/*.json`
(versioned filenames, e.g. `session.1.json`) via the `ShoopRustSchemaValidator`
singleton.

`src/lua` is the user-extensible layer: `lib/shoop_control.lua` is the control
API, `builtins/` holds MIDI controller profiles and keyboard bindings,
`system/sandbox.lua` confines scripts.

## Conventions

- All third-party versions go in the root `[workspace.dependencies]`; member
  crates inherit with `dep = { workspace = true }`.
- `cxx-qt` is patched to a fork (`[patch.crates-io]`) for Windows debug Qt
  library naming. Commented-out local `path` overrides sit alongside it for
  working against a local cxx-qt checkout.
- Knowledge about code belongs in comments next to that code, kept terse and
  describing current state only.
