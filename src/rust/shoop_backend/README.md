# Backend contracts

The crate root defines engine-independent backend identities, data types,
capabilities, request/result structures and the Backend trait. It builds with
`--no-default-features` without either engine implementation, concrete transport,
UI or Lua runtime. The RT adapter disables default features on this dependency.
The dependency policy checks this contract and the adapter's full production tree
on Linux, Windows, macOS and Wasm.

Existing concrete implementations, conversion helpers and fake/reference tests
are grouped behind the `legacy` feature. During migration that feature remains a
default so existing application entry points keep working. `native-drivers` and
`native-fx` require it; `contracts` enables it for existing development fixtures.
This separation is not completion of production legacy retirement: removing the
old entry points/default selection is still Stage 6 of the recovery plan.

Checks for the portable boundary:

    cargo build -p shoop_backend -p shoop_rt_backend --no-default-features
    cargo build -p shoop_backend -p shoop_rt_backend --no-default-features --target wasm32-unknown-unknown
    python3 scripts/check_rt_engine_boundary.py

Retained reference behavior: `cargo test -p shoop_backend --lib`.
