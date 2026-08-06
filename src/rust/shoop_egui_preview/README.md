# ShoopDaLoop egui connection preview

Backend-free native/browser preview for the egui presentation. It supplies representative track-port connection snapshots, captures desired-state intents, and offers controls for ready, loading, unavailable, pending/error, and endpoint-churn states.

```sh
cargo run -p shoop_egui_preview
cargo check -p shoop_egui_preview --target wasm32-unknown-unknown
cd src/rust/shoop_egui_preview && trunk build --release
python3 build_single_file_preview.py dist
```

Open the global or per-track **Connections** menu entry to exercise both scopes. The Trunk bundle is written to `dist`; the script writes `dist/preview.html`.
