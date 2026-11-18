# Execution backends and audio drivers

The backend computes the audio. The driver supplies input, delivers output and
requests processing. The application's carrier endpoint carries commands,
responses and snapshots; polling it does not request an audio quantum.

| Backend | Dummy | CPAL | Direct JACK | Browser WebAudio |
| --- | --- | --- | --- | --- |
| Native | Paced, unpaced, controlled | Yes | Yes | Yes |
| Rust FPGA reference | All three modes | Yes | Yes | With `fpga-reference` in app and worklet |
| Marlin RTL simulation | All three modes | Bounded worker bridge | Bounded worker bridge | Unavailable |

Native device availability still depends on platform, build features and devices.
Marlin hosting is initially supported on Linux with Verilator 5.032. It executes
the real SP, JP, media store, chunk cache and AXI path. The native AXI responder
supplies simulated external memory. Set `SHOOP_FPGA_SOURCE_ROOT` to a source
checkout when running a host outside the build tree. A cold model build happens
before driver callbacks start; detached-host readiness permits up to five minutes.
Missing sources, unsupported builds and toolchain failures produce startup errors.

Select `--backend native|rust-fpga|rtl-sim` independently of `--driver`.
The host, in-process app and Host Manager use the same composition. Attaching a UI
keeps the existing host's selections. Enable `rtl-sim` on `shoop_host`,
`shoopdaloop` or the Manager to include the optional simulator. Default builds do
not include Marlin. Native live driver replacement remains supported; reference
and simulation sessions require a restart to change drivers.

## Dummy timing

`--dummy-mode paced|unpaced|controlled` is also stored in the dummy driver's
settings. Paced is the default. Unpaced runs as quickly as computation allows,
without changing the sample rate or loop timing. Controlled mode waits for the
host/test harness's independent `DummyControl::request_frames` handle. That
handle is available before application graph activation completes.

A controlled request consumes exactly the requested number of driver frames.
Incomplete FPGA quanta remain buffered. A graph switch requiring a quantum stays
pending when demand is zero; endpoint polling does not manufacture that quantum.
The driver handle can supply demand while the application waits for completion.
Ordinary dummy input is silence; test generators are explicit driver fixtures.

## Buffers, latency and frame counts

Drivers call `rt_runtime::host::Renderer` with borrowed f32 audio and MIDI buffers.
The native renderer normally runs inline. The reference also runs inline and
converts f32 to/from the processors' fixed-point sample format.

The FPGA quantum adapter adds exactly one quantum of latency. It retains partial
input and output for irregular callbacks, including controlled requests smaller
than a quantum. Driver frames delivered, complete processor frames and buffered
frames are distinct counts. Silence during priming does not advance the processor
without enough real input. Missing device inputs are silent; additional outputs
are silent. The supported FPGA graph uses the first input and first two outputs.

For physical Marlin drivers the bridge adds
`ceil(max_callback_frames / quantum) + 1` quanta to the adapter latency. This
bounded delay and the one-quantum adapter delay are reported together in the RT
limits. The callback copies samples through a prepared block pool and never waits
for the simulator, compilation, commands or space. Output blocks carry frame and
runtime-generation identifiers. After priming, missing, out-of-order or stale
output, overflow and owner failure latch a fault and produce silence. Restart
recovers the stream. A simulator that cannot meet real-time demand can therefore
connect successfully and subsequently fault. No real-time performance guarantee
is implied. Dummy processing may wait on its non-real-time driver thread.

## Simulator ownership and loading

One thread creates, uses and destroys each model. Between requested complete
quanta it services bounded upload and command turns. A command turn can clock
configuration ports and fences, but never asserts `run_valid`; frames, cursor
advancement and sync pulses belong to the SP's audio quanta.

The shared ARM-side endpoint compiles graph descriptions, allocates unused slots
and lowers to either RustTarget or RtlTarget. Both loaders use the ownership and
fence rules in [LOADING_DESIGN.md](../LOADING_DESIGN.md). RTL upload records are
shared with the testbenches and contain initializers only for fresh allocations.
Continuing loops retain their state, gain and media. Runtime mode commands read
actual RTL state and write it only between quanta; no shadow Rust SP runs.

The FIFO reserves completion space before publication. A pending graph switch
leaves its group unread as an ordering barrier, yields to render demand, and
completes only on the real RTL fence response. Snapshots and successful admission
do not release old allocations. Full responses, snapshots or reclaim storage do
not make the owner block on the application. Stop is an independent signal,
checked within simulation loops and delivered before driver joining. Teardown
joins the owner outside callbacks. Per-operation cycle budgets replace an
absolute simulated-session expiration.

## Browser reference

