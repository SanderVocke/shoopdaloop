# Application Grab and Replacing integration

Implementation contract for [the application plan](../../PLAN_FINISH_GRAB_REPLACE.md).
Both P04 and P05 remain **OPEN**. No production capability is enabled by this
document or the window arithmetic helper. Final application evidence will be
recorded here as the implementation stages pass.

## Layer inventory

| Layer | Required work |
| --- | --- |
| Normal host preparation | `shoop_host/engine`, native `direct`, and worklet `config`/`host` must prepare histories from a common production policy. `shoop_rt_engine::prepare` currently supplies empty shared settings. |
| Portable domain | Add continuous writers and prefix continuation to `shoop_loops/runtime`, with a distinct Replacing mode in the resolver/program state. Retain the private-candidate API for prepared edits. |
| Kernel resources | Grab roots now retain visible audio pages/MIDI chunks; prepared metadata follows retained credits. Production global pooling, checkpoint-state density and continuous-writer reservations still need completion. |
| Engine / protocol | Add bounded admission, musical batch settlement, completions and capability/limit negotiation in `shoop_rt_protocol`, `shoop_rt_engine/endpoint`, jobs, model, cells, topology and resync. |
| Adapter | Implement Grab and Replacing in `shoop_rt_backend/backend`, requests, incoming/mirror, reads, capture, family and persistence. Invalidate canonical-import media on actual committed revisions. |
| Service | Replace deliberate refusals in `shoop_app/service/commands` and `commands/control`; reconcile media, projection, persistence and family guards with active/scheduled Replacing. |
| UI / Lua | Enable negotiated actions in loop/global controls and the normal app runtime; preserve script constants and route every action through service ownership. |

## Reference policy matrix

Let L be the current sync period, P its phase at the effective cut, N the
apply-N-cycles value clamped to at least one, T the targeted loop's period in
sync cycles, and C its current cycle. Resolve these musical values from
authoritative state at settlement, not an earlier UI poll.

| Intent | Targets | Window / initial mode | Completion |
| --- | --- | --- | --- |
| Synced UI Grab, no target | Initiator plus selected peers only when the initiator is selected | Last N complete sync cycles before current phase; position P; Playing if play-after-record, otherwise Stopped | One coherent batch |
| Synced UI Grab with target | Same selection rule | T complete cycles ending before phase P plus C earlier cycles; position C*L+P; same post-mode policy | One coherent batch |
| Unsynced UI Grab, no target | Initiator only | Prefix (N-1)*L+P ending at the live cut; continue Recording from its end | Finish at the reference next sync boundary into Playing/Stopped |
| Unsynced UI Grab with target | Initiator only | Prefix C*L+P ending at the live cut; continue Recording | Finish using target-delay policy |
| Control/Lua Grab | Exactly the explicit list | Global musical Grab policy, with no implicit selection expansion | Reliable batch outcome |
| AdoptRingbuffers | Exactly the explicit list | Explicit window, cycle position and post-mode | Reliable batch outcome |
| Solo | Peers on affected tracks | Stop only peers outside the admitted target set | Do not stop peers if admission refuses |
| Missing sync length for UI/default Grab | No mutation | Refuse with an actionable reason | No optimistic completion |

The low-level `shoop_loops::grab::WindowSpec` returns a retained source range,
logical duration, cycle/phase destination and explicit trailing empty duration.
It implements window arithmetic only. The service/domain must still validate
source continuity, IDs, modes, Q, budgets and admissibility before acquiring or
publishing media. Its frame coordinates remain u64, including above u32 time.
Available samples start at logical zero in the legacy adoption path; missing
requested duration is explicitly empty at the end. An unavailable/faulted source
range is not silently reclassified as startup padding.

`GestureSpec` resolves the musical portion using the authoritative sync period
and targeted state at that cut. It returns the initial mode and, for unsynced
grabs, the completion mode/delay. Target membership, solo peers and atomic
admission remain service/engine responsibilities. The target-membership audit
found that legacy Control Grab expands selection while raw AdoptRingbuffers is
exact-list. The user identified selection-dependent explicit scripts as a bug;
Control/Lua Grab will intentionally use the exact supplied list. UI gesture
selection semantics remain as shown above.

## Replacing and continuation contracts

Replacing uses fixed-period overwrite and cursor repetition, without ordinary
playback from that loop. Input monitoring remains a separate route. Playing to
Replacing preserves phase; Stop rewinds according to the reference. MIDI empty
positive intervals erase originals, and edits have natural suffix-state effects.
No synthetic restoration packet is stored at a splice.

Unsynced Grab must seed a growing recording with the retained prefix and join
the live stream once at the cut. A normal Record transition that starts a fresh
empty take is insufficient. Stop and playback return complete immutable media;
old exports and sibling takes retain their original data.

Current CandidateMidi preparation owns one patch, rebuilds affected indexes in
bounded steps, and refuses another patch while Building. It cannot be polled as
a continuous recording implementation. The continuous writer must separately own
every admitted tick and bound index debt, input retention and exit resources.
Neither moving publication deadlines nor queue growth establishes that bound.

## Production sizing gate

The reference nominal retention is 30 seconds. At 48 kHz, sixteen stereo sources
need 184,320,000 payload bytes for that interval. The shared arena's page geometry,
retained users, COW writes and MIDI add to this amount. The existing 256 MiB
loop-memory setting must keep its documented meaning.

Audio owners now charge the window's covering pages and retain a clipped root.
Its index budget follows page credits plus boundary paths rather than a full ring
per view. MIDI owners similarly retain and charge the intersecting chunks;
metadata follows the total retained-chunk credits, with admission reserving edit
growth and immutable source pins. These conservative guarantees still need
production global pooling and
admission must reserve guaranteed live history/writer work while sharing immutable
data, bounding metadata and refusing additional owners before live capture loses
its reservation. Exact budgets/defaults and their measured limits remain an F2
verification gate, not an implied unlimited capacity claim.

MIDI configuration now offers an allocation-free estimate of the complete
prepared metadata/state shape and owner ceiling. Unreachable handle/initial-state
slots are excluded. A heap-guarded pressure test retains all admitted owners while live
input wraps repeatedly, then edits a retained cut and checks the old export.
Further packet/state oracles cover clipped roots and growing/shrinking edit-credit
transfer while old exports remain readable. Full checkpoint projections,
per-source reservations and continuous-writer debt
still require the remaining production work; this change does not establish a
default application budget or enable either capability.

## Validation status

No application success is claimed yet. CI will run the new arithmetic/reference
cases and the complete integration gates as the plan progresses. Existing
unsupported-backend tests remain valid until negotiated support is implemented.
