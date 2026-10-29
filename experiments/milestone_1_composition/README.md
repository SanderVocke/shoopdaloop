# External local-composition feasibility spike

This standalone workspace is the Stage 1 experiment, not the production service
or a completed milestone. The `dsp` library depends only on `rt_runtime`; the host
uses the public loop facade without changing it. Tests use the repository's test
macro and heap/destructor instrumentation, not imported private fixtures.

From the repository's development environment, at the repository root:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --locked --manifest-path experiments/milestone_1_composition/Cargo.toml
CARGO_TARGET_DIR="$PWD/target" cargo test --locked --release --manifest-path experiments/milestone_1_composition/Cargo.toml
```

The fixed two-preset stereo mixer is intentionally minimal. A component-native
transport has one normal and one recovery credit. The host calls
`RenderControl::begin_callback` once before its one engine callback and
`finish_callback` afterwards, including after an engine error. The effect consumes
one copy-only directive inside the authoritative `ProcessBlock` interval and
publishes its actual acknowledgement. The client never accesses that mailbox or
mutable processor state. A future request stays in the existing transport, not in
an additional event scheduler.

The tests must demonstrate exact interior-frame changes without resolver cuts,
loop-induced internal cuts and autonomous wraps, terminal retention while the
client stalls, explicit recovery after normal dequeue, and heap/free guarding of
the entire host-control/render/result path. They do not establish MIDI correctness,
service teardown, latency compatibility, measurements, portable test discovery or
all acceptance criteria. Promote a successful design into the planned new crates
with their complete tests; do not leave the spike as an alternative engine.
