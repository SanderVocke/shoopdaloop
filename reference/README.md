# Preserved migration evidence

These sources document behavior to restore during feature parity. They are excluded
from production packaging and are not an alternative product launch path. The
[feature parity ledger](../docs/native_session_recovery/feature-parity.md) identifies
the corresponding backlog, executable reference suites and completion criteria.

`legacy_browser/` preserves old JS/Rust glue and integrated acceptance scenarios.
Its raw Wasm host and trace transport still have executable contract tests; test
asset staging builds their old worklet separately from production assets.
`legacy_frontend_acceptance.rs` holds frozen UI scenarios that depend on currently
unsupported features. `legacy_application_readme.md` preserves detailed user-facing
behavior. The former full browser harnesses are kept beside them. These frozen
scenarios need porting through the production service/client when their features
return; they are not counted as passing production coverage.
