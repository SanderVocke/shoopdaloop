# Host-side track processor contract and evidence

Implementation branch: `feat/hosted-track-processors`, based on
`feat/fixed-quantum-runtime` at `2e19210b`. Behavioral reference: `master` at
`92079fb6` and the retained reference fixtures. The implementation checklist is
in [the plan](../../PLAN_HOSTED_TRACK_PROCESSORS.md). This document records the
contract; no production support or completed validation is claimed yet.

## Host configuration and resource contract

Each implementation has an independent Cargo feature (`builtin-fx`, `oxisynth`)
and a startup policy (`auto`, `on`, `off`). Auto enables a compiled implementation
only when it supports the actual host sample rate. Explicit On fails startup if
the implementation is missing or the rate is unsupported. Off allocates no DSP
slots or decoded resources. Desktop/browser product builds include both features
by default; no-default-feature host builds include neither. Client compilation
does not determine the connected host's capabilities.

Initial defaults are two simultaneously active FX instances, two synth instances,
and a 256 MiB aggregate processor storage ceiling. Preparation reserves a second
complete candidate bank for transactional replacement, plus fixed lifecycle and
control metadata. Instance counts are independent of ordinary track capacity;
limits and preparation failures are observable. The budget includes actual
buffered storage, FX units/delay lines, synth voices and decoded SoundFonts in
both banks, not merely serialized asset sizes. Startup refuses an enabled pool
that cannot fit its checked budget; it never silently shrinks the requested pool.

FX supports the validated host sample rate. The pinned OxiSynth supports
8,000–96,000 Hz inclusive. Auto advertises synth unavailable outside this range;
explicit On refuses startup. Initial real processing uses M=1, S=0 and thus one
graph quantum of node latency. Host buffering is a separate delay.

The per-side audio limit is the negotiated engine strip width, at most eight.
FX slots support all widths from one through that limit, with distinct linked
stereo and isolated non-stereo behavior. The complete maximum shape is eight dry
plus eight wet channels. Synth is fixed at two dry/two wet/one dry MIDI; it ignores
dry audio samples. FX also requires dry MIDI. External tracks support independent
dry/wet widths and optional dry MIDI where native host ports support the shape.
Bus processor catalogs remain empty and Carla remains unavailable.

## Routing and media matrix

The reference routing predicate is `shoop_backend::dry_wet_routing_state`;
`next` below means the reference's pre-record/pre-rerecord admission window.

| Demand | Dry live input | Processor / external send | Wet live feed | Capture/playback |
| --- | --- | --- | --- | --- |
| Stopped, monitoring off | Muted passthrough | Sleeping | Muted | None |
| Monitoring | Enabled | Active | Enabled | No recording |
| Ordinary recording/replacing, or next recording | Enabled independently of monitoring | Active | Monitoring-dependent | Preserve master dry/wet capture policy |
| Regular playing | Monitoring-dependent | Monitoring/other-loop-demand-dependent | Monitoring-dependent | Recorded wet bypasses processor |
| PlayingDryThroughWet | Monitoring-dependent plus recorded dry | Active | Enabled | Recorded dry audio/MIDI feeds processor |
| RecordingDryIntoWet, or next rerecord | Live passthrough muted; monitoring forced off | Active from recorded dry | Enabled | Preserve dry; capture replacement wet |

Combine demand across every loop on a track before deciding processor activity.
Default actions resolve the current track preference; explicit Play stays regular.
Immediate/scheduled transitions, cancellation, sync, grab/replacement and wraps
must preserve one logical loop identity and consistent per-role timing. Capture
the applied processor state at the reference wet-take boundary; later edits do
not rewrite it. RestoreRecordedFxState is atomic and refuses without damage.

Logical channel order is dry audio, dry MIDI when present, then wet audio.
Persistence and exact media selection retain named roles; audio import/export
orders dry audio before wet audio without flattening their identities. External
ports expose input/send and return/output pairs. Hosted processor endpoints stay
private; applicable dry inputs, wet outputs and dry MIDI are user-facing.

