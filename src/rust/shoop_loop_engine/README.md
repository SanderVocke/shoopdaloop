# shoop_loop_engine

Portable loop timing and audio/MIDI execution on `rt_runtime`.

**Implementation in progress.** The current milestone provides preparation-time
identities, capacity validation, topology validation, and sparse composite-plan
compilation. It is not yet a running loop engine or production backend adapter.

The resolver will own authoritative timing while separate render operations
execute interval decisions. Native and browser hosts use the same semantics;
transport, devices, UI, and AudioWorklet bindings stay outside this crate.

The implementation contract and progress are tracked in the repository's
`PLAN_LOOP_LAYER.md` and `docs/loop_layer/`. No full legacy backend compatibility,
unlimited recording, or hard-real-time deadline guarantee is promised.
