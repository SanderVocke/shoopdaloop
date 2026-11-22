# Generated payload hint experiment

The control remains C1 (`6be27ce0`). `READ_HINT_POLICY=2` preserves early SP
and late JP hints and is the default. Value1 enables only the late JP hints;
value0 acknowledges late hints and emits no generated payload hints. The public
media-store hint port and all read/write operations remain implemented. Samples,
MIDI, accepted command semantics, publication, fault and reset guarantees must
remain exact; only speculative traffic and latency may change.

All three policies use the same five parameterized wrappers. The canonical
default first needs exact connected-cycle calibration against C1 and a physical
control to separate RTL naming/parameter effects. Then measure1 and0 sequentially,
including FIFO/resolver costs, traffic, stalls and full quantum latency.
Capacity tuning, if warranted, is a separate experiment.

A new pipeline fixture runs policies1/0 with publication, held-output, fault,
reset and progress checks in Instant, Fixed, DRAMsim3 and Legacy memory modes.
The original default-policy test remains. The separate benchmark executable
`midi_area_experiment` uses the same fixture/CLI; it preserves the frozen original
benchmark executable. The explicit new parameter allowlist admits only0/1/2.

Status: canonical default2 reproduces all33 C1/B0 connected rows exactly. Its
physical control is running. Late-only1 passes blocked paths and Legacy protocol
cases but fails the original first-buffer validity assertion after an injected
second-view read fault in Instant, Fixed and DRAMsim3 modes: the first cached
buffer is published before the later fault arrives. The probe waits for the
actual failing transaction before releasing its paused response; the original
default test still requires the fault to be in flight at the first scratch write.
Neither assertion is relaxed. This is a rejection for a behavior-preserving
default, pending area and normal playback measurements.

The optional `hint_policy_protocol` executable records each case's pass/failure,
including source hashes, without turning an experimental failure into a passing
default test. No-hint0 still needs its own probe and measurements.
