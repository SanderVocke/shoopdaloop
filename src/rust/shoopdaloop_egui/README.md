# ShoopDaLoop egui application

This is the shared native and browser composition root for the egui application.

- Native builds retain the threaded deterministic dummy backend.
- Hosted browser builds use a repository-owned Web Audio/AudioWorklet backend after an explicit **Enable microphone audio** action.
- Browser MIDI is not implemented. Audio tracks work, but MIDI tracks receive no browser device data.

## Native

From the repository root:

```sh
cargo run -p shoopdaloop_egui
```

This starts the native dummy engine; it does not open a physical audio device.

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

A hosted deployment must use HTTPS. `localhost` is also treated as a secure context by browsers. Click **Enable microphone audio** to create one `AudioContext`, request the default microphone, and connect microphone → AudioWorklet → default destination. Permission denial and driver failure leave the application responsive and expose a retry action.

The browser requests echo cancellation, noise suppression, and automatic gain control off, but the browser may negotiate different settings. The engine runs at the context's actual sample rate and render quantum. Mono capture is duplicated where a stereo direct track needs two inputs; a mono track uses capture channel one. Mono track output is sent to both destination channels, stereo maps left/right, and all tracks sum with final clipping to `[-1, 1]`. Input monitoring defaults off to reduce feedback risk.

Browser recording storage is prepared per channel for ten seconds at the actual sample rate. Exhaustion stops further channel recording work and is reported in diagnostics instead of growing Wasm memory in the render callback.

## Builds and artifacts

A release build reproducibly builds both the UI module and dedicated worklet module:

```sh
cd src/rust/shoopdaloop_egui
trunk build --release
python3 build_single_file_app.py dist
```

The hosted `dist/index.html` bundle supports physical browser audio. The self-contained `dist/shoopdaloop_egui.html` cannot claim microphone support when directly opened from `file:`. Open it with `?offline=1` to explicitly select the elapsed-time dummy engine; without that query it presents the secure-context limitation and does not silently substitute dummy processing.

Generated `dist` and worklet files are not committed.

## Browser verification

After building, run Chrome/Chromium with a deterministic generated fake microphone:

```sh
node --experimental-websocket browser_smoke.mjs
BROWSER_SIZE=360,200 node --experimental-websocket browser_smoke.mjs
DENY_FIRST=1 node --experimental-websocket browser_smoke.mjs
LIFECYCLE=1 node --experimental-websocket browser_smoke.mjs
SATURATE=1 node --experimental-websocket browser_smoke.mjs
STRESS=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 SECURE_LIMIT=1 node --experimental-websocket browser_smoke.mjs
xvfb-run -a python3 browser_firefox_smoke.py
```

The Firefox command also requires Selenium and geckodriver. Set `CHROME_BIN` or `FIREFOX_BIN` when browser executables use non-standard names. The hosted tests click the enable action, create mono and stereo tracks, monitor and record non-zero fake capture, verify non-zero waveform and playback output, and check callback progress. Additional Chrome modes cover denial/retry, suspend/resume, forced worklet loss/retry, cleanup, stress recording, and explicit offline dummy operation.

Compiler-only checks from the repository root:

```sh
cargo check -p shoopdaloop_egui --target wasm32-unknown-unknown
cargo build -p shoop_audio_worklet --target wasm32-unknown-unknown --release
```
