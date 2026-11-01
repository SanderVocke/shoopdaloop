# MessagePort transport

Both record-delivery halves of the browser engine connection, independent of
engine semantics and audio/MIDI hosting. `client::RecordBatches` batches outbound
records and splits received records; `server::{Records, RecordLanes}` manages
prepared buffers. Both use the shared record adapters from `shoop_rt_transport`.

The optional `browser` feature exposes `browser::Port`, which posts one batch per
main-thread turn through a Web `MessagePort`, tracks terminal errors and permits
a host-owned sideband lane. It brings browser bindings only on Wasm. The worklet
build uses no browser feature or JS imports. MIDI device protocols, audio buffers,
worklet loading and engine lifetime stay in the host/application.

```sh
cargo test -p shoop_rt_transport_message_port
```
