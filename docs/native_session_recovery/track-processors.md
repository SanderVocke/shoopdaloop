# Host-side track processor contract and evidence

Implementation branch: `feat/hosted-track-processors`, based on
`feat/fixed-quantum-runtime` at `2e19210b`. Behavioral reference: `master` at
`92079fb6` and the retained reference fixtures. The implementation checklist is
in [the plan](../../PLAN_HOSTED_TRACK_PROCESSORS.md). This document records the
contract and delivered implementation. Public native production routes are verified
locally; the final delivered-revision platform/browser CI gates remain pending.
PR [#946](https://github.com/SanderVocke/shoopdaloop/pull/946) targets
`feat/backend-replacement`, reconciled through `40db766d`; the original base above
is retained as provenance. Current wire versions are engine v21, application v29
and worklet configuration v6.

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

FX supports 8,000–384,000 Hz. The pinned OxiSynth supports
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

| Layer | Starting foundation | Delivered addition |
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
FX remain separately refused. P06 closes under the qualified user-directed closure
below; P07 remains open for Carla.

## Validation evidence

### User-directed closure and remaining CI

On 2026-11-05 the user instructed: “preempt that. go ahead and close the plan now.”
The hosted-processor plan and P06 are closed under that delivery decision, without
waiting for the entire platform matrix. This supersedes the earlier CI-wait gate;
it does not turn failed or unfinished checks into passing evidence. P07 remains
OPEN for Carla, P08 for bus FX and P17 for dependent composite modes. PR #946
remains open and unmerged against `feat/backend-replacement`.

Implementation source `8a1be130` has a clear automated review, all 3,006 native
coverage tests pass (nine skipped), both Codecov checks pass, and all four isolated
DSP/startup jobs, real Node DSP, Miri, shared-media, bare-metal and platform process
inspection checks pass. Targeted production/native/browser evidence is below.

At closure inspection, the independent runtime-host fixture build fails under
`-D warnings` because `measure_memory` is unused, after the application tests pass:

- [Linux debug](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251128/job/127289414110).
- [Linux release](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251128/job/127289413562): all 2,032 application tests pass (four skipped).
- [Windows debug](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251128/job/127289413797): all 2,022 application tests pass (four skipped).

The remaining platform/browser/firmware jobs are unfinished at closure. These CI
failures and results remain PR follow-up; no fully green matrix is claimed. The
closure commit changes Markdown only. The saturated-synth deadline limitation in
the measurement evidence also remains explicit.

### Integrated validation at `8a1be130`

Candidate: `8a1be13098880c22ddae0d3e85d80bc684c41ee2`, reconciled against
`feat/backend-replacement` at `40db766d95b62eeb5de5c830e012f422fc9517ca` in
[PR #946](https://github.com/SanderVocke/shoopdaloop/pull/946). Engine/application/
worklet configuration versions are 21/29/6. Automated review reports no major
issues for this candidate
([receipt](https://github.com/SanderVocke/shoopdaloop/pull/946#issuecomment-6659834542));
the applicable owner-admission finding is fixed and resolved. The following
CI gates below contain passing, failed and unfinished jobs. The qualified
user-directed closure above replaces waiting for their completion.

Current-head native coverage passes all **3,006 tests / nine skipped**
([job](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251128/job/127289413940)).

| Gate | Evidence |
| --- | --- |
| Native Linux/macOS/Windows debug/release, coverage, packaged carriers, Node/Chrome Wasm, hosted/self-contained Chromium, Firefox, JACK, bare-metal and policy checks | [Build and Test 42040251128](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251128) |
| Independent neither/FX/synth/both builds and startup, excluded assets/imports, real Node DSP, selected Miri, startup/memory/callback measurements | [Hosted processors 42040251127](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251127) |
| Firmware, board profiles, USB and network deployment | [Firmware 42040251101](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251101) |
| Static analysis and documentation | [CodeQL 42040251118](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251118), [Docs 42040251070](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251070) |

Focused local reconciliation validation uses real DSP, guarded engine calls,
public service archives and packaged native/browser paths. Counts and the exact
admission, latency and profiling regressions appear below. Broad suites and the
platform matrix run in CI rather than being repeated locally. Historical entries
retain their original source pins and superseded intermediate status.

The [current measurement artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/42040251127/artifacts/13420204031)
records PR merge checkout `55e593d9851723343d54e6b9d2edaade2ce8a012`. Its tree
`becf6da67a33c62e89632d92f67676347e5fb246` equals the candidate's tree. The Ubuntu
runner exposes four logical CPUs on Intel Xeon Platinum 8573C under Microsoft
virtualization, using rustc 1.99.1 / LLVM 23.1.1. At 48 kHz/Q128, M=1/S=0,
256 callbacks and 1,024
equal-offset MIDI events per callback, eight-channel/all-stage FX observes
728/872 microseconds mean/peak; stereo/256-voice synth observes 2,842/2,945
microseconds. The block period is 2,666.67 microseconds: this run again provides
no saturated-synth deadline margin. Shared-VM wall-clock observations and the
browser smoke's zero-xrun workload do not establish a maximum-load guarantee.
Earlier higher peaks remain documented below.

Widths 1/2/8 prepare in 20/250/337 ms. Requested allocation peaks are
8,799,344/84,578,472/137,316,376 bytes, below complete reservations of
54,532,576/198,793,088/346,273,664 bytes; retained storage is
8,799,344/84,570,312/137,308,216 bytes. Hardware xruns, distributed event offsets
and simultaneously saturated instances are not measured by this synthetic fixture.
Selected Miri covers portable ownership/lifecycle logic, not third-party DSP.

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

### Loop/media foundation milestone

Loop bindings expose independent primary and auxiliary audio capacities; each
source retains its own snapshot format and channels. Dynamic node-pair activation
carries dry/wet roles and auxiliary media with the existing owner generation cut.
The resolver owns `PlayingDryThroughWet` and `RecordingDryIntoWet`, so transitions,
scheduling, cancellation and wrap counting use one logical loop clock. RT protocol
version 9 adds the corresponding mode tags; production creation remains gated.

Ordinary dry/wet capture records wet, dry and dry MIDI as separate sources.
Regular playback selects wet only. Dry playback selects dry audio/MIDI only. Wet
rerecord forks a writable COW wet source while independently retaining the dry
leases. Repeated wraps edit wet positions without changing take length, dry
samples or the dry media resource/revision. Capacity failure refuses the transition
and leaves the former media and playback running. The edit reserves a snapshot
metadata credit at admission; unrelated snapshots cannot consume that credit.
Aliased incoming leases are rejected before media ownership changes.

Native primitive tests exercise Q=1/16/32/64/128, widths 1/2/8, independent 1+8,
8+1, 2+5 and 5+2 sides, complete sample comparisons, MIDI suppression on ordinary
wet playback, wrapping rerecord, repeated capacity failures and canceled scheduled
rerecord without a second wrap count. All render and prepared submission paths in
these fixtures count allocations and deallocations. The loop-library Wasm Node
suite passed 185 tests before the last unequal-width fixture was added; its final
rerun is still required. Portable runtime tests cover reserved-snapshot saturation
and stale reservation generations. Runtime and loop dependency audits pass for
Linux, Windows, macOS and Wasm. Production processor routing, latency alignment,
public operations and persistence remain to be connected and validated.

### Immediate sleep/wake reset prerequisite

FX delay lines now invalidate history with a valid-write count. A documented,
pinned FunDSP 0.23.0 patch enables the same behavior for its fixed and tapped
delays under `realtime-reset`; the host DSP package opts in. Scalar and block
processing retain zero-history semantics, including zero-delay and fractional
taps. Preparation touches delay storage before activation. The patch also clears
the plate reverb's prediffusers, omitted by its upstream reset. Ordinary FunDSP
builds retain their original reset path. Licenses and archive provenance are in
`third_party/fundsp`.

`Backend::suspend` clears stream history while retaining applied settings and
mappings. Hosted owners can suspend and immediately restart under the same owner
generation; retirement still clears defaults incrementally before reuse. Tests
compare resumed FX and synth output exactly with fresh instances using the same
controls, count allocations/frees, and reject stale generations. Synth comparison
uses its fresh-instance quiet output, including the reference reverb's tiny DC
noise floor when sends are enabled.

Native tests verify independent impulse expectations for fixed/cubic/linear
fractional delays, scalar/block processing across resets and wraps, all three
reverbs at 8/48/384 kHz, immediate wake, retained controls, and cleared voices and
queued output. The DSP Wasm Node suite passed 47 tests; the linked both-feature
worklet still had zero imports. These Wasm checks preceded the final setup-only
storage-touch addition; the final full gates must include that revision.

The loop foundation now publishes settled per-source demand, including recording
and rerecording in the reference countdown window (a raw skip count of one). Lookahead is cached per loop at the
same preview/commit boundary as playback/capture decisions, so consumers do not
rescan pending transitions for every processor. Cancellation clears lookahead;
wrap counting remains resolver-owned. Unaligned dry-play offsets refuse before
state changes, and self-synchronized dry modes retain delayed transition behavior.
Native loop/engine regressions and the warning-denying workspace build pass for
these additions. Reference routing uses imminent recording/rerecording demand;
monitoring behavior must follow that truth table when the production nodes connect.


### Hosted production graph milestone

RT protocol v10 adds processed-strip creation and generation-owned processor
records. Admission enforces advertised type counts and shapes; configuration and
binding complete before graph activation. Routing-only changes retain the owner.
Removed owners retire after their graph cut; failed preparation returns claims.
All prepared bank nodes remain in the graph for bounded cleanup while unassigned.

The execution order is loop playback, strip input, hosted processors, captures,
loop faders and strip outputs. Dry playback feeds processor inputs; ordinary wet
playback bypasses processors. A read-only source-tagged completed-batch tap supplies
wet capture at the same source frame as raw dry audio/MIDI. Scheduled wet release
retains the buffered Q delay. Ordinary wet playback has the same pure Q delay;
the host bridge adds Q once (zero bridge delay for Q=1). Sleep releases the pending
batch before invalidating DSP state; immediate wake uses a fresh stream origin.
Applied controls are retained when activity ends on their submission boundary.
Processor faults silence their own outputs rather than faulting unrelated graph
nodes. Remaining public fault projection and recovery tests are still required.

Prepared primary/auxiliary widths remain independent. Logical widths govern loop
stereo balance even in a wider prepared host. Rerecord uses prepared COW storage;
when no input-history store exists, the engine reserves an edit-only store from
one quarter of the existing media arena. It retains no retrospective input and
performs no history copy. Snapshot metadata credits and dry leases stay separate.

Native real-bank tests use 8 kHz, Q=16, up to eight dry plus eight wet channels,
and guard render and protocol pumps against allocations and frees. They cover
monitoring, ordinary capture/wet playback, dry-through-wet, wrapping wet rerecord,
at-limit admission, and synth sleep/removal cleanup. Separate impulses verify
Q=1/16/32/64 through irregular callbacks and exact node-plus-host delay. A real
OxiSynth note pair at offsets 5 and 87 matches unbuffered backend samples exactly
after the declared delay. Its reverb's existing tiny DC noise is retained in the
reference comparison. Full native runtime/adapter/loop/core/protocol suites pass,
as do workspace all-target checks, warning-denying build, tracing/test policy and
all-target dependency/feature isolation audits. Later changes repeat their affected
gates; packaged browser/native, full public/persistence and platform gates remain.

External routing and public service/editor/persistence admission remain unfinished.
The capability flags stay disabled until the complete production path is usable.

The final milestone fixtures also exercise enabled drive gain to distinguish
wet playback from double processing, synth wet capture and dry MIDI replay,
ordinary MIDI-overflow voice cleanup, live-plus-playback MIDI offset ordering,
rerecord prerouting/cancellation, isolated invalid DSP output, and stereo loop
balance inside an eight-channel host. Supported low-rate racks clamp fixed EQ,
reverb tone and reverb feedback filter cutoffs below Nyquist; the 48 kHz recipes
retain their original values. All enabled stages remain finite at the tested
8/11.025/16/22.05/48 kHz rates and through reverb/modulation type changes.
Wasm Node passes 48 DSP tests, 188 loop tests and 14 host-factory/production-graph
tests. Native real-bank graph tests pass all eleven scenarios. These remain
library/production-graph evidence; public UI, External publication, full media and
persistence/recovery, packaged browser and cross-platform CI gates are still open.


### External core routing milestone

Protocol v11 adds explicit audio send, audio return and MIDI send port kinds,
retaining existing tags. Send and return connection banks fade independently of
dry input. External capture selects raw return plus original dry audio/MIDI;
regular wet playback bypasses sends, dry playback routes to sends, and wet
rerecord preserves dry media. External node delay is zero; the host bridge still
adds its own Q once. Physical peer/driver latency is not a new automatic
measurement feature here.

`EngineDescription.external_dry_wet` prepares the generic topology and COW edit
storage without FX/synth instances. Minimal library descriptions default it off;
native product and browser startup set it on. Worklet configuration v5 carries a
strict boolean. The configured media arena supplies the edit reservation; no
extra decoded synth or rack resources are constructed. Note and sustain release
covers live-route deactivation and recovers from cleanup overflow once.

Native fixtures cover 1+8, 8+1, 8+8, 2+5 and 5+2, wet/dry/rerecord results, at-limit
role separation, independent return crossfade, minimal admission refusal and MIDI
cleanup saturation. All render/request paths are heap-guarded. Full core/protocol/
worklet native tests and all-target checks pass, as does a warning-denying workspace
build. Wasm Node passes 74 core and 18 worklet tests. Native JACK publication and
service/UI mapping are still pending; these fixtures are not platform acceptance.

### Adapter role and dry-playback integration milestone

The production adapter retains processed topologies and generation-owned slots
through admission, confirmed snapshots and resynchronization. External tracks
publish independent dry input/send, wet return/output and optional dry MIDI/send
roles. Hosted tracks publish dry inputs, wet outputs and dry MIDI only; processor
endpoints remain internal. Per-type limits count all admitted channel widths.
The existing native dynamic-port publisher consumes these ordinary descriptors;
real-driver publication acceptance remains open.

Dry audio playback now traverses the loop presence fader before the processor
send. Removing one dry-playing loop fades only that loop while other loops keep
sounding, and the meter observes the selected dry role. Hosted regular wet
playback retains its pure Q delay. Preparing node configuration concurrently with
graph staging keeps configuration acknowledged before activation and preserves
the existing Direct three-poll routing and six-poll creation bounds.

Rerecord/prerouting monitoring changes update the authoritative model after
execution. Reliable unsolicited control records retry when the outbox is full,
use the current strip generation and follow pending control outcomes. Reconnect
retains the forced-off value, and subsequent explicit changes can enable it.

Native validation: 131 tests across the core, adapter and real host banks pass,
including role/connection round trips, cross-width type limits, dry removal fade,
dry-role meters, monitoring/resync and existing latency regressions. The workspace
build passes with warnings denied; test-attribute and four-target production
boundary audits pass. Public feature bits remain gated while controls, media and
persistence work continues.

### Host-confirmed processor control milestone

Protocol v12 carries a strip's default wet/dry playback choice independently of
loop mode. It also carries bounded parameter, activation and atomic full-state
requests plus reliable applied-state snapshots. Snapshots include the processor
slot generation, phase, scalar/mapping values and their exclusive source-frame
end. Projection retries under backpressure and compares float bits. Resync waits
for a coherent atomic read rather than completing with a missing processor record.

The production adapter converts these records into the existing FX/synth editors
without importing DSP code. Real controls cover stages, types, continuous values,
synth host-advertised presets, sends, Panic and assign/remove/clear mappings.
Visibility remains transient. Activation aborts stale sound on deactivation;
automatic routing changes regain control of processing demand. Global MIDI and
idle mapped-control handling still require the next integration milestone.

Full-state application preflights all affected values, actual width, mapping
uniqueness and the complete pending-event capacity before mutation. Envelope-only
restore preserves live mappings. A successful restore resets processor histories
and initializes restored FX smoothing targets at its source boundary. The node
and host delays remain separate and unchanged. Canonical built-in envelopes now
share a portable encoder with the concrete DSP implementation; legacy FX v1 and
current FX/synth v2 remain supported. Concrete synth admission still validates the
selected preset against the prepared host resources.

Native evidence: 194 regression tests across DSP, processor contracts, wire
protocol, host banks, core and adapter pass; an additional real-graph fixture
proves the restored gain changes exactly after the declared two-Q combined delay,
with no smoothing ramp or early samples. Compatibility checks cover every FX type
combination, canonical float bits and every packaged synth preset. Public backend
control tests prove changed audio, mapped-MIDI projection and resync, activation,
invalid-state preservation, synth sends/presets/Panic and default playback state.
The warning-denying workspace build, four-target production boundaries and the
429-module tracing inventory pass. Service/UI command wiring, global controls,
recorded take state, media and transactional persistence remain open; public
capability bits stay gated.

### Global and idle MIDI milestone

Protocol v13 adds the global control input to the host connection model and
resynchronization. Its graph edges reach processed track destinations only.
Hosted processor endpoints stay internal. A fixed cache retains the latest value
for 1,952 supported control keys: CC 0–119, channel pressure and pitch wheel on
sixteen channels. Notes, programs and channel-mode controls are excluded from the
global path. Deferred values precede ordinary and current global traffic at equal
offsets. Ordinary and current traffic reserve capacity before deferred values;
overflow retains latest values for later bounded delivery.

Inactive dry MIDI updates explicit mappings without DSP work or voice creation.
Global controls remain deferred until processing activates. Source time and stable
equal-time order survive the merge, including additive connections to both lanes.
Global traffic never enters dry MIDI capture. Global input recovery leaves ordinary
voices running; ordinary recovery retains its existing cleanup. External MIDI
note/sustain release still reaches the peer after monitoring stops. Panic retains
the reference synth's effect tails; sleep, state restore and retirement clear them.

Prepared processor reservations include the additional buffers and control cache.
Coherent snapshots include rejected, overwritten, deferred, drained and pending
control counts. Editors can explain retained controls while a processor sleeps.
Visibility and activation do not depend on a client polling the backend.

Activation policy refinement: the retained native and legacy backend polls apply
routing again on every UI observation. Reproducing that would let UI polling alter
explicit activation and violate host ownership and disconnect continuity. The RT
host instead retains explicit activation until the source's routing demand changes;
monitoring and current/imminent record or dry-playback changes restore automatic
routing. Pure observation and global MIDI traffic never activate audio DSP.

Native: 169 targeted runtime, processor, protocol, core and adapter tests pass,
including existing latency checks; the workspace build passes with warnings denied.
Wasm Node: 18 host-bank/graph, 78 core, 45 adapter, 18 buffered-adapter, six wire and
two processor-contract tests pass. Tests prove changed FX audio, inactive mapped
controls, exact source ordering, synth voice continuity through global overflow,
External dual routing, capture exclusion and connection resync. Four-target
production dependency audits, test policy and the 431-module tracing inventory pass.
Service/UI, MIDI Learn, recorded-take state and media/persistence are still pending;
this evidence does not enable public feature bits or close those acceptance rows.

### Service actions, Learn and recorded-take source boundaries

The production service now forwards host-advertised creation and typed editor
controls, projects topology/default/editor state, and resolves default UI/Lua
playback independently per track. Queued default changes retain private ordered
intent until acknowledgement; public state remains confirmed. Protocol v14 added
a bounded latest dry-input MIDI value for the existing Learn widgets. Reuse clears
that value; global control-only traffic does not become dry-input observation.

Protocol v15 adds recorded processor settings identified by loop generation and
content revision. The portable buffered backend supports optional source observers:
parameters and MIDI at an observation offset apply first, settings are observed,
then the sample renders. Later MIDI in the quantum is excluded. Applied settings
are retained in two fixed per-loop banks for the committed and candidate takes.
A failed/cancelled candidate cannot replace the old recorded settings. Startup
reservation includes this storage. Public state carries the canonical envelope;
restoration keeps live mappings and uses the existing atomic restore preflight.

A faulted processor now aborts dependent captures before further staging. Active
capture cancellation returns reserved final leases without publishing a snapshot;
the planner keeps the prior media revision. A real corrupted-output fixture proves
that failed rerecording preserves old state/media, wet playback bypasses the faulty
processor, and an unrelated Direct track keeps rendering. Resource tests repeat
abort/reuse sixteen times under heap guards and recover all pages, versions and
leases.

Native: 678 tests across the runtime, buffered adapter, loops, DSP, host processors,
protocol, core, backend and service pass in `work/recorded-state-native.log`. Two
additional actual service-wire fixtures exercise FX N=3 and synth: audible output
to an ordinary bus, editor state, presets/sends, MIDI Learn, live mapping retention
through recorded-state restoration, invalid controls, activation, detach/reconnect
during an accepted edit, and removal. Their render calls are heap-guarded.
An External service regression confirms that initial bus routes include wet outputs
and exclude dry sends. As with other foundation fixtures, the service fixtures
explicitly expose gated protocol capability bits; they are not final public
admission or packaged-platform evidence. Core Wasm checks pass for buffered 19,
adapter 45, core 79 and protocol 6 tests. Both-DSP Wasm passes all 22 host
processor tests, including the two actual service-wire fixtures. The warning-denying
workspace build and all 242 native editor/widget tests pass. Media/persistence and final delivery remain
open; no stage is closed on these fixtures alone.

### One-revision dry/wet media reads

RT protocol v16 adds `BulkSelector::AudioRoles`: each frame contains the prepared
dry bank followed by the prepared wet bank. The adapter selects independent
logical widths without reducing the eight-channel limit on either side. Canonical
audio order remains dry channels followed by wet channels, matching the reference.
Both cursors retain the same immutable export, so a clear or replacement cannot
mix audio from different revisions within a read. The reader prepares its scratch
space, handles partial frames across callbacks/copy budgets, and releases the
export after streaming stops. This extends the existing media path; processor DSP
continues to run inside the host graph.

A heap-guarded core fixture records eight dry plus eight wet channels, begins a
chunked read, clears the loop after its first chunk, and verifies the old samples
on both sides through completion. It covers 32-byte and 512-byte pages. An adapter
fixture selects three dry and one wet channel from eight-channel prepared banks.
Real FX and synth service-wire fixtures validate ordered dry/wet samples from the
public media path. The service counts both sides against its media-view budget
and refuses reads during active rerecording.

Native core/protocol/backend/service regressions pass 169 tests; the two additional
real-DSP service fixtures pass. The warning-denying workspace build passes. Wasm
passes backend 46, core 80 and protocol 6 tests. All 22 both-DSP host Wasm tests,
including the service media reads, pass. Writes and session replacement/persistence remain pending, so protocol
capability admission remains gated.

### External zero sides and continuous dry MIDI

The production graph now covers External 0/8 audio, 8/0 audio and MIDI-only shapes
through record, regular wet play, dry play and wet rerecord. Default dry playback
requires a wet side; explicit dry playback remains available for the other valid
shapes. Dry media survives rerecording in each case.

A wet-take commit retains the immutable dry MIDI prefix under another lease.
Returning directly to dry playback must hand the cached cursor to that equivalent
prefix, including a prefetched next event. The renderer checks resource/revision,
description, capture cut and capture metadata, then resumes at the previous record
boundary. It retains its live state and emits no cleanup, restored controls or
duplicate note-ons. Different content still requires a discontinuity. A focused
heap-guarded fixture releases the original lease while a note is held and verifies
that the retained prefix emits exactly its one pending note-off, then reclaims all
resources. The zero-side graph fixtures also exercise this path through rerecord.

Loop meters now project the selected role's logical width rather than the prepared
capacity or the other role's width. Reading a narrower meter discards unreported
lanes, preventing stale peaks after a later role switch. Stereo-balance controls
keep the reference's wet-side width policy; non-stereo input/output/loop meters
show the loudest logical channel. An unequal 2-dry/1-wet fixture checks dry, wet and
rerecord meter values. Real FX N=3 service fixtures use different amplitudes on
each input and verify the non-stereo projection and ordered dry/wet samples.

Recording-start detection runs once per processor block; control/MIDI segment
observations reuse that result. Native regressions pass 610 tests, plus all 22
host-processor tests after this refinement. The warning-denying workspace build
and test/tracing inventories pass. Wasm passes the retained-prefix fixture,
core 83, backend 46 and both-DSP host 22 tests. Public capability admission remains
gated pending complete writes and session persistence/replacement.

### Role-aware writes and prepared replacement

Protocol v17 carries separate dry and wet upload tokens and optional recorded
processor settings. The adapter keeps canonical audio in dry-first, wet-second
order, normalizes each role into its own prepared channel bank, and includes both
banks in the finite media budget. Selected channel updates preserve the opposite
role, MIDI and the recorded settings. The core adopts and consumes all three
sources; the successful selection publishes settings against its confirmed loop
generation and content revision before acknowledging the update.

Full replacement prepares processed strips in inactive reserved DSP banks. Width,
settings, preset and mappings are validated and applied before the pending graph
is activated. Loaded take settings remain in the fenced node configuration until
the new graph runs. The adapter resynchronizes the committed owners before the
load completes, including processor and take metadata that may have been
announced before the candidate mirror became public. A subsequent load waits
asynchronously for a displaced same-type bank to finish bounded retirement.

The real adapter/host fixture covers External 3→1, FX 3→3 and synth 2→2 imports,
selected dry and wet edits, runtime reexports after discarding canonical caches,
recorded settings, six repeated loads, cancellation after claiming the candidate
DSP bank, and failed synth preset preparation followed by a successful load.
Rendering is heap guarded. Service images, document conversion and public session
admission remain gated until their separate persistence work is complete.

The required native workspace gate passes 2,819 tests, with seven existing skips.
The warning-denying workspace build, tracing/test inventories and four-target
boundary audits pass. Node/Wasm passes backend 47, core 83, protocol 6 and the
both-DSP host package's 22 tests. Post-commit resynchronization retains canonical
dB controls when their amplitude matches exactly, preventing save/load rounding
drift while still adopting changed engine controls.

### Backend replacement reconciliation

PR #946 now targets `feat/backend-replacement` at the user's request. The combined
wire uses engine protocol v18 and application protocol v25, retaining the target's
profiling requests, graph inspection and hot-path optimizations. Hosted strip
creation has its own request tag; profiling and processor capabilities have
separate bits. The fader keeps the target's steady-gain fast path while preserving
the hosted wet delay and independent dry-role processing. The target's host-manager
and updated testing mandates are retained.

Focused native checks pass: protocol round trips, real-host media/replacement,
public admission gating, profiling topology, and real FX monitoring/capture/both
playback paths across all widths. Warning-denying builds of the affected host,
worklet, app and Host Manager pass; tracing and test-attribute inventories pass.
Broader platform/regression validation is delegated to PR CI under the updated plan.

### Service images and session archives

Application protocol v26 adds portable channel roles, processed topology, default
playback, canonical applied processor settings, separate mappings and recorded-take
settings to persistence images. Service capture freezes settings at the accepted
cut while subsequent live parameter changes continue. The document codec restores
the scoped FX, synth and External representations, including dry/send and
wet/return port intent; unsupported Carla, bus processors and general latency
overrides retain explicit refusals.

Real service/archive save/load/save fixtures pass for FX N=3, synth and External
3→1. They compare role media, MIDI state, defaults and settings after fresh owner
creation, and restore imported take settings while keeping live mappings. The
codec preserves selected export order and role numbering and supports sixteen
logical import destinations. File dialogs and waveform labels follow those roles.
The affected native product builds with warnings denied; codec/image tests and
the three real service fixtures pass. Synchronized family imports, automatic
alignment projection and final public capability admission remain unfinished.

The first reconciled PR CI run also exposed outdated profiling/worklet contract
fixtures and small native stacks. Runtime ownership now uses prepared heap storage;
indexed MIDI validation avoids materializing unused full channel state. The
1 MiB startup/cleanup regression, nine MIDI-index tests and actual JACK replacement
across in-process/socket/shared-memory/TCP pass locally. The next CI run supplies
the repeated Windows/macOS/browser artifact evidence.

Engine protocol v19 extends synchronized family imports with independent dry/wet
uploads and recorded settings. Metadata is validated before staging and published
against the selected media revision before completion. Codec family conversion
retains channel roles and gains. The real host fixture checks changed and restored
family media for External, FX and synth, bypassing canonical caches for runtime
readback; recorded settings and MIDI survive. Codec/protocol and existing family
regressions pass locally.

The target advanced again to `e1f28c87`, adding P12 timing parity and the preliminary
FreeRTOS host. Reconciliation retains those contracts and uses engine protocol v20
and application protocol v27. Timing records and capability bits remain distinct
from processor records. Bounded playback windows now hold both dry/wet roles in
canonical order, while the per-side host ceiling stays eight. The combined native
product builds; focused protocol, P12 timing, processed media/replacement and all
three service archive fixtures pass. Actual JACK replacement on all four carriers,
1 MiB startup/cleanup and the service default-mode regression pass too. Per-channel
MIDI export reads one initial channel without materializing all sixteen states.
Tracing/test inventories and boundary checks pass. Remaining processed timing
interactions stay part of the unfinished admission work.

Capture candidates now retain their own windows until publication. A wet rerecord
keeps the original dry audio/MIDI metadata and resets the wet capture alignment;
live offset edits update candidate metadata too. Generation-fenced render snapshots
provide exact committed raw lengths/windows to the endpoint, adapter and reconnect
path. Media reads wait for those records. The new real-FX imported-take fixture
compares dry media, MIDI, offsets, preplay and raw lengths after rerecord/reconnect;
the unequal-role loop fixture and existing P12 timing/JACK regressions pass.

Application protocol v28 adds authoritative hosted resource descriptors: instance
limits/usage, buffering frames and reserved/budgeted host bytes. The public
add-track dialog displays usage, and service snapshots carry automatic processor
advance and loop alignment. Both real DSP service fixtures assert capacity changes
and the 16-frame test delay. Focused dialog tests, warning-denying native product
builds and tracing/test-attribute checks pass.

Wet rerecord writes each wet lane at its own start offset using bounded COW
channel edits. Negative positions are skipped and untouched raw tails remain.
Admission and live timing edits refuse writes beyond the retained raw extent
before mutating the take. The independent three-channel-offset regression runs
under callback heap guards; maximum-width rerecord and real service fixtures pass.

Public admission now derives dry/wet, External, processor and global-control
capabilities from the actual prepared host. Service and adapter fixtures no longer
override advertisement. Real wire/archive fixtures, default actions, imported take
timing/reconnect and minimal-host refusal pass. The startup-policy test checks all
four combinations against actual host build support and handshake capabilities.

### Packaged acceptance and CI coverage

The native application/companion probe passes socket and shared-memory delivery
with mono/N=3 FX, synth and External tracks in one session. It uses the normal
application intent/file workflows and client Lua VM, checks real DSP output,
default versus explicit playback, editor state, role imports, save/reload and
socket reattachment. It also passes with both DSP types Off and External retained.
On this Linux development host, readiness was 523 ms with both Off and about
750 ms with both Auto (debug, dummy 8 kHz/128-frame callbacks, Q=32, three physical
channels, five tracks, forty loops). These are observed preparation/readiness
times for this configuration, not a general timing bound.

Real service fixtures additionally verify structure-only and full archive
round trips for FX N=3, synth and External 3→1. The real-FX pickup fixture uses
49 source frames of lead-in with the declared 32-frame combined node/host delay
at Q=16 and irregular callbacks. Callback heap guards remain active.

`Hosted track processors` CI supplies four independent native/worklet feature
builds, four startup-policy combinations in each build, both-DSP Node tests,
prepared memory/startup observations and release callback measurements at eight
FX channels or 256 synth voices with 1,024 MIDI events per callback. Measurements
record hardware, configuration and source revision. Simulated callbacks report
no hardware xrun count and provide no deadline guarantee. Selected Miri checks
cover portable host ownership and buffered reset identity; they do not certify
third-party DSP. Existing archive and browser jobs exercise the new public probes
on their supported platforms. Delivered-revision results still need recording.

Real service failure/rate checks now cover malformed processor state, startup-
disabled types, successful subsequent load, New Session instance release, and
8→16 kHz conversion. Conversion is first reported as required, then applied
before loading into newly prepared host banks. Settings, roles and recorded-state
metadata survive, frame lengths scale, loaded loops stop, and dry playback yields
real DSP audio. All four focused service-wire fixtures pass. Portable ownership
Miri passed at `159771c7` in [the hosted CI run](https://github.com/SanderVocke/shoopdaloop/actions/runs/41991644691).
The target branch's existing `shoop_profiling.now` is the sole permitted worklet
import; optional DSP introduces no additional imports.

Take alignment is derived from committed wet/dry channel windows, using the same
wet-minus-dry interpretation as the reference. It is independent of the current
processor's buffering. New source-aligned takes therefore report zero; an imported
five-frame difference is retained until a new wet take is published. Dry-only
External takes project their audio alignment. Real rate-conversion fixtures now
also change Q from 16 to 32: stored take alignment stays zero and current processor
latency reports 32, with audible playback on the newly prepared host.

Automatic playback after a fixed-cycle recording now follows the owning track's
default, including dry-through-wet. The wire regression covers both defaults,
autoplay disabled and explicit wet Play. This corrects the P2 automated review
finding on `67b2565b` without changing explicit playback requests.

A focused actual JACK companion test now covers External 2→1, stereo FX and
OxiSynth through socket delivery. A separate JACK peer verifies both External
audio sends, MIDI send and audible wet outputs. Public port roles are published;
hosted internal endpoints stay private. Dry capture remains sample-exact, wet
capture is audible, MIDI notes survive and audio continues across reconnect.
All three types pass in one 2.43-second native run. The existing Direct suite
continues to cover in-process/socket/shared-memory/TCP delivery. Automated review
of `2e035504` completed without findings after the post-record fix.

The `a068ce72` target update is merged, including Zynq board profiles and the
512 MiB ZedBoard budgets. Embedded DSP remains explicitly disabled. Live loop
timing now consumes reliable records outside resync, preserves imported raw
lengths/windows and rejects stale telemetry. Capture waits for matching timing;
a reliable content change also invalidates cached media. The real rerecord
fixture checks retained dry/MIDI and wet raw tails before reconnect, then checks
adversarial timing/telemetry ordering. CI fixture updates cover advertised FX
capabilities, channel roles and waiting for a committed firmware session snapshot.


The isolated both-DSP Node job's diagnostic log identified missing `protoc` before
any tests ran. Its workflow now installs `protobuf-compiler`. Targeted local
reconciliation passes nine real adapter/DSP tests, both network tests, the native
application workflow, the processed JACK fixture and four portable board-profile
contracts. Changed native products and the no-default Zynq client build with
warnings denied; test/tracing inventories pass (457 modules). Firmware execution,
Wasm and cross-platform matrices remain CI gates on the next delivered revision.

### CI startup, memory and callback measurements

The both-DSP job passes at candidate `fe714c4c` in
[run 42003214102](https://github.com/SanderVocke/shoopdaloop/actions/runs/42003214102/job/127163405535).
Its measurement artifact is `hosted-processor-measurements-42003214102`
([artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/42003214102/artifacts/13403998708)).
The recorded checkout is PR merge `db52310c9638f1b5f62151b5de14bec01c9ab277`;
its Git tree matches `fe714c4c2ce9efa7e60a7850a8c23abf1c64f202` exactly.
The Ubuntu x86_64 runner exposes four logical CPUs on an AMD EPYC 9V74 under
Microsoft virtualization; rustc is 1.99.1, LLVM 23.1.1.

Preparation uses the default active/candidate banks and a 512 MiB configured
ceiling. Mono cannot host the required stereo synth. Peak requested allocations
stay below the complete declared DSP reservation:

| Host audio width | Peak requested bytes | Retained bytes | Reserved bytes | Preparation ms |
| --- | ---: | ---: | ---: | ---: |
| 1 | 8,799,344 | 8,799,344 | 54,532,576 | 24 |
| 2 | 84,578,472 | 84,570,312 | 198,793,088 | 293 |
| 8 | 137,316,376 | 137,308,216 | 346,273,664 | 389 |

Release callback observations use 48 kHz, Q=128, M=1/S=0, 256 callbacks and
1,024 equal-offset MIDI events per callback. FX has eight channels and all six
stages enabled. Synth has two output channels and saturates its 256-voice limit.

| Processor | Mean microseconds | Peak microseconds | Block period microseconds |
| --- | ---: | ---: | ---: |
| Built-in FX | 752 | 793 | 2,666.67 |
| OxiSynth | 2,501 | 2,675 | 2,666.67 |

These are wall-clock observations on a shared CI VM, not execution-time bounds.
At maximum voice load, this synth run exceeds the block period and demonstrates
no deadline margin. Simulated callbacks have no hardware xrun measurement;
distributed event offsets and simultaneously saturated processor instances are
not measured by this fixture. Allocation/lifetime guards and finite work bounds
still apply, but supported shapes and memory admission do not certify that every
CPU/configuration can sustain maximum polyphony in real time. Synchronous slack
adds no execution budget. Third-party DSP is outside the selected Miri claim.


Tracing-enabled CI exposed legacy native SDK spans inside buffered DSP calls.
The prepared backends now enter the processing body directly; the owning runtime
profiler observes host nodes with its bounded RT records. Legacy/reference entry
points retain their existing spans. The allocator guards remain enabled with
native detailed tracing: all 32 buffered-DSP, graph and public service cases pass.
All 15 adapter integration tests also pass after `wait_idle` waits for matching
committed timing on settled loops. Changed DSP/host/adapter products build with
warnings denied. This reconciles the coverage job without disabling tracing or
weakening allocation assertions.


Further CI reconciliation at `5b2c0460`: all four isolated DSP builds/startup
policies, both-DSP Node Wasm, selected Miri, real JACK acceptance, the shared-media
foundation, Cortex-A no_std and all three host-process inspection jobs pass.
The remaining failures identify several boundary/fixture gaps. The browser codec
worker now permits the same sixteen audio destinations as the portable codec;
its focused test covers inspection/preparation, seventeen-channel refusal and
complete streamed samples. DSP unit allocation guards enter the same processing
body as the buffered backend, retaining native detailed tracing and all guards.
Independent primitive host fixtures include the new binding/media fields and
observe the added modes/transitions without changing their Direct replay oracles.
The 512 KiB callback-stack fixture boxes the prepared runtime before thread entry.

The firmware packet trace shows a native heartbeat expiring during the large
commit while the control endpoint is busy. Backend pumps now yield after eight
rounds and cap received messages per round; a busy-producer regression verifies
that queued telemetry remains while real track controls still confirm. Real
processor service and adapter tests pass with the smaller slices. Retained-heap
sampling moves after data-peer detachment, preserving exact byte equality and
requiring zero remaining data sockets. USB output capture stays active until
firmware shutdown returns, preserving the complete reset-suffix assertion.
Cross-platform/firmware execution of these follow-ups remains a CI gate.

The follow-up measurement at `5b2c0460` uses merge checkout
`f6a6ff6ebe55e0df9daf2475c35d9a03b77a584b` on the same advertised VM/toolchain.
[Run 42007823682](https://github.com/SanderVocke/shoopdaloop/actions/runs/42007823682/job/127178951620)
([artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/42007823682/artifacts/13407701925))
observed FX mean/peak 594/658 us and synth 1,946/2,324 us. Preparation at widths
1/2/8 was 20/241/359 ms; allocation peaks and reservations match the earlier table.
These differing observations do not establish a deadline guarantee or erase the
earlier saturated-synth run that exceeded its block period.

### Validation follow-up at `2e6ac94d`

The [hosted-processor workflow](https://github.com/SanderVocke/shoopdaloop/actions/runs/42025467133)
passes all four independent feature/startup jobs, Node Wasm, excluded assets and
the sole profiling-clock import, plus selected Miri ownership checks. Automated
review reports no major issues for this revision
([receipt](https://github.com/SanderVocke/shoopdaloop/pull/946#issuecomment-6657764959)).
The [firmware run](https://github.com/SanderVocke/shoopdaloop/actions/runs/42025467159)
passes, including firmware, QEMU, board, USB and network deployment checks.
The [broad platform run](https://github.com/SanderVocke/shoopdaloop/actions/runs/42025466864)
passes JACK, bare-metal, process inspection, shared media and release browser
checks; its native test failures isolate the three-poll topology latency regression
addressed during the reconciliation below. This is not a completed-delivery claim.

The negotiated upload window again reaches 64 requests while legacy peers remain
at eight, with the bounded/fair pump retained. A deterministic guarded regression
reproduces late topology chunks exceeding one callback's record budget; it now
passes with acknowledgment and actual-callback pacing. Nine structural tests,
eighteen persistence tests, pump fairness and the production application workflow
pass locally. The legacy lock fixture now confirms graph readiness before
requesting its sole block, preserving the active lock guard and exact 64-frame
position oracle; twenty repetitions pass. Both new regressions also pass filtered
Node Wasm execution. Warning-denying builds, test/tracing inventories and dependency
boundaries pass.

The rebuilt real Chromium AudioWorklet workflow passes FX, synth and External
role imports, dry/wet playback and save/load/save with zero reported xruns.
It acknowledges the existing format-conversion prompt and hosted save dialogs
through their actual UI contracts. The firmware probe retains its five-second
heartbeat and uses the existing persistence-specific 120-second publication
allowance for commits. Its two publication/refusal tests pass locally, and the
firmware workflow confirms the publication fix.

[Measurement artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/42025467133/artifacts/13414696083)
records PR merge checkout `1d997ba8217ac4b1ecfd8ea99480ae508707d06b`; its tree
`1ffdcc23d8b119c30a434cfaa87d4ae72196de76` equals `2e6ac94d`'s tree. This runner
exposes four logical CPUs on an AMD EPYC 9V45 under Microsoft virtualization,
with rustc 1.99.1 / LLVM 23.1.1. The same measurement configuration observes:

| Processor | Mean microseconds | Peak microseconds | Block period microseconds |
| --- | ---: | ---: | ---: |
| Built-in FX | 415 | 577 | 2,666.67 |
| OxiSynth | 2,465 | 5,706 | 2,666.67 |

Preparation at widths 1/2/8 is 14/173/227 ms; requested peaks, retained storage
and reservations match the preceding table. The saturated synth again exceeds
the block period. Shared-runner wall-clock peaks include scheduling variability
and do not establish CPU execution bounds; maximum-polyphony deadline margin
remains unproven. Simulated xruns remain unavailable. These results supplement
the earlier observations, with the same limits on event offsets, simultaneous
instances and third-party Miri coverage.

### Direct-track composite target reconciliation

The target advanced to `40db766d` with P03. The combined engine/application/worklet
wire versions are 21/29/6; processor and composite records retain distinct tags.
Direct-track composites coexist with hosted FX/synth/External tracks in one
session and archive. P17 dry/wet composite interactions remain refused at service,
adapter and RT boundaries, including candidate images and processed children.
Failed candidate loads preserve the live processor state and composite plan;
subsequent valid loads prove that candidate reservations are released.

Focused checks pass: eighteen real DSP graph tests, five processor service tests,
thirteen engine composite tests, eleven monitoring tests, fifteen adapter composite
tests, twenty persistence tests, and the RT/application codec and image tests.
The guarded late-control regression and retained dry/MIDI-window reconnect test
pass. Three latency tests retain their existing poll limits: topology validation
shares the final append's callback while earlier chunks still wait for an actual
callback boundary. Production packages and the firmware probe build with warnings
denied; formatting, test/tracing inventories, parity and dependency audits pass.

Both packaged native socket probes pass their actual Lua/editor/media/archive and
recovery flows. Rebuilt Chromium acceptance with `COMPOSITES=1` passes both Direct
composites and FX/synth/External dry/wet workflows with zero reported xruns. Final
cross-platform CI and automated review must validate the integrated revision.

The four independent DSP feature/startup jobs, both-DSP Node tests, optional asset
and import checks, and Miri pass at `0f395569` in
[run 42032960877](https://github.com/SanderVocke/shoopdaloop/actions/runs/42032960877).
JACK, bare-metal, shared-media and all platform process-inspection jobs pass too.
Debug web CI finds the fixture's inherited profiling tag 40; the combined wire uses
35. Correcting that tag passes the actual artifact's profiling clocks, timestamps,
xrun capture, toggle and memory checks locally. The production codec is unchanged;
the final matrix must validate the fixture correction.

Automated review catches the direct RT candidate path that can convert an empty
processed primitive into a composite. Replacement validation now rejects that
processor-owned program before preparation. The guarded direct-protocol regression
demonstrates acceptance before the fix and refusal afterward, unchanged playing
audio, candidate cancellation and successful preparation of a valid Direct retry.
All eleven replacement tests pass; warning-denying engine/host builds pass.

Broad native CI on this checkpoint passes 3,004 of 3,006 tests; the two remaining
failures are composite service integration cases. Mixed raw transport must still
return Unsupported for deferred composite dry/wet modes when primitive capability
is enabled; that preflight diagnostic is restored before any target is changed.
The detailed local Perfetto trace shows the editor fixture submitting version 7
while version 6 is still pending, causing legitimate supersession. The serial-edit
fixture now waits for queued/submitted/pending client operations to finish with a
finite limit. Its event identity, geometry, mode and empty-notification assertions
remain unchanged. All 24 composite service cases and both affected cases under
actual detailed capture pass; warning-denying app/host builds and test policy pass.

Client composite details expose the current Play/Record/Stop event-mode subset
separately from primitive capabilities. The editor requires both, preserving
disabled P17 actions when dry/wet primitives are available. The menu regression
fails with the previous admission condition and passes with the subset, while
Record and the broader retained reference editor remain usable. All seventeen
composite-widget cases and the real-service projection case pass; client/UI/product
and reference packages build with warnings denied. This local UI projection adds
no wire format or additional composite execution support.

The service follow-up at `8454cb34` passes all **3,006 native coverage tests /
nine skipped** and both Codecov checks in
[run 42037611836](https://github.com/SanderVocke/shoopdaloop/actions/runs/42037611836/job/127280492729).
Editor follow-up `8a1be130` has a clear automated review
([receipt](https://github.com/SanderVocke/shoopdaloop/pull/946#issuecomment-6659834542));
its final integrated platform/browser/firmware results are retained under the
user-directed closure above.

The earlier `649d1fbe` measurement used PR merge checkout
`09e41a9a2bb105f4504f808d8eab82db1b39633f`, with identical candidate tree
`8bcf5145d1b74e891c127df42bd9f2c0b3334c0e`, on four logical Intel Xeon 6975P-C
CPUs under KVM and the same Rust/LLVM versions. Its
[artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/42034699772/artifacts/13417757962)
observes FX mean/peak 508/545 us, synth 2,251/2,689 us, and width 1/2/8 preparation
in 15/190/260 ms, with the same memory values. It also exceeds the synth block
period and does not establish deadline margin.
