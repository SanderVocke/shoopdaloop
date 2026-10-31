# rt_runtime

## Part I — Process audio

- [1. The runtime and the host](part-1-process-audio/01-the-runtime-and-the-host.md)
- [2. Your first audio graph](part-1-process-audio/02-your-first-audio-graph.md)
- [3. Writing processors](part-1-process-audio/03-writing-processors.md)
- [4. Connecting an audio callback](part-1-process-audio/04-connecting-an-audio-callback.md)

## Part II — Control a running graph

- [5. Commands, completions, and ownership](part-2-control-a-running-graph/05-commands-completions-and-ownership.md)
- [6. Routing and live graph edits](part-2-control-a-running-graph/06-routing-and-live-graph-edits.md)
- [7. Changing parameters at an exact sample](part-2-control-a-running-graph/07-changing-parameters-at-an-exact-sample.md)
- [8. Processing and scheduling MIDI](part-2-control-a-running-graph/08-processing-and-scheduling-midi.md)

## Part III — Work with stored media

- [9. Loading and reading resources](part-3-work-with-stored-media/09-loading-and-reading-resources.md)
- [10. Recording and exporting](part-3-work-with-stored-media/10-recording-and-exporting.md)
- [11. Seeking and reading MIDI events](part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md)
- [12. MIDI state and checkpointed seeking](part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md)

## Part IV — Coordinate processors and changes

- [13. Planning processing intervals](part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md)
- [14. Application-specific operations](part-4-coordinate-processors-and-changes/14-application-specific-operations.md)
- [15. Staging and coordinating graph activation](part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md)
- [16. Recording resources together](part-4-coordinate-processors-and-changes/16-recording-resources-together.md)

## Part V — Configure and operate the runtime

- [17. Capacity, work budgets, and diagnostics](part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md)
- [18. Submitting without allocation](part-5-configure-and-operate-the-runtime/18-submitting-without-allocation.md)
- [19. Stopping, disconnecting, and reclaiming ownership](part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md)

## Terms used in this book

| Term | Meaning |
|---|---|
| Host / callback | Your application's audio callback; the host supplies borrowed buffers, the runtime processes them. |
| Control side / render side | The control endpoint submits commands and collects results; the render side owns the running graph and processes buffers. |
| Processor | A node with audio/MIDI ports that implements the work done per processing call. |
| Graph | The installed set of processors and the connections between their ports and host ports. |
| NodeId / generation | Identifies a processor slot; the generation distinguishes replacements in the same slot. |
| Epoch | The runtime's checked identity of the currently active graph; used to reject stale changes. |
| Command / ticket | An owned request submitted to the render side; its sequence number identifies it. |
| Completion | The owning return of a command: the request, its outcome, and any retired object. |
| Execution result | The separate receipt for a scheduled action, delivered at its actual frame. |
| Lane / recovery | The normal command lane, plus a small independent recovery lane for bounded urgent operations. |
| Resource / revision | A stored media object and its version; uploads create new revisions. |
| Snapshot / lease | A pinned, immutable view of a resource at some revision; dropping a copied handle releases nothing — release leases explicitly. |
| Planner | An optional single authority that settles domain decisions for each processing interval before processors run. |
| Decision binding | Links a consumer processor to a planner decision slot; not a signal connection. |
| Stage / Ready | A validated but not-yet-active graph candidate; activation publishes it at one boundary. |
| Capture group | Coordinated storage for recording several resources together. |
| Capacity vs budget | Capacity is prepared storage (fixed at prepare time); budgets bound optional work per callback. |

## Reference

- [Reference index](reference/README.md)
- [Configuration and errors](reference/configuration-and-errors.md)
- [Execution rules](reference/execution-rules.md)
- [Media and MIDI policy](reference/media-and-midi-policy.md)
- [Lower-level APIs](reference/lower-level-apis.md)
- [Host boundaries](reference/host-boundaries.md)
