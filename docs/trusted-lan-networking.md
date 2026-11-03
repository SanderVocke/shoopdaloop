# Trusted-LAN session connections

A native UI can control a session host on another Linux, macOS or Windows
machine. The host owns audio/MIDI devices, playback, recording and accepted
operations. The UI owns files, dialogs, codecs, scripts and controller bindings.
Both sides must use application protocol version 12.

Connections use TCP with keyless compatibility negotiation. Compatible clients may
connect to compatible hosts; certificates, credentials and trust records are absent.
Audio streaming, simultaneous controlling UIs and browser remote connections remain
outside scope.

## Start and connect

For local operation, open **Host Manager → New host…**. The form configures audio,
MIDI, capacity and connections and starts a detached process with OS-assigned ports.
The manager rediscovers local hosts by process and owned-socket inspection. It does
not scan ports, remember hosts on disk or use a registry/discovery listener.

For CLI operation, `shoop_host` starts with dummy audio and local assigned ports;
`--config PATH` is optional. `shoopdaloop --host-only` forwards host options.
For a LAN server at `192.168.1.50`, explicit ports are useful for firewall rules:

```sh
shoop_host --lan --endpoint 192.168.1.50:15400 --management-endpoint 192.168.1.50:15401
shoopdaloop --ui-only --host-address 192.168.1.50:15401
```

Use `shoop_host.exe` on Windows. Without `--lan`, socket setup accepts loopback
addresses only. LAN delivery requires socket transport. IPv6 addresses use brackets,
such as `[fd00::50]:15400`. DNS hostnames are accepted within a bounded connection
deadline. With wildcard listeners, a client can inspect the host through its concrete
IP and actual port; `--advertise-data HOST:PORT` and
`--advertise-management HOST:PORT` optionally supply client-facing destinations.

The manager's **Advanced → Connect to address…** accepts either host endpoint.
**Import profile…** and **Export profile…** serve explicit advanced configuration;
`--export-profile PATH` and `--host-profile PATH` provide CLI equivalents. Profiles
are keyless endpoint selections and omit process/session identities. They are not
saved or restored automatically. Old credential-bearing profiles and wire versions
are rejected. Remote hosts are added explicitly; discovery is local to the machine.

App connections send the protocol version and data/management role after the generic
compatibility greeting. Both are checked before carrier/session admission, including
shared-memory allocation. Inspection negotiates a separate intent and returns a
bounded current host description. It never acquires a controller, consumes a shared
attachment or changes musical state. Actual attachment checks protocol compatibility,
acquires exclusive control and synchronizes a complete snapshot. A healthy controller
cannot be evicted by another attachment or an ownerless shutdown request.

After loss, the last projection is marked stale. **Connect to endpoint** runs the
same connection flow against whatever the endpoint currently owns, including a
restarted host with an empty session. Previous projections, transfers and pending
request assumptions are discarded. Session dialogs, edit history and old file
callbacks are cleared; local preferences and already finishing file writes remain
UI-owned. Commands, session loads, initialization and
scripts are never replayed. Configured scripts remain visible but disabled until
explicitly started. A connection-scoped control lease fences delayed traffic and
shutdown requests from a departed controller.

**Close UI only** preserves the host. **Close UI and host** waits for confirmed
MIDI reset, device stop and cleanup. An uncertain shutdown retry checks the same
operation. To shut down a host with no live controller:

```sh
shoopdaloop --shutdown-host --host-address 192.168.1.50:15401
```

Saved files, WAV and MIDI imports/exports stay on the UI machine. Transfers use
the existing bounded segmented persistence protocol; images above 32 MiB do not
require a shared filesystem. Loss before commit discards staging; an accepted
commit remains host-owned. A new connection observes current state and host jobs;
it does not restore old per-client results or assume a lost request succeeded or
failed.

## Host settings and status

**Session → Connected host settings…** queries the selected host through its
management endpoint. It shows requested startup settings, effective audio state,
sample rate/buffer size, current smoothing and Master auto-connect, diagnostics,
and the number of host-owned operations in progress. Refresh queries current
state without taking control or modifying the host.

The native API is `shoop_host::connection::host_report(target, timeout)`; the
portable management request is `ManagementRequest::Settings`. Reports use bounded
1024-byte chunks with a 64 KiB total ceiling. The portable server remains
`no_std` + `alloc`.

`HostSettings` is shared by native `host.json` and local UI launch defaults and
validation. The native UI already stores user preferences as JSON. While
connected to an external host, preference saves apply to future local launches;
they do not reconfigure that host. The panel is read-only. Create a new host for driver/pool changes; the manager form also configures startup settings. The native `shoopdaloop --manage-hosts` window can inspect local hosts and explicitly
added remote profiles without taking control. Remote configuration editing/hot
swapping remain future work.

For example, `host.json` can contain:

```json
{
  "driver": { "Dummy": { "sample_rate": 48000, "buffer_size": 256 } },
  "pools": { "tracks": 16, "loops": 128, "channels": 2, "loop_memory_mb": 256 },
  "master_auto_connect": true,
  "loop_smoothing_ms": 3,
  "transport": "socket"
}
```

