# P16 acceptance evidence

Implementation milestone: `d6bd336e`, PR [#977](https://github.com/SanderVocke/shoopdaloop/pull/977), targeting `feat/backend-replacement`. Final qualification is in progress; the parity row remains open until the remaining gates are resolved.

## Ownership and observable behavior

The service/RT schemas are unchanged. `SessionScripts` and the codec worker's script parts are client-only values. `Work::Prepare` separates resources before producing the musical image. Only that image is staged at the host. Resource provider namespaces include a client-manager identity, so independent clients cannot unregister each other's resources.

| Criterion | Implementation and evidence |
| --- | --- |
| A1 | `shoop_persistence_codec::scripts::tests` independently compares source, Markdown and PNG bytes, checks script-free musical images and streams resources separately from them. Dependency audits pass for native, Windows, macOS and Wasm production feature sets. |
| A2 | Production `persistence_io::tests::scripts` ports the exact export and filesystem conversion scenarios. A source directory is deleted before conversion back, explicit start, provider reads and resave. Admission failures preserve ownership. |
| A3 | Production save tests mutate/remove client records after requesting a save and inspect the frozen output. Project-epoch checks refuse stale captures. Another client's replacement quarantines retained scripts instead of pairing them with new music. |
| A4 | Archive and compile-only Lua validation run in codec workers. Invalid syntax and declined rate conversion preserve music and records. Prepared records are built incrementally before staging, with reserved runtime identities. |
| A5 | Production load tests preserve User/Ephemeral record identities, retain enabled preferences without starting scripts, and clear only Session records on successful empty load/New Session. Explicit starts alone cause observable script control changes. |
| A6 | A future script API source survives accepted musical resampling, exact export and resave. It stays inactive when explicit execution is refused. Catalog reconciliation cannot restart or reclaim converted Session records. |
| A7 | Public runtime/provider tests cover accepted-load reconnect, ambiguous delivery, evicted results and replacement-client save. Candidates remain inactive on uncertainty; accepted operations are never replayed. The packaged socket probe drops file-transfer clients and observes host-owned accepted load completion exactly once from a fresh connection. |
| A8 | Resource/metadata/count/chunk limits, alloc-only resource tests, native/Wasm builds and cross-target dependency audits pass. Native driver tests preserve the running Lua VM and application IDs. Browser and complete native qualification are tracked below. |

## Commands and results

Environment: Linux x86_64, Rust 1.100.0, nextest 0.9.116. Activate the installed toolchain/native libraries with `. /workspace/.setup/activate.sh`. Builds use `RUSTFLAGS='-D warnings'`.

- `cargo test -p shoop_persistence_codec -p shoop_script_resources -p shoop_scripting -p shoop_session`: initial component suites passed (24/15/39/46 tests respectively; later additions receive separate checks).
- `cargo test -p shoop_script_resources --no-default-features --lib`: five tests passed.
- Production persistence suite: 32 tests passed, including the added recovery, incompatible source and project mismatch cases.
- `shoop_scripting::p16_tests`: three tests passed (incremental UTF-8 preparation, quota refusal, independent provider namespaces).
- Isolated native driver workflow retry: both tests passed, including a running VM surviving the switch.
- `cargo build -p shoopdaloop -p shoop_host`: passed after the Cargo OUT_DIR compatibility fix.
- `cargo check -p shoopdaloop -p shoop_persistence_codec --target wasm32-unknown-unknown --no-default-features`: passed.
- `python3 scripts/check_dependency_boundaries.py --package shoop_persistence_codec --package shoop_app_client --package shoopdaloop`: all four target audits passed.
- Formatting, Rust test-attribute policy, whitespace and parity-ledger validation passed.
- Packaged socket persistence probe: passed with >32 MiB audio, exact MIDI, native file I/O, corrupt-file refusal, script source/Markdown/PNG preservation after deleting originals, stopped load, explicit resource access, exact resave and host-owned completion after client loss.

## Qualification still in progress

The first complete native run executed 3218 tests: 3190 passed, 28 failed, 11 skipped. A concurrent production build replaced the test-enabled host companion and caused several recovery failures. The focused rerun uses a consistent companion. Some existing host fixtures also reach their 10–15 second startup deadlines on this environment; the retry and final suite results must be recorded before claiming a complete native gate.

The first actual Chromium run used the unoptimized 80 MiB worklet and reached the existing 20 second audio-startup deadline before any P16 steps. A release browser run is required; a Wasm compile alone is not browser acceptance.

Initial documentation-only CI (`6ed9ee10`) failed native builds on all platforms because Rust 1.100 changed Cargo's OUT_DIR layout. The implementation fixes that failure. Its web debug Node run independently failed `rt_audio_math` before executing tests; the web release job passed. Final-head CI and review results remain to be recorded.

## Deliberate limits

No VM migration, automatic start, server script catalog, attachment transfer of script data, durable client crash journal, general Lua module discovery or directory-bundle export is included. A process that disappears loses its unsaved client additions. Reconnect retains local context only for the same verified host instance. Browser device evidence uses fake microphone/MIDI endpoints; it cannot qualify an arbitrary physical controller or grant permissions for another user.
