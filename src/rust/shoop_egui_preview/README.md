# ShoopDaLoop egui preview

This backend-free preview runs the milestone egui workspace either as a native application or in a browser. It uses representative in-memory state and records/applies UI intents; it does not provide audio processing or persistence.

## Native

From the repository root:

```sh
cargo run -p shoop_egui_preview
```

## WebAssembly

Install the Rust target and [Trunk](https://trunkrs.dev/):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Then serve the preview:

```sh
cd src/rust/shoop_egui_preview
trunk serve --open
```

For a deployable bundle instead:

```sh
trunk build --release
```

The static output is written to `src/rust/shoop_egui_preview/dist/`. The `WebAssembly preview` GitHub Actions workflow verifies this release bundle and uploads it as the `shoop-egui-wasm-preview` artifact.

A compiler-only verification can be run from the repository root without Trunk:

```sh
cargo check -p shoop_egui_preview --target wasm32-unknown-unknown
```