FX order is Compressor, Drive, three-band EQ, Chorus, Modulation, Reverb. Preserve
type choices, smoothing, independent bypass and disabled-stage DSP suppression.
Synth keeps presets, additive sends, Panic and its existing supported MIDI subset.
Learn observes valid CC before processor filtering; toggles/types are not learn
targets. Global FX control fans out supported continuous controls to track
destinations only; idle processors retain latest values without rendering or
recording global-only traffic. A dual-routed controller remains additive.

## Ownership, timing and protocol work

| Layer | Existing foundation | Required addition |
| --- | --- | --- |
| `rt_buffered` | Exact-Q batching, private inline dummy, source-position release | Public synchronous backend, arbitrary ports, bounded sample-relative controls, live lifetime |
| Optional DSP packages | Retained FX/synth/state in `shoop_engine` | Extracted reusable code; prepared OxiSynth voices/lazy tables; no legacy production dependency |
| Host / worklet | Pre-render preparation and fixed storage | Startup policy, active/candidate banks, slot generations and complete memory accounting |
| RT protocol / engine | Direct strips; stopped/playing/recording; loop media primitives | Processor catalog/state, dry/wet roles/modes, atomic graph/domain publication and alignment |
| Backend / service | Confirmed operations, recovery projection, fenced replacement | Authoritative track processors, controls, MIDI projection and admission |
| Persistence / client | Retained editors/roles/documents; production core codecs | Dry/wet images/media, recorded FX state, truthful UI/Lua and transactional restore |

Audio/MIDI never travels to a separate processor executor. Control events retain
source-frame ordering through batching and cannot mutate completed output.
Render paths, worklet record pumping and slot admission allocate/free nothing.
Processor storage is prepared, activated, retired/reset in bounded pieces, and
destroyed off-render or after callbacks stop. Session candidates reserve their own
bank; failure returns reservations and leaves current audio and media untouched.

Changes to RT/app schemas and worklet configuration must version their wire
contracts together. Generation-fenced reconnect publishes actual state rather
than replaying mutations. UI disconnect does not reset voices or buffered output.
Only implemented facets are advertised; general P09/P10 controls, Carla and bus
FX remain separately refused. P06/P07 remain open until evidence is linked.

## Validation evidence

Baseline on `2e19210b` exposed duplicate test global allocators in the engine;
the test crate now shares one allocator. With that fixture correction,
`cargo test --locked -p rt_buffered -p shoop_rt_engine -p shoop_app_reference --lib`
passed 2 adapter, 22 engine (one ignored), and 134 reference tests.
`cargo build --locked --workspace` passed with `RUSTFLAGS="-D warnings"`; formatting
and the test-attribute audit passed. Commands used `/workspace/.setup/activate.sh`
on the provisioned Debian environment. No production feature is enabled by this
baseline fix. Record implementation revisions and CI links as stages complete. Tests named in the plan are the reference anchors;
production audio/MIDI, public UI/Lua, native carriers, hosted/self-contained
browsers, resource limits, cold allocation guards and transactional recovery must
also pass before declaring the restoration complete.

### Buffered backend milestone

The public `Backend`/`Batch` seam supports independent audio/MIDI shapes, separate
input/output MIDI budgets, bounded source-timed controls, intrinsic latency queries
and drain/abort/activation. Storage generations invalidate partial and completed
batches without scanning audio. Backend failures require explicit recovery.

`cargo test --locked -p rt_buffered -p shoop_rt_engine --lib --tests` passes the
new six backend contract tests and existing adapter/engine tests. The guarded
shape matrix includes zero-port, synth-style, mono, N=8, asymmetric and multi-MIDI
backends over Q=1/16/32/64/128 and M/S=1/0,3/0,3/7.
`cargo check --locked -p rt_buffered --target wasm32-unknown-unknown` and the
production/build dependency audit for native/Wasm pass; the warning-denying
workspace build, tracing inventory and test-attribute audit pass. Wasm execution
passes all 17 adapter tests using wasm-pack 0.15.0, wasm-bindgen 0.2.127 and Node
22.22.2 (`scripts/run_wasm_tests.py --runtime node --profile dev --package rt_buffered`).

### Extracted DSP and real buffered backends

