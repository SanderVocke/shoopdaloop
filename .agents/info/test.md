- Complete Rust suite: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`.
- Formatting: `cargo fmt --all -- --check`.
- Warning-denying build: `RUSTFLAGS="-D warnings" cargo build --workspace`.
- Tracing inventory: `python3 scripts/check_tracing_coverage.py --require-closed`.
- Browser checks: build `shoopdaloop_egui` and `shoop_audio_worklet` for `wasm32-unknown-unknown`, then use the smoke commands documented in `src/rust/shoopdaloop_egui/README.md` when browsers are available.

Omit `SHOOP_ALLOW_MISSING_BACKENDS=1` when the test specifically requires real host facilities. Use targeted package/tests while iterating, then run the complete gates before committing behavior changes.
