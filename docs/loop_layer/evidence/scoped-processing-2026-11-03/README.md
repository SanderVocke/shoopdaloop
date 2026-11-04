# Scoped processing evidence

The source revision and commands are recorded in `results.json`. This is new
evidence for the scoped-processing foundation; earlier DSP/network benchmark
archives are unchanged.

`example.txt` is output from a detached clean checkout. Its executable asserts
independent sample/event oracles before reporting success. `validation.txt`
records the verification summary. The compressed logs retain the underlying
native, release, portable, WebAssembly and browser results.

The real interleaved bus path renders five loop intervals in a 17-frame callback
and calls DSP once at `(0, 17)`. The shared dry/wet path advances one resolver,
plays prerecorded stereo media, processes DSP and captures actual output during
`[5,15)`. It verifies all 20 captured samples, four captured MIDI events and the
DSP-generated note held before the capture start. Irregular callbacks and an
origin of 4,294,967,333 preserve alignment.

The dense case admits 32 one-frame timing intervals and one `(0,32)` DSP call.
There are no callback heap operations. Its plan storage is 8,704 bytes per owner
for the prepared 32 spans on this x86-64 host; storage sizes on other targets can
differ. Offering only 31 spans is refused during preparation before publication.
The synchronized-owner case uses one resolver and distinct media/receipts.

Negative cases cover real capture capacity exhaustion with an owning old export,
actual processor/capture faults, missing consumer completion, MIDI overflow and
reserved reset delivery, local activation with MIDI on the cut, parent-interface
refusal, stale epochs and off-render plan retirement. A failing cleanup reports
its own error without replaying frames. Independent pre-existing host oracles
also verify unchanged validation allowances and wire diagnostics.

Reproduce from the repository root using its documented build environment:

```sh
cargo run --locked -p shoop_loops --example scoped_processing
cargo test --locked -p rt_runtime --test scoped_processing \
  -p shoop_loops --test callback_runtime
cargo test --locked --release -p rt_runtime --test scoped_processing \
  -p shoop_loops --test callback_runtime
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
python3 scripts/run_rt_runtime_hosts.py --native-only --profile debug
python3 scripts/run_loops_hosts.py --native-only --profile debug
python3 scripts/run_rt_runtime_hosts.py --native-only --profile release
python3 scripts/run_loops_hosts.py --native-only --profile release
python3 scripts/run_wasm_tests.py --profile dev --runtime node \
  --package rt_runtime --package shoop_loops --trace failure
```

The browser run uses the README's Trunk build and new-engine smoke with pinned
Node 22.22.2, wasm-pack 0.15.0, wasm-bindgen 0.2.127 and local Chromium
154.0.8037.57. It includes persistence and bus zero/recreation flows. CI uses its
own pinned browser/toolchain gates. See the [API guide](../../scoped-processing.md)
and [plan](../../scoped-processing-plan.md) for limits and final CI/review status.

These runs establish correctness and resource bounds. They do not measure
physical-device latency or certify worst-case callback deadlines.
