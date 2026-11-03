# Independent native session host

This crate composes the authoritative app service, RT backend and native engine/device
host. It provides compatible keyless data/management connections, socket/shared-memory
delivery and detached launch adapters. No UI or Lua runtime is linked into the host.

Run `shoop_host` for dummy audio with OS-assigned local ports, or use the manager's
**New host…** form for audio/MIDI and capacity settings. Configuration is passed over
bounded startup pipes. Optional `--config`, `--export-profile` and `--log-file` paths
serve explicit advanced inputs/outputs; normal launch and attachment create none.

The native manager discovers `shoop_host` processes through live OS process/socket
inspection on Linux, macOS and Windows. A bounded inspection greeting returns the
current process identity, instance, endpoints and attachment availability before
session admission. Probes never acquire control or consume shared memory. There is
no registry, remembered host list, fixed port range or extra discovery service.

Socket clients can replace a departed controlling lease without recreating the
engine. Shared memory supports one data attachment. Graceful shutdown retains
current-generation authority, live-controller protection and confirmed MIDI/device
cleanup. Closing the UI or manager leaves the host running. Explicit local Force stop
uses OS identity checks and reports termination separately from graceful cleanup.

Profiles remain optional endpoint selections without process/session IDs. Every
attachment synchronizes fresh state. `connection::host_report` queries startup
settings, effective runtime state and diagnostics without acquiring control.

Diagnostics go to stderr unless `--log-file PATH` requests an ordinary rotating file.
See [roles and manager controls](../shoopdaloop/README.md) and
[LAN setup and portable connection scheduling](../../../docs/trusted-lan-networking.md).

`cargo test -p shoop_host --features testing` covers real subprocess recovery,
keyless compatibility, harmless inspection, detached launch and optional output.
Deterministic dummy input is confined to test fixtures.
