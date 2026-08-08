# ShoopDaLoop egui application

This is the shared native and browser composition root for the egui application.

- Native builds retain the threaded deterministic dummy backend and provide actor-owned Lua scripting, keyboard control, and script-created native MIDI control ports.
- Browser builds use a repository-owned Web Audio/AudioWorklet backend after an explicit microphone or output-only enable action.
- Browser Lua and MIDI device input/output are intentionally unavailable. MIDI loop content and `.shoop`/`.shoop-midi` file workflows remain cross-target.

## Native

From the repository root:

```sh
cargo run -p shoopdaloop_egui
```

This starts the native dummy audio engine; it does not open a physical audio device. Native MIDI controller discovery uses the host MIDI service. Open **Settings** and select **Scripts** to manage the embedded keyboard/APC scripts or path-based user scripts. This is the only script-management dialog. ``keyboard.lua`` is enabled on first run; bundled toggles and ordered user path/enabled entries are preserved in the fresh egui settings document after **Save**. Runtime-only Stop, Restart, and Reload controls plus lifecycle, documentation, logs, callbacks/timers, MIDI connections, dropped messages, and failures are visible in the same tab.

Bundled Lua sources are compiled into the native binary, so packaged startup does not depend on the source checkout. User-file reads and settings writes stay in this composition root. Source-bearing session scripts are staged before transactional session commit and round-trip in ``.shoop`` files without embedding machine-wide paths.

## Application settings

Choose **Settings** from the main menu to edit application-wide preferences. The initial preferences control the audio channel count and MIDI state used the next time the Add Track dialog opens. They do not alter existing tracks, an Add Track draft that is already open, or `.shoop` session data. The dialog keeps edits in a draft until **Save**; **Cancel** or closing it discards the draft, and reset actions restore registered defaults.

Native builds store fresh egui settings in `settings.json` below the OS configuration directory resolved for the `org` / `ShoopDaLoop` / `ShoopDaLoop egui` application identity. The dialog displays the authoritative resolved path. Browser builds use origin-scoped `localStorage` key `org.shoopdaloop.egui.settings`; direct-file persistence is browser-policy-dependent and must not be assumed to carry across URLs or origins.

Missing settings use stereo/MIDI-off defaults. Invalid known values use their defaults with a warning. Malformed, unreadable, or unsupported-version documents are not overwritten automatically; use the explicit replacement action after reviewing the diagnostic. The egui app never imports the retained QML settings format. See `../../../docs/settings_format_v1.md` for the format, locations, migration boundary, and recovery contract.

## Hosted browser audio

