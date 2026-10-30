# 12. Editing the graph around loops

[Contents](../README.md) · [Previous: Exporting and importing media](11-exporting-and-importing-media.md) · [Next: Capacities, refusals, and results](../part-6-operating/13-capacities-refusals-and-results.md)

The signal graph around a loop is yours: you can route host ports, your own processors, and loop nodes however you like at prepare time, and `rt_runtime` lets you edit routes live (see [its chapter 6](../../../../rt_runtime/docs/part-2-control-a-running-graph/06-routing-and-live-graph-edits.md)). Two rules protect the loop library's timing:

1. **The planner refuses graph changes that would drop or replace its nodes.** A live edit that removes a loop's playback or capture node is refused; the running loop keeps its decisions.
2. **Node-pair swaps go through staging.** Replacing which physical nodes a loop uses is not a plain route edit. You stage it with `Operation::NodePairBegin` / `NodePairAppend` / `NodePairStatus`, then activate with `Operation::ConfigurationActivate`, which publishes the new node pair together with media, timing, and routes as one coherent change. `Options::default()` disables staging; enable it via `Options { node_pairs: NodePairSettings { candidates, sources, source_packets_per_callback }, .. }` when you need it.

For the common case — adding your own processors and wiring them around loops — no staging is needed:

```text
host in ──▶ capture ──▶ (loop) ──▶ playback ──▶ your FX ──▶ host out
```

Build that graph at prepare time in `GraphDescription` exactly as in chapter 2, adding your processor nodes and connections next to the loop's two nodes. The loop's `Binding` still names its playback/capture nodes; everything else is ordinary graph wiring.

If a running loop must move to different nodes (for example because you rebuilt your FX chain), stage the node-pair change instead of editing routes around it — the planner will refuse the unsafe edit and the staged path is the supported one.

[Previous: Exporting and importing media](11-exporting-and-importing-media.md) · [Next: Capacities, refusals, and results](../part-6-operating/13-capacities-refusals-and-results.md)
