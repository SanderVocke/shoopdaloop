# rt_runtime

A generic audio/MIDI runtime crate under development. It does not depend on Shoop domain types, device APIs, or browser bindings. Test support is dev-only.

**Current milestone:** validated configuration, checked IDs/clocks, bounded borrowed event storage, processor preparation/render contracts, and an empty-graph render endpoint. Graph execution, control transport, and resources are not implemented yet; do not infer those capabilities from the crate name.

```rust
use rt_runtime::{Config, Frame, RenderRuntime};
use rt_runtime::buffer::AudioBlock;

let config = Config { audio_inputs: 0, audio_outputs: 1, ..Config::default() }.prepare()?;
let mut runtime = RenderRuntime::new(config);
let mut channel = [1.0; 128];
runtime.process(AudioBlock {
    frame: Frame(0), frames: 128, inputs: &[], outputs: &mut [&mut channel],
})?;
assert_eq!(channel, [0.0; 128]);
# Ok::<(), rt_runtime::Error>(())
```

Run `cargo test -p rt_runtime` and `cargo test -p rt_runtime --release`. Tests install a counting allocator with positive allocation/reallocation/deallocation canaries; the same assertions run in release and Wasm. Counts are measured only inside render scopes, not during test framework setup. A separate audit of callback-reachable code is required for locks/waits/I/O; allocator instrumentation does not detect those.

Run `python3 scripts/check_rt_runtime_boundary.py` from the workspace root to audit production/build dependencies on all supported targets. The shared repository test attribute is a dev-only exception. Registering the browser-test feature must not introduce test support into the production dependency tree.

See `docs/rt_runtime/CONTRACTS.md` and `docs/rt_runtime/VALIDATION.md` at the workspace root for intended API contracts and actual milestone evidence. `PLAN.md` and `DESIGN.md` define the remaining implementation and scope. No Shoop integration is included.
