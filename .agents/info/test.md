Run these commands in the environment selected by the build guidance. In particular, enter the repository's development shell first on Nix/NixOS.

- Complete Rust suite: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci` (requires cargo-nextest 0.9.116).
- Formatting: `cargo fmt --all -- --check`.
- Warning-denying build: `RUSTFLAGS="-D warnings" cargo build --workspace`.
- Tracing inventory: `python3 scripts/check_tracing_coverage.py --require-closed`.
- Browser checks: build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`, then use the smoke commands documented in `src/rust/shoopdaloop/README.md` when browsers are available.

Omit `SHOOP_ALLOW_MISSING_BACKENDS=1` when the test specifically requires real host facilities. Use targeted package/tests while iterating, then run the complete gates before committing behavior changes.
