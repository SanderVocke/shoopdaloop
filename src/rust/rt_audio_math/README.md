# Audio arithmetic

`Sample` selects audio arithmetic at compile time. `f32` is the default;
`Q824` stores signed Q8.24 samples directly in an `i32`, with a range of
[-128, 128 - 2^-24]. Generic sample loops use static dispatch.

Fixed gain multiplication widens to `i64`, truncates toward zero and saturates
to `i32`. Addition/subtraction saturate at each operation. Operation order is
therefore observable: accumulation and gain stages must keep their declared
grouping. Raw sample bits and little-endian bytes never pass through float.

`Ramp` retains its start, target, duration and elapsed frames across processing
blocks. For duration D > 0, sample n uses
`start + trunc0((target - start) * min(n, D) / D)` in the fixed policy.
Sample zero uses start; sample D uses target; D=0 applies target immediately.
An interrupted ramp starts from the previous ramp's value at that boundary.
Float uses the same sample-time convention. Filling fixed ramps uses integer
stepping checked against a widened formula at every sample, including extreme
endpoints and durations.

`rt_runtime` buffers/resources and `shoop_loops` capture/playback use this
policy with float defaults. To instantiate fixed processing, use types such as
`GraphRuntime<Node, LoopPlanner<Q824>, (), Q824>` and
`ResourceStore::<Q824>::new_numeric(config, quantum)`. Processors and typed
operations must use the same policy. Existing float controls and window gains
are explicit conversion boundaries; device and persistence formats retain
their existing float representation. A host must convert those formats when
using a fixed instance.

`shoop_rt_engine::dsp` shares generic gain, balance and ramp processing.
The normal application engine and its FX/plugins remain float.

```sh
cargo test -p rt_audio_math -p rt_runtime -p shoop_loops -p shoop_rt_engine
```

Regressions cover raw-bit capture/playback, ordered saturating routing,
callback partitioning with declared buffering latency, interrupted ramps,
bounded float agreement and heap-free render scopes.
