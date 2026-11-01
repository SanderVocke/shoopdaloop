# Independent native session host

This crate composes the authoritative app service, one RT backend adapter and the
native RT engine/device host. It contains protected per-user discovery, authenticated
loopback app data/management connections, socket/shared-memory delivery and sibling
launch adapters. No UI or Lua runtime is linked into the host.

See [roles and CLI examples](../shoopdaloop/README.md). Socket clients can replace a
departed controlling lease without recreating the engine. Shared memory supports one
attachment only. Host shutdown requires current authority and reports MIDI reset,
render stop and resource cleanup; UI/launcher exit does not imply host shutdown.

`cargo test -p shoop_host --features testing` runs private-path and real subprocess
recovery/launch tests. The testing feature's deterministic dummy input is confined
to fixtures and is disabled in ordinary production builds.
