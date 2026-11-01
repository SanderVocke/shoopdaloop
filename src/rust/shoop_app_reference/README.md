# Application behavior reference

This unpublished development crate preserves the previous application model,
handlers and behavioral tests. It is used by reference tests, including the retained
Wasm remote-application fixtures. Production frontend/host dependency audits forbid
it. It is not an alternative backend or a recovered-state owner.

`cargo test -p shoop_app_reference` runs the native evidence; the Node Wasm suite
also discovers it. Restore features through the new service/client architecture,
using these contracts and the [parity ledger](../../../docs/native_session_recovery/feature-parity.md).
A reference test passing is not proof of production feature support.
