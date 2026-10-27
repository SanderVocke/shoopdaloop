# Chapter 2 review: retain verified frame-zero startup

Destination: `docs/part-1-process-audio/02-your-first-audio-graph.md`.
The complete draft and `examples/first_graph.rs` were read. Hashes/checks are in
`../part-one-baseline.json`; the draft differs from the original empty baseline.

## No startup-example rewrite needed

The current example correctly uses `prepare_with_initial_graph` with `NoPlanner`,
an explicit `ResourceStore`, prepared nodes, graph and empty decision bindings.
It asserts epoch1/frame0/no outstanding command credit, renders eight known samples
under the heap guard, checks the resulting frame and destroys owners off-render.
It ran successfully in native debug and release with warnings denied. Preserve it.

The constructor compiles/installs off-render; it does not execute the planner or
processors, advance the timeline, require a warm-up callback, or consume ingress/
receipt credits. Invalid preparation drops consumed inputs off-render. Do not copy
the archived getting-started example's render-until-Commit loop into this startup
path; that older example teaches a different live-validation workflow.

## Keep these distinctions at their existing point of use

- `Config::prepare` validates arithmetic/limits; runtime/resource/processor
  construction allocates and initializes backing storage separately.
- `NodeId` slot/generation, endpoint direction, planar channel arrays and the exact
  selected prefix are already explained correctly. `AudioPorts::pair` borrows
  prepared storage; it must not be replaced with a per-call allocated reference list.
- Empty decision bindings are correct for `NoPlanner`, not a universal recipe for
  planner consumers. Keep the forward link to chapter13; its example must use
  validated planner/consumer metadata and explicit bindings.
- Startup installation, ordinary live `Commit`, and Ready→permit→coordinated
  activation are three different paths. The present forward links to chapters6/13
  are enough here; explain Ready versus active fully in chapter15.
- This synchronous example has no accepted outstanding work or acquired snapshot
  leases at destruction. Keep its minimal teardown, but do not generalize dropping
  the endpoints into graceful cancellation/release of arbitrary domain state.

`tests/initial_graph.rs` and `contracts.rs` are the underlying startup/host boundary
checks; the Stage5 preparation evidence retains their history. The refreshed example
execution here is only this native demonstration, not a new actual loop-worklet claim.
Preserve source-excerpt markers and rerun the book checker after any later excerpt
change. No new example or expanded monolithic application is required for this slice.
