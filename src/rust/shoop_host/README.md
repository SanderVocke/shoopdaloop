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

## Backend selection

`--backend native|rust-fpga|rtl-sim` selects the execution implementation at host
startup. `native` is the default. `rust-fpga` requires the `fpga-reference` Cargo
feature. `rtl-sim` is available on Linux behind the `rtl-sim` Cargo feature.
See [shared drivers](../../../docs/shared_drivers.md) for its serialized owner,
latency and simulation deadline policy. Backend choice is independent of audio-driver
and UI-transport selection; unsupported backend/driver combinations fail before
starting a device. The old `--audio-backend` and `--fpga-reference` CLI switches
are not accepted.

The same choice is stored as `backend` in host settings and is exposed by the
application's startup preferences and Host Manager's New Host dialog. For example:

```json
{"backend":"rust-fpga"}
```

This minimal configuration uses the reference startup defaults. Explicit
configuration values are preserved and validated. Native driver preferences are
used for native hosts only. Host Manager retains its separate audio-driver
selection; choose Dummy for the current reference implementation.

## Rust FPGA reference with the UI

Build the host and UI, then start the reference host:

```sh
cargo build -p shoop_host -p shoopdaloop --no-default-features --features shoop_host/fpga-reference
target/debug/shoop_host --backend rust-fpga --name "Rust FPGA reference"
```

Attach with Host Manager, or use the management address printed by the host:

```sh
target/debug/shoopdaloop --ui-only --host-address 127.0.0.1:PORT
```

Choose `--driver dummy` for device-free processing, `cpal` for system audio,
or `jack` for direct JACK. The selected driver supplies timing and actual audio;
Dummy input is silence unless a test fixture supplies a generator. Dummy pacing
can be paced, unpaced or controlled. Saved driver preferences apply independently
of backend selection. The reference has zero smoothing and automatic Master
outputs. Its default standalone media pool is 16 MiB; memory capacity and quantum
can be supplied with `--config PATH`. The quantum adapter reports one quantum of
processing latency. JACK exposes fixed `audio_in_1`, `audio_out_1` and
`audio_out_2` ports; CPAL uses the first input and first two outputs.

The supported graph contains the built-in Sync track and mono audio tracks.
Choose Regular, Mono and no MIDI in Add Track. Track and loop counts are admitted
by the compiled program's actual resource requirements and the loader's free
physical slots, including active graph ownership during a switch. There is no
separate fixed track/loop profile limit. Capacity refusals name the exhausted
resource and restore the previous layout. The UI retains its normal controls;
unsupported reference operations are refused through the ordinary error path. Extra buses, track/Master mix
controls, effects, MIDI, monitoring, media reads/import/export, loop clearing,
copying, composites and general routing are unavailable. Use track removal and
creation for fresh empty loops. Loop gain, record/play/stop, sync scheduling,
selection, names and supported live structural changes can be exercised normally.

The application service automatically commits a complete graph edit, including
its loops and default connections. `RtBackend` collects that desired graph on the
control side, and the reference endpoint compiles it through the shared compiler
and `RustTarget` outside the processing thread. The ARM allocator preserves
continuing allocations and prepares unused slots only. The operation completes
after the quantum-boundary program switch and ownership fence. Commands behind a
pending switch wait in the existing application queue. Failed edits restore the
last committed control-side graph without resetting running processor state.
The UI sends its existing commands; it does not need `ActivatePreparedGraph`.
Lower-level clients using `RtBackend` directly can still stage edits and request
explicit activation.

Input routing can be connected/disconnected; each mono output must feed both
Master channels or neither. Sync relationships must be acyclic (self sync is
allowed). Quantum, sample rate and simulated I/O geometry stay fixed for a host's
lifetime. Native execution remains the default. This restricted graph support is
an implementation limit, not a separate application-profile abstraction.

## Application and UI verification

The temporary application client exercises recording, scheduled transitions,
gain, plan clearing, live program replacement and recycled logical identities:

```sh
cargo run -p shoop_host --no-default-features --features fpga-reference --example fpga_reference_client -- --host target/debug/shoop_host
cargo test -p shoop_host --no-default-features --features fpga-reference --test prepared_graph --test reference_activation
cargo test -p shoopdaloop --no-default-features --features fpga-reference fpga_reference_ui_operates
```

The UI test starts the actual application runtime against a socket reference
host, renders egui offscreen, records/plays/stops, changes loop gain, adds a loop
during playback, removes/recreates the track, reattaches, and closes both UI and
host. Separate tests hold
the activation acknowledgment to check command ordering and operation completion,
and inject transport/compiler refusals to check restoration and retry.
The application client writes diagnostics under `work/fpga-reference-client`
(override with `--output`).

`RtlTarget` accepts the same logical graph and loads through the Marlin testbench
loader. Marlin does not yet have an application host with the runtime command and
snapshot frontend used by this UI integration.

## Dummy pacing

Dummy startup accepts `--dummy-mode paced|unpaced|controlled`, independently of
`--backend`. The same mode is stored in the Dummy driver's JSON `mode` field and
in startup preferences/Host Manager. Omitted modes remain paced. Unpaced advances
sample time continuously without sleeping; controlled mode waits for frame
requests through the host driver's `DummyControl` handle. Attaching a UI does not
change pacing and does not receive those startup overrides.

Fixed-quantum renderers use a prepared adapter with one quantum of reported
latency. Partial driver requests remain buffered; they do not cause padded engine
quanta. Driver frames delivered and complete engine frames are distinct counters.
