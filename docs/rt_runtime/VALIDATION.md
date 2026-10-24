# Runtime implementation evidence

## Stage 0: planning baseline

- Planning commit: `76d1f3d9` (parent proposal `704fbb72`). Implementation branch: `feat/generic-rt-runtime`.
- Existing untracked `.pi/` is harness state and is not staged or modified.
- Read repository index, mandates, coding style, build/test guidance, and CI-debug guidance.
- `docs/rt_runtime/CONTRACTS.md` defines the API/ownership/timing table, A1–A8 test mapping, tiny-capacity cases, and benchmark workloads.
- Workspace membership is already `src/rust/*`. The test policy requires the shared dev-only `shoop_test` attribute. Wasm package discovery uses `[package.metadata.shoop-wasm-test]`, not package-name prefixes.
- Native CI already covers Linux x86_64, Windows x86_64, and macOS arm64 in debug/release. Wasm package discovery is automatic, but dependency auditing and browser-change filtering require explicit additions during stage 7.
- The existing Wasm asset server sends COOP/COEP headers. The independent AudioWorklet fixture needs an explicitly non-isolated origin; ordinary browser unit tests do not prove this.

### Baseline results

Commands executed through `nix develop --command` on NixOS x86_64, before Rust implementation changes:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass. |
| `python3 scripts/check_shoop_test_usage.py` | Pass. |
| `RUSTFLAGS="-D warnings" cargo build --workspace` | Pass. |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci` | 1800 passed, 4 skipped (existing suite). |

The current flake supplies Rust 1.97.1, nextest 0.9.143, Node 22.23.2, and wasm-pack 0.15.0. The baseline nextest result above uses 0.9.143, not the repository-documented 0.9.116; obtain the prescribed version for final verification. Both native and `wasm32-unknown-unknown` targets are installed. The browser runner pins Node 22.22.2 and Chrome/ChromeDriver 147; matching Node/Chromium/driver packages exist in the local Nix store but are not on the development shell PATH. Use those tools explicitly inside the selected shell for subsequent Wasm tests; do not weaken the runner's checks. No baseline browser execution is claimed here.

Raw baseline command logs were retained locally as `/tmp/rt-runtime-baseline-build.log` and `/tmp/rt-runtime-baseline-tests.log`. Future milestone entries must record committed commands/results rather than relying on these temporary paths as permanent evidence.
