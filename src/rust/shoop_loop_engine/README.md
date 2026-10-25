# shoop_loop_engine

Portable loop timing and audio/MIDI execution on `rt_runtime`.

**Implementation in progress.** The current implementation provides checked
identities/capacities, topology validation, sparse composite-plan compilation,
and a fixed-storage primitive/composite timeline resolver. Prepared audio
playback/capture operations now consume supplied interval positions, with indexed
conversion, gain and discontinuity smoothing; see [audio operations](docs/audio.md).
Runtime planner integration and MIDI media execution remain pending.

The resolver API supports `submit`, retained `completion`/`take_completion`,
`cancel`, `preview`, `commit`, and `finish`. `preview` produces candidate
`decisions`; only `commit` publishes state, terminal outcomes, and semantic
events. Finish the selected interval before another submission or boundary.
A failed or invalidated preview cannot be committed or expose valid decisions.
`Decision::playback_discontinuity` is an authoritative, transactional media restart
flag, independent of optional journal storage. Same-position seeks and explicit
replacements can set it without changing visible state; final non-playing states
clear it. Use this flag rather than diagnostic events to drive renderer smoothing
and MIDI cleanup/restoration.

Pending requests and unread terminal records share fixed reserved slots.
Automatic wraps and sync pulses do not consume those slots. Journal capacity
is optional: overflow increments a saturating diagnostic without changing
musical execution. Clock edges coalesce per loop/frame; a prepared sync
component terminates by visiting each seeded member once, not by a wave quota.

`replace_topology` checks the expected epoch and generation high-water marks,
revalidates pending requests, and returns the displaced owner. Removed loops
must already be stopped. Changed running programs are safely refused here;
staging their replacement until iteration zero remains an integration milestone.
Dispose returned/rejected topologies off-render.

Composites execute sequential/parallel/delayed/repeated prepared entries and
finite nesting. Regular programs repeat; explicit-mode programs execute one
pass. `PlayIteration` selects a starting iteration and `SeekIteration` selects
an authoritative snapshot without replaying preceding events. A non-retriggering
seek repositions already-playing descendants without inventing new clock edges;
newly activated playback/recording still has its normal start edge.

Composite cursors report iteration, passes, and the frame the iteration was
entered. `last_pulse` separately records the last actual clock edge. Nominal
units are duration/seek metadata, never a clock generator; an unsynchronized
program starts at its selected iteration and stalls until explicitly repositioned.
A stopped source can still relay clocks from its own source. Composite position
is not a synthetic uniformly advancing media playhead.

The resolver API models logical capture positions and boundary failures; it does not
yet own captured samples, deliver runtime receipts, or provide independent
recovery-lane admission. Those integrations remain required plan milestones.

The resolver owns authoritative timing; separate render operations execute
supplied interval decisions without their own playback clock. Native and browser hosts use the same semantics;
transport, devices, UI, and AudioWorklet bindings stay outside this crate.

The implementation contract and progress are tracked in the repository's
`PLAN_LOOP_LAYER.md` and `docs/loop_layer/`. No full legacy backend compatibility,
unlimited recording, or hard-real-time deadline guarantee is promised.
