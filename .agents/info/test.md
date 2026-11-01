Run these commands in the environment selected by the build guidance. In particular, enter the repository's development shell first on Nix/NixOS.

- Complete Rust suite: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci` (requires cargo-nextest 0.9.116).
- Formatting: `cargo fmt --all -- --check`.
- Warning-denying build: `RUSTFLAGS="-D warnings" cargo build --workspace`.
- Tracing inventory: `python3 scripts/check_tracing_coverage.py --require-closed`.
- Browser checks: build `shoopdaloop` and `shoop_rt_host_worklet` for `wasm32-unknown-unknown`, then use the smoke commands documented in `src/rust/shoopdaloop/README.md` when browsers are available.

Omit `SHOOP_ALLOW_MISSING_BACKENDS=1` when the test specifically requires real host facilities. Use targeted package/tests while iterating, then run the complete gates before committing behavior changes.

The workspace retains `shoop_app_reference` and independent legacy engine/worklet
fixtures for the feature-parity handoff. The test-only legacy engine/backend features
flag does not select a production backend. See `docs/native_session_recovery/feature-parity.md`
before retiring any UI, implementation, configuration, assets or behavioral tests.
