# Portable Rust FPGA reference host

The carrier endpoint compiles/binds graph descriptions and exchanges immediate
commands, responses and snapshots. The prepared renderer owns persistent state,
job and media processors. It accepts f32 input/output blocks, converts complete
quanta to/from FPGA fixed-point samples, and never sleeps or opens a device.

A prepared quantum adapter adds one quantum of latency and retains partial input.
The fixed driver exposes one physical input and two outputs. The compiler can
route these through multichannel internal loops, strips and buses; the result
feeds the first two outputs and extra device outputs are silent. A missing input
is silence. MIDI remains unsupported by this execution profile.

Preparation publishes an initial empty snapshot without running audio. Graph
installation can remain pending in controlled mode until the independent driver
handle requests frames. Polling the endpoint never advances the renderer.

The native host wraps this core in shared Dummy/CPAL/JACK drivers. The portable
core has no production dependency on native devices or a concrete native engine;
it is also the core used for browser integration.
