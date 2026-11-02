# Backend contracts

The contract builds as `no_std` with `alloc`; the `legacy` feature enables `std`
for the retained implementations. Portable consumers must leave `legacy` disabled.

The crate root defines engine-independent backend identities, data types,
capabilities, request/result structures and the Backend trait. It builds with
`--no-default-features` without either engine implementation, concrete transport,
UI or Lua runtime. The RT adapter disables default features on this dependency.
The dependency policy checks this contract and the adapter's full production tree
on Linux, Windows, macOS and Wasm.

Existing concrete implementations, conversion helpers and fake/reference tests
are grouped behind the opt-in `legacy` feature. Default features are empty.
`native-drivers`, `native-fx` and `contracts` enable development/reference evidence;
production UI and session hosts do not enable or depend on these implementations.
See the feature parity ledger for the restoration backlog and preserved behavior.

Checks for the portable boundary:

    cargo build -p shoop_backend -p shoop_rt_backend --no-default-features
    cargo build -p shoop_backend -p shoop_rt_backend --no-default-features --target wasm32-unknown-unknown
    python3 scripts/check_rt_engine_boundary.py

Retained reference behavior: `cargo test -p shoop_backend --features legacy --lib`.