`shoop_dsp` provides independent `builtin-fx` / `oxisynth` features (neither by
its own default). The old engine reexports the moved code and shares the inline
MIDI value type, so reference application/engine fixtures remain executable.
The original DSP tests moved with the implementation and now use allocation AND
deallocation guards. A test-only counter feature retains legacy instrumentation.
The real `FxBackend` and `SynthBackend` implement the portable buffered seam;
source-relative controls and MIDI split rendering at event boundaries. Rack slots
prepare stereo and isolated channel processing for any active width up to their
fixed maximum. Applied values/mappings are readable without allocating snapshots.

Pinned source patches under `third_party/oxisynth*` document archive hashes and
licenses. They reserve/touch voices, eagerly initialize lookup tables, remove a
per-note temporary Vec, suppress dependency logging locally under the realtime
feature, and provide allocation-free clearing of voices, cached output and tails.
Ordinary Panic retains its reference tail semantics. Slot activation also resets
FX smoothers and mappings, avoiding state leakage across owners.

Validation on this milestone:

- 39 moved DSP unit tests and five new real buffered integration tests pass.
  A cold guarded test exercises every embedded preset, increasing polyphony,
  saturation/stealing, Panic and activation reset. FX golden comparisons cover
  every width 1 through 8 in a prepared eight-channel slot. MIDI mapping/state,
  nonaligned synth events, dry-audio independence and unsupported rates are covered.
- The retained reference application (134 tests) and engine (701 tests) pass.
- The Wasm/Node DSP run passes 42 tests before the additional rate-preflight case;
  the final delivery suite will include that case too.
- Neither/FX/synth/both isolated checks compile. `check_host_dsp_boundary.py`
  passes all four feature combinations for Linux, Windows, macOS and Wasm;
  it verifies absence of disabled DSP normal/build dependencies and legacy/app/device
  imports. Engine production/build audits pass for native and Wasm. The full
  warning-denying workspace build, formatting, tracing and test-attribute checks pass.

Host configuration, graph admission and public production capabilities remain
unimplemented at this milestone; no parity row is closed by this library evidence.

### Startup and prepared-bank milestone

Portable startup settings and scalar identifiers live in `shoop_processor_protocol`.
`shoop_host_processors` prepares opaque banks and preset metadata at the actual
host sample rate before native callbacks or worklet rendering. The core has no
concrete DSP dependency. Product native/worklet builds default to both types;
CLI, Host Manager and saved startup settings independently select policies,
capacities and aggregate memory. The worklet configuration is version 4 and RT
protocol version 8 carries effective processor limits and preset records.

Slots begin clean and unclaimed. Claim advances an owner generation; stale
controls, duplicate claims and premature reuse fail. Retirement aborts buffered
release and clears individual prepared rack units on successive cleanup turns.
One slot receives a cleanup turn per quantum across the entire bank, including
when no track is assigned. Final destruction remains quiescent. This does not yet
advertise usable dry/wet tracks: service capability bits stay gated pending the
remaining graph/media/operation implementation.

On the provisioned Linux environment, 48 kHz bank preparation measured requested
heap sizes (excluding allocator overhead). With two FX plus two synth instances
and the full second bank, stereo retained 84,552,968 bytes, with peak 84,561,128
bytes against a conservative 198,649,664-byte reservation. Eight-channel FX plus
stereo synth retained 137,255,928 bytes, peak 137,264,088 against 346,130,240 reserved
(using a 512 MiB configured ceiling). At mono host capacity the stereo synth is
unavailable, and the FX-only bank retained 8,793,456 bytes against 54,451,648 reserved.
The reservation is a bound for DSP/staging/metadata and parsing peak, not a claim
that all reserved bytes are allocated. Tests compare peak and reservation for all
three widths. Disabled startup is heap-guarded and prepares zero instances/assets;
over-budget startup refuses before DSP constructors.

Validation: startup CLI/config round trips, portable build/policy/rate matrix,
processor ownership/reset under allocation/free guards, native and Wasm feature
checks for neither/FX/synth/both, native core/DSP dependency audits, warning-denying
workspace build, tracing and test-attribute audits. The actual both-feature
`shoop_rt_host_worklet.wasm` links and `WebAssembly.Module.imports` returns `[]`.
Full track admission, callback deadline measurements and packaged UI acceptance
remain outstanding; these startup checks do not substitute for those gates.
