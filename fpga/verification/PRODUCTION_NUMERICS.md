# Production numeric policies

`GraphRuntime<P, L, Q, S>`, its processor/planner contexts, buffers and resource
store use `S: Sample = f32`. `LoopPlanner<S>` and the actual playback/capture
nodes use the same policy. `Q824` stores raw signed i32 values and supplies the
specified saturating arithmetic. There is one runtime and loop implementation;
the default application remains float. Application-only FX and plugin processors
remain float; the hardware comparison profile does not instantiate them.

Audio byte APIs encode little-endian samples of the store's policy. Float stores
retain IEEE-754 encoding; fixed stores retain raw Q8.24 bits, including low bits
that f32 cannot represent. Device/persistence adapters must explicitly convert
float data. Typed loop/processor controls preserve fixed values; the existing
float parameter action and window-control metadata remain conversion boundaries.

Production gain controls retain start, target, duration and elapsed frames.
Sample zero uses start, offset duration uses target, and shorter ramps hold their
target for the rest of a block. Gain, scale and balance coefficients are evaluated
per sample in their declared order. Restart smoothing follows the same time
convention. Reference render modes materialize changing controls when their
endpoint descriptor cannot express the curve. Their comparison now checks every
sample, including the middle of each quantum.

The production capture/playback test runs both policies through the real graph,
planner, capture groups and media codecs, checks raw fixed readback, and checks a
justified float error bound. Runtime routing tests use saturating operands to
verify canonical compiled connection order. Shared arithmetic tests cover extreme
and interrupted ramps; production control tests check partition invariance and
float agreement. Full-session error statistics and drift checks remain a required
part of the common runner, not a replacement for exact Q8.24/RTL comparisons.

`production-numeric.json` records paired release measurements against the saved
baseline: Q=32, 128-frame callbacks, four stereo voices, three alternating runs
per phase on one shared-host CPU. The ratios of median callback times are 1.012
playback, 0.985 record/play, 0.938 recording and 1.010 clear. All measured callbacks
have zero allocations, reallocations and deallocations. These measurements show
no material default-float regression; they do not establish a hardware deadline.
