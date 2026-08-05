# ShoopDaLoop egui dummy-engine application

This is the shared native and browser composition root for the egui application. Both targets run `shoop_app` and `shoop_backend` against the real `shoop_engine` processing model with its deterministic dummy driver. The dummy driver advances loop processing but does not capture a microphone, produce audible output, or connect physical MIDI devices.

## Native

From the repository root:

```sh
cargo run -p shoopdaloop_egui
```

## WebAssembly

Install the Rust target and [Trunk](https://trunkrs.dev/):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Then serve the application:

```sh
cd src/rust/shoopdaloop_egui
trunk serve --open
```

For a deployable bundle and optional self-contained HTML file:

```sh
trunk build --release
python3 build_single_file_app.py dist
```

The browser runtime cooperatively advances bounded dummy-engine work from animation updates. Large browser scheduling gaps are dropped and reported as xruns rather than processed in one unbounded catch-up burst.

After a release bundle is built, a Chrome/Chromium smoke test can run the browser's scripted add-track, record, stop, play, and details workflow:

```sh
node --experimental-websocket browser_smoke.mjs
```

Set `CHROME_BIN` when the browser executable is not named `google-chrome`.

A compiler-only check can be run from the repository root:

```sh
cargo check -p shoopdaloop_egui --target wasm32-unknown-unknown
```
