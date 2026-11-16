# Independent native session host

This crate composes the authoritative app service, RT backend and native engine/device
host. It provides compatible keyless data/management connections, socket/shared-memory
delivery and detached launch adapters. No UI or Lua runtime is linked into the host.

Run `shoop_host` for dummy audio with OS-assigned local ports, or run `shoop_host_manager` and use its
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

Live JACK graph inventory and connection commands use the same session protocol.
See the [graph contract and scope](../../../docs/native_session_recovery/jack-graph.md).

Native live driver changes use confirmed service-owned transactions. Audio/MIDI,
controls, processors, composites, routes and public IDs survive supported changes;
loops stop and client loss does not cancel accepted work. Failed transitions restore
the source capture or report fatal recovery failure. Startup preference saving is
separate and retryable. `--driver-preferences PATH` selects the host-local store;
without it, `--config FILE` uses a sidecar, otherwise the host's application config
directory is used. Saved driver preferences override configuration; explicit audio
CLI flags override both. See the
[audio-driver switching contract](../../../docs/native_session_recovery/audio-driver-switching.md).

## Temporary Rust FPGA-reference client

Build and run the live-graph application-API exercise without the UI:

```sh
cargo build -p shoop_host --no-default-features --features fpga-reference --bin shoop_host --example fpga_reference_client
cargo run -p shoop_host --no-default-features --features fpga-reference --example fpga_reference_client -- --host target/debug/shoop_host
```

The client launches a reference host, attaches over its real socket API, creates a
mono track, connects its input and submits `ActivatePreparedGraph`. `RtBackend`
collects the desired layout on the control side. Activation sends one bounded
immediate group containing a graph header, loop descriptions and activation.
The endpoint compiles that description through the shared compiler and `RustTarget`
outside the processing thread. The ARM allocator reuses continuing allocations,
initializes only unused slots, and stages a bound program for the independently
paced worker. An ordered `SwitchProgram` selects it at a quantum boundary. The
operation completes after the ownership fence; processing frames remain continuous.

The client then uses immediate transition plans, gain and plan clearing, checking
their results through application snapshots. It records the sync loop, records a
second loop between source pulses, plays it, changes gain and stops both loops.
It replaces programs during recording and playback, changes input routing, and
replaces the user track with eight new loops, including recycled logical identities.
It writes `config.json`, `profile.json`, `host.log` and `report.json` under
`work/fpga-reference-client` (override with `--output`). The same client is run by
`cargo test -p shoop_host --no-default-features --features fpga-reference --test prepared_graph`.

This profile supports the built-in sync track plus one mono audio track, up to
16 primitive loops in total, acyclic sync relationships (self sync allowed),
optional input connections and either both Master output channels or neither.
Track/Master controls must remain at their defaults, with zero smoothing and
automatic Master connections. The input supplies deterministic 0.25 samples;
stereo output is computed without a physical audio device. Media import/export,
processors, MIDI, monitoring, arbitrary routing and UI integration are outside
this profile. One prepared update may be outstanding. Quantum, sample rate and
physical I/O remain fixed. Unsupported legacy mutations receive
refusals; they are never sent to the execution queues. Native execution remains
the default. Select the reference host with `fpga_reference: true` in startup
configuration and build with the corresponding feature.

`RtlTarget` accepts the same logical graph and its packed configurations still load
through the testbench loader. Marlin does not yet have the runtime graph/command
frontend used by this application client.
