> Development/reference engine-only process fixture. The production application
> launches `shoop_host`, which owns the app service and engine together. This crate
> remains for independent carrier/engine behavior tests and is forbidden in the
> production frontend graph. See the feature parity ledger for restoration evidence.

# Native engine process host

`shoop_rt_host_process` supplies a headless companion executable and parent launch/
supervision library. `Pending` authenticates the launched process, sends paginated
discovery/configuration through `shoop_rt_protocol::host`, then starts the existing
native engine/device host over socket or shared-memory records. `Started` returns
a common protocol client, startup warnings and a separately owned `Supervisor`.
Drop the backend before the supervisor for graceful endpoint/MIDI/render cleanup.

The executable is resolved beside the app using an absolute path, independently
of cwd/PATH. A private stdin bootstrap carries authentication and mapping setup;
an independent stop signal remains available after data-channel failure. Startup
rollback and bounded shutdown reap incomplete/stopped launches. `cpal` (default)
adds native audio/MIDI; disabling it leaves the dummy host.

One child serves one app session. Data/control-channel EOF, malformed traffic or a
second handshake permanently retire control while retaining the endpoint, engine
and devices. A healthy engine continues playback/recording without control, even
after an app crash. Explicit normal app close stops it; an engine orphaned by a
crash needs external termination. No reconnect, automatic restart or session
adoption is provided. Actual driver/render faults keep the native fault policy.

All process, socket and mapping work stays off the render/endpoint boundary.
The optional `testing` feature adds fixture clocks/counters and fault injection;
production builds have no controlled-render commands or fixture status files.

```sh
cargo build -p shoopdaloop -p shoop_rt_host_process
cargo test -p shoop_rt_host_process
```
