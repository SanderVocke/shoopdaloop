# shoop_loop_engine

Portable loop timing and audio/MIDI execution on `rt_runtime`.

**Implementation in progress.** The current implementation provides checked
identities/capacities, topology validation, sparse composite-plan compilation,
and a fixed-storage primitive timeline resolver. Composite execution, runtime
planner integration, and audio/MIDI media execution remain pending. Composite
topologies are explicitly refused by the interim resolver, not partially run.

The primitive API supports `submit`, retained `completion`/`take_completion`,
`cancel`, `preview`, `commit`, and `finish`. `preview` produces candidate
`decisions`; only `commit` publishes state, terminal outcomes, and semantic
events. Finish the selected interval before another submission or boundary.
A failed or invalidated preview cannot be committed or expose valid decisions.

Pending requests and unread terminal records share fixed reserved slots.
Automatic wraps and sync pulses do not consume those slots. Journal capacity
is optional: overflow increments a saturating diagnostic without changing
musical execution. Clock edges coalesce per loop/frame; a prepared sync
component terminates by visiting each seeded member once, not by a wave quota.

`replace_topology` checks the expected epoch and generation high-water marks,
revalidates pending requests, and returns the displaced owner. Removed loops
must already be stopped. Dispose returned/rejected topologies off-render.

This API models logical capture positions and boundary failures; it does not
yet own captured samples, deliver runtime receipts, or provide independent
recovery-lane admission. Those integrations remain required plan milestones.

The resolver will own authoritative timing while separate render operations
execute interval decisions. Native and browser hosts use the same semantics;
transport, devices, UI, and AudioWorklet bindings stay outside this crate.

The implementation contract and progress are tracked in the repository's
`PLAN_LOOP_LAYER.md` and `docs/loop_layer/`. No full legacy backend compatibility,
unlimited recording, or hard-real-time deadline guarantee is promised.
