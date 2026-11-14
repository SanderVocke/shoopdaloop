# Resident public commands and native execution cells

This is an M5 component checkpoint, not the migrated application transport.
`public_native_probe` connects the shared ARM public-row owner, real public
storage, private delivery and real TP. The continuous wrapper now connects that
owner in its explicit resident application profile; general prepared/demo/board
defaults retain their raw profile. Historical query/export wiring is implemented.
Mutable parameter/graph continuity and the actual Rust application service/clock
split remain open.

The two capacities have different lifetimes. A public row reserves an admitted
future action and retains its eventual receipt. Native TP cells represent only
requests actually delivered at a device cut. Allocating all future public actions
in the TP would change pool exhaustion, lowest-slot identity reuse and STOP
cancellation. Instead delivery scans the authoritative native cells for the
lowest free, nonretired generation, installs there and saves the resulting ID in
the public row. Native collection happens after FINISH; the public result remains
owned until its separate generation-qualified collection. Copying a receipt to
ARM does not release that public ownership.

The one-loop resident TP profile initializes a fixed forward roster across all
configured native cells. Invalid/outcome/type/target filters retain the resolver's
ordinary behavior. Collection updates both generation tags in place, preserving
forward links; exhausted generations retire at zero and allocation skips them.
The profile rejects replacement membership. General multi-loop/native membership
profiles retain their existing publication protocol and capacities.

`native_requests` is the sole native allocation/collection owner. A cached full
pool is a positive fact: only successful collection can make a cell free, so it
invalidates the cache. It contains no second free/generation table or musical
resolver. A deployed managed profile must prevent an independent raw host from
mutating native requests. The TP private command port remains independent of a
held host reply, while execution/sample/observer priority remains bounded.

`audio_native` checks the actual target identity at delivery before deferred
static errors and native capacity. CLEAR additionally checks the actual revision
watermark before allocating. Future AT and tick requests use the existing TP
resolver and compact action/result encoding. Mode 1 reads authoritative pending
or terminal rows after PROCESS; mode 2 collects terminal native resources after
FINISH, comparing the complete saved compact terminal before release.

Planner cancellation follows its publication's generation-qualified reference.
Unknown/recycled/wrong-family targets return InvalidId. Queued normal planner
requests become cancelled public receipts without native allocation. Delivered
native requests are cancelled through the TP; their public row stays pending
until outcome resolution, so two cancels in the same cut both report delivered.
Retained normal terminals and a normal cancellation targeting itself return
InvalidState. Queued/retained recovery targets and a recovery cancellation
of itself return InvalidId. Pending non-transition queries return InvalidState.
The private lookup/write guard checks lifetime identity and acknowledges after
synchronous RAM commit. No sequence search is implemented in fabric.

Private INITIAL_LOOP (22) is available only in the resident one-loop profile. It
installs a valid empty primitive with no/self sync source into an uninitialized
slot without resetting musical time or native generations. A repeated activation
is refused. It is a primitive for a later composite signal-image activation;
it does not by itself qualify that public composite or expose a general live
reconfiguration path.

The component comparison admits 32 future actions before pausing ARM service for
1,040 quanta. Four native cells are available, all public results remain retained
and a host STATUS reply is held throughout. Every loop state and ordered event
matches the independent production Rust resolver. Results preserve original
requests, effective/terminal frames and native IDs: slot zero is reused with
successive generations. Separate read-only test instrumentation observes state
and events; execution never depends on consuming those observations. Directed
checks cover duplicate, retained-terminal, self, recovery and unknown cancels,
plus pending tick reuse, STOP cancellation, target/static/capacity error
precedence, full-cache invalidation and frame-preserving initialization.
This component test establishes no continuous-audio deadline or physical claim.

The connected continuous comparison now covers 1,312 quanta, 41,984 stereo
frames, 2,624 coherent cuts, nine transitions and five historical queries/exports
in instant, fixed and DRAMsim3 memory. A 1,040-quantum ARM pause includes both
stereo takes, future AT/self-sync/tick actions and STOP cancellation. All audio,
ordered events and post-FINISH states match independent production Rust. Queries
match the independent pre-FINISH state, preserve their actual scalar/epoch values
and retain original capture start/end metadata when delivered much later.

Native state/export records have a separate receipt lifetime from export leases.
Generation/session-checked word reads do not release them; public row collection
acknowledges the retained record first. Component tests fill every public slot,
advance 1,050 cuts with no host service and recover exact historical metadata.
They check scope/error precedence, empty exports, borrowed cursors, retirement,
word bounds, generation reuse and session reset. These tests do not complete
application migration, canonical mutable state, bulk reads or physical timing.
