# Host-side track processor contract

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
Bus catalogs advertise Built-in FX under the [bus contract](bus-fx.md). Bus and track FX share prepared slots and channel limits. Carla remains unavailable.

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
and Built-in bus destinations; idle processors retain latest values without rendering or
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
Only implemented facets are advertised; general P09/P10 controls, Carla remains separately refused; Built-in bus FX follows the bus contract. P06 is complete within this contract; P07 remains open for Carla.

## Delivery limitations

P06 was closed at the user's direction before all platform CI completed.
The unused-helper build failure and stale lockfile pins were subsequently repaired;
that does not establish a fully green current matrix. Check
[PR #946](https://github.com/SanderVocke/shoopdaloop/pull/946/checks) for final
platform results. Carla and dependent composite modes remain open in the
[feature backlog](feature-parity.md).

Saturated-synth measurements exceeded their block period in one run. Later lower
measurements do not erase that limitation or establish a hardware deadline guarantee.
