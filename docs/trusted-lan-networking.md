# Trusted-LAN session connections

A native UI can control a session host on another Linux, macOS or Windows
machine. The host owns audio/MIDI devices, playback, recording and accepted
operations. The UI owns files, dialogs, codecs, scripts and controller bindings.
Both sides must use the same application protocol version (currently 9).

Connections use unencrypted TCP and the existing 32-byte token check. This mode
targets trusted LANs; TLS, pairing, automatic discovery and Internet traversal
are outside its scope. Audio streaming and simultaneous controlling UIs are not
provided. Browser remote connections are not implemented.

## Start a host

Create a UTF-8 `host.json` containing `{}` to select the dummy driver, or use the
existing host driver/pool configuration. Generate a credential once:

```sh
shoop_host --generate-credential host.key
```

For a server at `192.168.1.50`, choose two available ports:

```sh
shoop_host --config host.json --lan --endpoint 192.168.1.50:15400 --management-endpoint 192.168.1.50:15401 --credential-file host.key --export-profile remote.json
```

Use `shoop_host.exe` on Windows. Alternatively the application can forward the
same host options with `shoopdaloop --host-only --host-config host.json ...`.
Allow the chosen TCP ports through the server's LAN firewall.

Without `--lan`, listeners and process/engine sockets retain their loopback-only
policy. LAN delivery requires sockets, an explicit credential file and a profile
export path. The export refuses to overwrite an existing file. A restarted host
normally has fresh host/session identities: export a fresh profile and distribute
it to clients. This is separate from replacing a UI while the same host survives.

To bind a wildcard interface, supply concrete advertised destinations with
`--advertise-data HOST:PORT` and `--advertise-management HOST:PORT`. Wildcard
addresses and zero ports are not usable profile destinations. IPv6 destinations
use bracket notation, for example `[fd00::50]:15400`. Native clients can also use
DNS hostnames. DNS, TCP setup, token exchange and endpoint greeting share the
connection deadline; resolver concurrency and pending requests are bounded.

## Connect a UI

Copy `remote.json` and the matching `host.key` to the UI machine. The exported
profile references the credential filename relative to the profile's directory;
it contains neither the token bytes nor a server filesystem path. An absolute
client-local credential path is also supported.

The credential file must remain owned by the current user and deny access to
other users. On Unix, use `chmod 600 host.key`. On Windows, ensure the copied file
has a protected ACL granting the current owner access (SYSTEM and Administrators
are allowed), as for existing local host credentials. For a new copy with only
inherited permissions, PowerShell can remove inheritance and grant the current
user access:

```powershell
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
icacls .\host.key /inheritance:r /grant:r "${me}:(F)"
```

Open **Session → Open remote profile…**, or start explicitly:

```sh
shoopdaloop --ui-only --remote-profile remote.json
```

The UI checks the data endpoint's host/session identity before attachment. The
management endpoint receives the same identity check before any shutdown request.
A healthy controlling UI is refused without eviction if another UI tries to join.
Remote profiles are separate from local registrations: listing/pruning local
hosts never consults a remote PID or deletes a remote profile or credential.

After connection loss, the UI shows its last projection as stale and offers
**Reattach to this host**. Reattachment obtains coherent state and accepted
operation results. It does not replay commands, load a session file, rerun startup
initialization or restart the previous script VM.

**Close UI only** preserves the host. **Close UI and host** waits for confirmed
MIDI reset, device stop and cleanup. An uncertain shutdown retry checks the same
operation. To shut down a host with no live controller:

```sh
shoopdaloop --shutdown-host --remote-profile remote.json
```

Saved files, WAV and MIDI imports/exports stay on the UI machine. Transfers use
the existing bounded segmented persistence protocol; images above 32 MiB do not
require a shared filesystem. Loss before commit discards staging; an accepted
commit remains host-owned and is reconciled after reattachment.

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
and four pending token handshakes per listener. A LAN stream has bounded reliable,
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
allocate, sleep or wait. `shoop_transport::handshake` reuses the token/ack format
and accepts elapsed time; the host supplies entropy or provisioned credentials.

`shoop_app_connection` uses `no_std` with `alloc` and implements session data,
management, heartbeats, state/media scheduling and segmented client uploads over
`RecordPort`. It does not depend on native sockets, threads, files, clocks, Lua,
the client projection or an engine/device host. A composition root supplies
identities and the initial data `CarrierReady` greeting, drives each connection
with elapsed monotonic time, calls `Service::tick` once per service turn, and
invokes `Service::detach` when a connection retires. It also executes device
shutdown and reports cleanup completion back to the service. Management emits its
own identity greeting through the portable pump.

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

- `cargo test -p shoop_transport`: fragmented streams, bounded queues, token
  exchange/deadlines and allocator-guarded prepared progression.
- `cargo test -p shoop_app_connection`: fake-clock heartbeat and authority tests,
  host-driven service progression and the throttled bidirectional transfer test.
- `cargo test -p shoop_host --features testing`: two-process LAN/IPv6/DNS,
  non-loopback Linux, identity/credential refusal, silent network loss and existing
  local recovery cases.
- The native UI recovery test opens a remote profile, retains recording across
  close/reattach and confirms host shutdown.
- `SHOOP_PROBE_LAN=1 shoopdaloop --probe-engine-transport --session-transport socket`
  and `SHOOP_PROBE_LAN=1 shoopdaloop --probe-persistence --session-transport socket`
  run packaged LAN workflows, including >32 MiB exact audio/MIDI and connection
  loss around persistence. Native package CI runs both on Linux/macOS/Windows.

Canonical wire fixtures are checked on each native platform. Mixed-OS physical
pairings and RTOS hardware execution are reported separately when performed.
