# Engine protocol connections and carriers

`shoop_rt_transport` provides `client::ProtocolClient`, credit/correlation state,
`carrier::{Carrier, Endpoint, Outbox}`, typed `in_process::InProcess` delivery and
`records::{RecordClient, RecordServer, RecordPort}`. It depends only on the logical
messages and canonical codec in `shoop_rt_protocol`, never on an engine or host.

An in-process connection passes typed messages without encoding. Encoded carriers
implement `RecordPort` and reuse the record client/server. Prepare the bounded
outbox and codec buffers before callback work; pumping does not allocate.

The `contracts` feature exposes test fixtures and shared lifecycle assertions.
Run `cargo test -p shoop_rt_transport`; the crate's dev dependency enables these
fixtures for its own tests. Native and Wasm use the same connection implementation.
