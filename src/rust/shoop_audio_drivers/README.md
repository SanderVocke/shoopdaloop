# Shared native audio drivers

Dummy, CPAL and JACK own device discovery, audio/MIDI buffers, callback pacing,
port publication, stream diagnostics and shutdown. A caller supplies a `Template`
with a backend `Factory`. Once the device geometry is known, that factory returns
prepared buffers, a `Processor` implementing `rt_runtime::host::Renderer`, an
optional status observer and opaque control-side ownership. Device code never
constructs a native or FPGA execution engine.

Preparation and typed control-owner handover happen before callbacks start.
`Processor::finish` runs after callbacks stop, on the lifecycle owner. The native
composition facade retains its protocol-client and stopped-engine API. Production
build dependencies are audited to exclude concrete execution engines and UI code.
