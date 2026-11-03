# Independent native session host

This crate composes the authoritative app service, one RT backend adapter and the
native RT engine/device host. It contains explicit profile-based attachment, authenticated
loopback app data/management connections, socket/shared-memory delivery and sibling
launch adapters. No UI or Lua runtime is linked into the host.

See [roles and CLI examples](../shoopdaloop/README.md). Socket clients can replace a
departed controlling lease without recreating the engine. Shared memory supports one
attachment only. Host shutdown requires current authority and reports MIDI reset,
render stop and resource cleanup; UI/launcher exit does not imply host shutdown. Explicit ownerless shutdown can
acquire a new fenced generation only when no live UI owns the session.

`cargo test -p shoop_host --features testing` runs private-path and real subprocess
recovery/launch tests. The testing feature's deterministic dummy input is confined
to fixtures and is disabled in ordinary production builds.

Explicit LAN endpoints and remote profiles are described in
[trusted-LAN setup](../../../docs/trusted-lan-networking.md). Portable connection
progression lives in `shoop_app_connection`; native hosting retains device lifetime.

Reusable profiles select endpoints without host/session IDs. Every connection
synchronizes fresh state. `connection::host_report` queries shared startup settings,
effective runtime status and diagnostics; the existing UI displays this information.

The native `shoopdaloop --manage-hosts` window monitors only profiles explicitly
provided in the current session, without taking a control lease. No host registry,
discovery directory or remembered host list is read or written. Credentials must
be supplied explicitly. The launcher uses bounded pipe messages for startup and
readiness; it writes a profile only to an explicitly requested export path.

Host diagnostics go to stderr unless `--log-file PATH` requests a private rotating
file. See the application README for profile-based roles, manager controls and
opt-in log retention.
