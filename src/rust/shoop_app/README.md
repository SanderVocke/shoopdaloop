# Authoritative session service

`Service` owns musical state, app/engine identity mappings, connection intent and
admitted work for one backend lifetime. It receives the portable app protocol and
publishes bounded projections/results independently of UI delivery. Reattachment
replaces a controlling lease, not the adapter or engine. Platform transports, Lua,
files and widgets are outside this crate.

`shoop_app_reference` retains the old application model and executable behavioral
evidence for the [feature parity ledger](../../../docs/native_session_recovery/feature-parity.md).
It is not a production dependency or recovery mechanism.

Run contracts with `cargo test -p shoop_app --test service`; these cover typed and
encoded delivery, real RT outcomes, recording/media continuity and stale owners.