The browser main side owns compilation, allocation and the public carrier
endpoint. A bounded internal message transfers an already lowered Rust-target
image into the worklet's separate Wasm heap; no native pointers or Arc handles
cross that boundary. The worklet MessagePort handler decodes and stages it outside
rendering. Upload readiness, ordered switch commands, responses and coherent
snapshots return over their existing record path. The main allocator releases
old ownership after the switch response. Rendering itself performs no graph
compilation or image decoding and uses the same quantum adapter and SP/JP core.
The internal image format is versioned with the worklet configuration and is not
a public ARM/FPGA hardware image format. Rebuild the app and worklet together.

Build the app with `fpga-reference`, and pass `--features fpga-reference` to
`src/rust/shoopdaloop/build_worklet.py` (or set `SHOOP_WORKLET_FEATURES`). Browser
startup accepts the same backend setting/argument. Marlin produces a clear
unsupported-backend error in a browser.

FPGA graph admission uses the compiled program's resource requirements rather
than a fixed number of tracks or loops. Control-side mono track and loop identities
grow with the draft. The compiler checks the target's instruction, state, loop,
job, audio-buffer, history and port capacities; the ARM loader then allocates
physical slots while keeping the active graph's state and media views pinned.
Retained loops reuse their allocations. A graph that fits by itself can therefore
be refused if there is insufficient space to stage it beside the active graph.
Refusals identify the exhausted resource, preserve the active program and roll
back the application's draft. Recording duration remains limited by the separate
media memory pool, since future recorded audio is not known at compilation.

The shared `shoop_graph::GraphView` exposes strips, loops, buses, ports and
connections. It describes audio and MIDI, multichannel layouts, direct and dry/wet
tracks, processor identifiers/parameters, composite plans and controls. `Mirror`
implements this read-only interface without maintaining another graph model;
`RtBackend::graph_view()` exposes it for compilers and diagnostics. Logical port
identities are separate from their channel positions. Cursors, meters and transport
bookkeeping are excluded.

The carrier handoff captures an immutable owned snapshot of that view. Graph
snapshots travel in bounded command groups: BeginGraphImage declares quantum and
byte length, GraphImageData carries canonical 20-byte chunks, and ActivateGraph
completes the image. RT protocol version 39 covers the format. The transfer is
bounded at 1 MiB, independently of target memory budgets. Decoding and compilation
happen on the ARM side; no semantic graph or graph unpacker is sent to the FPGA.
Intermediate replies acknowledge upload progress; only the activation reply
confirms the ownership fence. A new header or connection resets an incomplete
upload.

The shared compiler consumes `GraphView` directly. It expands actual channel
connections into a dependency graph, schedules the sync and processing passes
independently, removes unused computations and emits the shared program. Direct
multichannel strips and loops, multiple buses, fan-in/fan-out, cross-channel
routing, gain/balance/mute, manual monitoring and boundary smoothing are supported
within target dimensions. There is no fixed Master topology. MIDI, processors,
composites and automatic monitoring still need execution support and are refused
explicitly. Unsupported graph edits restore the previous application layout and
do not alter the active program. The compact BeginGraph/ConfigureLoop records
remain component fixtures using the same compiler.

Persistent values are keyed by owning object, purpose and channel. Each media
channel has its own view, while all channels of a loop use one cursor. The loader
pins continuing values and allocates fresh configuration for changed immutable
coefficients. Media-view tables have one entry per possible state value; the
reference, browser and Marlin loaders use the same allocation rule.

The current target has 128 instructions, 64 state values, 32 loop cursors,
64 media views, 128 queued jobs and 64 audio buffers. With this lowering, 21 mono
loops routed to stereo Master fit; 22 need 66 buffers and are refused. With all
loop outputs disconnected, 31 loops exactly fill instructions, jobs and buffers.
These counts follow from the program and target dimensions. Live replacement can
reach a state-slot limit sooner. Scratch buffers retain the processors' one-write
contract. No instruction or processor RTL changes are required.

Host-driver geometry remains separate: the current prepared reference/Marlin
application renderer exposes one capture and two playback channels. A wider
compiled graph needs a target and driver with sufficient I/O dimensions; internal
loop channels need not match host channels. Unsupported runtime commands continue
to be refused or ignored through the ordinary interface. Native backend
prepared-pool limits are unchanged.

## Verification

The suites retain native driver, MIDI, JACK publication/recovery and browser
coverage. Added checks cover allocation-free irregular adaptation, dummy modes,
actual reference device input/output, exact RTL/reference audio and snapshots,
serialized command-only operation, switch/fence ordering and saturated response
credits. Mock bridge tests cover missing/stale outputs and stop without waiting
for simulation. Small actual RTL application/UI scenarios exercise live loading.
The worklet reference suite transfers serialized images between independent
owners and checks record/play/reload under a render allocation guard; it also runs
as Wasm tests. Device qualification and simulator speed remain separate concerns.