An exported `remote.json` looks like this (replace the address):

```json
{
  "version": 12,
  "name": "Studio host",
  "endpoint": "192.168.1.50:15400",
  "management": "192.168.1.50:15401",
  "network": { "heartbeat_interval_ms": 1000, "heartbeat_timeout_ms": 5000, "snapshot_hz": 30 }
}
```

## Scheduling and budgets

Host options `--heartbeat-interval-ms`, `--heartbeat-timeout-ms` and
`--snapshot-hz` default to 1000, 5000 and 30. The exported profile carries these
settings for its client. The interval must be 10–60000 ms, timeout at least twice
the interval and no more than 300000 ms, and snapshot cadence 1–60 Hz. Editing a
profile does not reconfigure the running host.

Each side sends a probe and requires its matching reply. Expiry retires only that
connection and releases its controlling lease; rendering, recording and accepted
work continue. Fresh attachment generations fence the departed controller.

The native host admits at most four data connections, four management connections
and sixteen pending compatibility/inspection handshakes per listener. Pending greetings and description writes have a two-second deadline. A LAN stream has bounded reliable,
telemetry and bulk queues (8/1/8 records), and requests a bounded socket send
buffer. Application scheduling limits queued records to eight, coalesces only
unsent periodic snapshots and preserves started transfers. Initial synchronization
and operation confirmations bypass the periodic cadence. Heartbeats and ordinary
control replies progress between snapshot/media/persistence chunks. TCP still
preserves byte order and can delay feedback on a congested link.

The deterministic acceptance fixture uses a real service/engine, 1 MiB/s in both
directions, 20 ms RTT and 10 ms pump steps. Ordinary controls complete in 40 ms
during a 1 MiB upload and 20 ms during download, below the 250 ms acceptance
budget. This is a fixture result, not a latency guarantee for arbitrary networks
or for committing a large image into the engine.

## Future Cortex-A/RTOS composition

`shoop_transport::stream` owns record buffering and partial I/O over `ByteIo`.
The host implements immediate `try_read`/`try_write` results (progress,
would-block, interruption, EOF or failure). The prepared stream pump does not
allocate, sleep or wait. `shoop_transport::handshake` validates a versioned magic and connection/inspection intent and accepts elapsed time. The native host handles descriptions outside portable session admission.

`shoop_app_connection` uses `no_std` with `alloc` and implements session data,
management, heartbeats, state/media scheduling and segmented client uploads over
`RecordPort`. It does not depend on native sockets, threads, files, clocks, Lua,
the client projection or an engine/device host. A composition root supplies
a fresh connection nonce and the initial data `CarrierReady` greeting, drives each connection
with elapsed monotonic time, calls `Service::tick` once per service turn, and
invokes `Service::detach` when a connection retires. It also executes device
shutdown and reports cleanup completion back to the service. Management emits its
own versioned greeting through the portable pump.

An RTOS adapter can drive the same code from a control task using BSD sockets,
lwIP or another byte-stream stack. It supplies scheduling, timer-wrap conversion,
connection admission, a heap and the existing engine's required atomics. Run the
service/network task separately from rendering: projection, persistence and
serialization can allocate and are not hard-real-time operations. Current
snapshot/media/persistence ceilings remain in force; they do not promise that a
particular board has enough memory. See the existing persistence contract for
peak staging and capture budgets.

Both `armv7a-none-eabi` and `aarch64-unknown-none` builds are CI gates:

```sh
cargo build --locked --lib --no-default-features -p shoop_rt_engine -p shoop_app -p shoop_app_connection -p shoop_rt_backend -p shoop_script_resources --target aarch64-unknown-none
```

Replace the target with `armv7a-none-eabi` for 32-bit Cortex-A. These are library
builds and simulated adapter tests, not an RTOS implementation or board execution.

## Verification

- `cargo test -p shoop_transport`: fragmented streams, bounded queues, fragmented compatibility greetings
  and deadlines and allocator-guarded prepared progression.
- `cargo test -p shoop_app_connection`: fake-clock heartbeat and authority tests,
  host-driven service progression and the throttled bidirectional transfer test.
- `cargo test -p shoop_host --features testing`: two-process LAN/IPv6/DNS,
  non-loopback Linux, compatibility/version refusal, silent network loss and existing
  local recovery cases.
- The native UI recovery test opens a remote profile, retains recording across
  close/reattach and confirms host shutdown.
- `SHOOP_PROBE_LAN=1 shoopdaloop --probe-engine-transport --session-transport socket`
  and `SHOOP_PROBE_LAN=1 shoopdaloop --probe-persistence --session-transport socket`
  run packaged LAN workflows, including >32 MiB exact audio/MIDI and connection
  loss around persistence. Native package CI runs both on Linux/macOS/Windows.

Canonical wire fixtures are checked on each native platform. Mixed-OS physical
pairings and RTOS hardware execution are reported separately when performed.