Install the Rust target and [Trunk](https://trunkrs.dev/):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
```

Serve the application from localhost:

```sh
cd src/rust/shoopdaloop_egui
trunk serve --open
```

For reliable browser behavior, use HTTPS or `localhost`, which browsers treat as a secure context. Click **Enable microphone audio** to create one `AudioContext`, request the default microphone, and connect microphone → AudioWorklet → default destination. Click **Enable output-only audio** to skip microphone capture and connect an input-free AudioWorklet directly to the default destination. Permission denial and driver failure leave the application responsive and expose retry actions.

The self-contained HTML embeds the application and AudioWorklet Wasm modules plus the worklet script. It may be opened directly through `file:` and attempts both output-only and microphone modes without rejecting the URL. Browser security and media-permission behavior for local files varies, so HTTPS or `localhost` remains the portable option. Both physical-audio modes still require an explicit click because of browser autoplay policies.

The browser requests echo cancellation, noise suppression, and automatic gain control off, but the browser may negotiate different settings. The engine runs at the context's actual sample rate and render quantum. Mono capture is duplicated where a stereo direct track needs two inputs; a mono track uses capture channel one. Mono track output is sent to both destination channels, stereo maps left/right, and all tracks sum with final clipping to `[-1, 1]`. Input monitoring defaults off to reduce feedback risk.

Browser recording storage is prepared per channel for ten seconds at the actual sample rate. Exhaustion stops further channel recording work and is reported in diagnostics instead of growing Wasm memory in the render callback.

## Session and loop files

The main menu saves and loads fresh `.shoop` v1 sessions. Loop context menus import/export exact `.shoop-audio` and `.shoop-midi`, float WAV, and standard MIDI. Audio import requires explicit destination mapping; audio export presents an ordered channel selection. Different-rate assets require confirmation before deterministic audio/MIDI/timing conversion. QML-era session/media formats are deliberately unsupported.

Native picker reads and atomic temporary-file replacement run outside the application actor. Browser pickers use asynchronous upload/download file handles; ordinary hosted and direct-file artifacts do not require the File System Access API. Session/media bytes stay outside immutable GUI snapshots. See `../../../docs/session_format_v1.md` for formats, limits, timing, and recovery behavior.

## Builds and artifacts

Trunk builds the UI and dedicated worklet with matching profiles:

```sh
cd src/rust/shoopdaloop_egui
trunk build                 # debug UI and worklet
trunk build --release       # release UI and worklet
python3 build_single_file_app.py dist
```

CI application archives can also be produced locally from already-built outputs:

```sh
# From the repository root after a native debug build.
python3 src/rust/shoopdaloop_egui/package_artifacts.py native \
  --platform linux --arch x86_64 --profile debug \
  --binary target/debug/shoopdaloop_egui --output-dir artifacts

# From src/rust/shoopdaloop_egui after a Trunk debug build.
python3 package_artifacts.py web \
  --profile debug --dist dist --output-dir ../../../artifacts
```

Native CI outputs are unsigned application archives rather than installers or portable dependency-closure packages. The hosted web archive supports physical browser audio and contains the complete UI and AudioWorklet assets. The separately generated profile-named HTML embeds those assets and attempts physical output or microphone audio when directly opened from `file:`. Open it with `?offline=1` to explicitly select the elapsed-time dummy engine instead.

Generated `dist`, worklet, staging, and artifact files are not committed.

## Cross-target CI

`.github/workflows/build_and_test_egui.yml` has one eight-cell matrix: Linux x86_64, Windows x86_64, macOS arm64, and WebAssembly, each in debug and release. Every cell builds, packages, uploads, and then tests. Native cells upload unsigned application archives; web cells upload a hosted bundle archive and a separately downloadable self-contained HTML file. The matrix has no coverage flavor yet.

For fast workflow iteration with `nektos/act` 0.2.89 or newer, run the Linux and web debug cells on a suitable self-hosted development environment:

```sh
act pull_request -W .github/workflows/build_and_test_egui.yml \
  -j build_and_test --matrix target:linux --matrix profile:debug \
  -P ubuntu-24.04=-self-hosted --artifact-server-path .act/artifacts

act pull_request -W .github/workflows/build_and_test_egui.yml \
  -j build_and_test --matrix target:web --matrix profile:debug \
  -P ubuntu-24.04=-self-hosted --artifact-server-path .act/artifacts
```

The web command needs Trunk 0.21.14 and `wasm32-unknown-unknown`; the local workflow skips GitHub cache/upload actions and browser/device automation while still building, packaging, validating, testing, and checking dependency isolation. GitHub-hosted runners remain authoritative for uploads, caches, Chrome/Firefox, Windows, and macOS.

## Browser verification

After building, run Chrome/Chromium with a deterministic generated fake microphone:

```sh
node --experimental-websocket browser_smoke.mjs
BROWSER_SIZE=360,200 node --experimental-websocket browser_smoke.mjs
DENY_FIRST=1 node --experimental-websocket browser_smoke.mjs
LIFECYCLE=1 node --experimental-websocket browser_smoke.mjs
SATURATE=1 node --experimental-websocket browser_smoke.mjs
STRESS=1 node --experimental-websocket browser_smoke.mjs
OUTPUT_ONLY=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 OUTPUT_ONLY=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 DIRECT_FILE_MIC=1 node --experimental-websocket browser_smoke.mjs
SETTINGS_ONLY=1 node --experimental-websocket browser_smoke.mjs
SETTINGS_ONLY=1 SETTINGS_UNAVAILABLE=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 SETTINGS_ONLY=1 node --experimental-websocket browser_smoke.mjs
xvfb-run -a python3 browser_firefox_smoke.py
STRESS=1 xvfb-run -a python3 browser_firefox_smoke.py
```

The Firefox command also requires Selenium and geckodriver. Set `CHROME_BIN` or `FIREFOX_BIN` when browser executables use non-standard names. The hosted tests click the enable action, create mono and stereo tracks, monitor and record non-zero fake capture, verify non-zero waveform and playback output, save real produced `.shoop` bytes while callbacks advance, and transactionally load those exact bytes. Stress mode fills the bounded recording store and verifies callback continuity throughout the larger transfer/encode/load. Additional Chrome modes cover denial/retry, suspend/resume, forced worklet loss/retry, cleanup, queue saturation, and explicit offline dummy operation. Settings modes cover real save/reload and Add Track consumption, unavailable storage, failed writes, invalid known values, future-version rejection without overwrite, and hosted/direct-file behavior. The self-contained offline self-test performs the same real-byte session round trip without relying on a fixture-only success flag.

Compiler-only checks from the repository root:

```sh
cargo check -p shoopdaloop_egui --target wasm32-unknown-unknown
cargo build -p shoop_audio_worklet --target wasm32-unknown-unknown --release
```
