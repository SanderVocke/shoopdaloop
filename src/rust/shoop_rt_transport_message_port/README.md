# MessagePort record transport

Both ends of the browser record carrier live here, without engine or application
dependencies. `client::RecordBatches` assembles main-thread batches;
`server::RecordLanes` adapts prepared worklet buffers to the same `RecordPort`
interface. Both use `shoop_rt_transport`'s record client/server, framing and codec.

The optional `browser` feature exposes `browser::Port`, which posts transferable
record batches at the end of a main-thread turn. Its optional sideband callback
lets a host join device bytes to that turn without putting MIDI/device semantics
in this crate. The default feature set has no browser bindings and is used by the
import-free worklet artifact. Build that artifact in its own Cargo invocation so
main-thread features are not unified into it.

`cargo test -p shoop_rt_transport_message_port` covers batch assembly, malformed
framing, closure and the shared lifecycle/credit/bulk contract. The worklet host
keeps audio, MIDI lane formats, configuration, ABI and rendering.
