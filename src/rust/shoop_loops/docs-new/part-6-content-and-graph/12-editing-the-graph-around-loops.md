# 13. Editing the graph around loops

[Contents](../README.md) · [Previous: Exporting and importing media](11-exporting-and-importing-media.md) · [Next: Capacities, refusals, and results](../part-7-operating/13-capacities-refusals-and-results.md)

The signal graph around a loop is yours: you can route host ports, your own processors, and loop nodes however you like at prepare time, and `rt_runtime` lets you edit routes live (see [its chapter 6](../../../rt_runtime/docs/part-2-control-a-running-graph/06-routing-and-live-graph-edits.md)). Two rules protect the loop library's timing:

1. **The planner refuses graph changes that would drop or replace its nodes.** A live edit that removes a loop's playback or capture node is refused; the running loop keeps its decisions.
2. **Node-pair swaps go through staging.** Replacing which physical nodes a loop uses is not a plain route edit. You stage it with `Operation::NodePairBegin` / `NodePairAppend` / `NodePairStatus`, then publish media, timing, and routes together with `ConfigurationActivate` for stopped edited owners or `ConfigurationReplace` for a complete fresh-owner replacement. `Options::default()` disables staging; enable it via `Options { node_pairs: NodePairSettings { candidates, sources, source_packets_per_callback }, .. }` when you need it.

For the common case — adding your own processors and wiring them around loops — no staging is needed:

```text
host in ──▶ capture ──▶ (loop) ──▶ playback ──▶ your FX ──▶ host out
```

Build that graph at prepare time in `GraphDescription` exactly as in chapter 2, adding your processor nodes and connections next to the loop's two nodes. The loop's `Binding` still names its playback/capture nodes; everything else is ordinary graph wiring.

If a running loop must move to different nodes (for example because you rebuilt your FX chain), stage the node-pair change instead of editing routes around it — the planner will refuse the unsafe edit and the staged path is the supported one.

## Prepare content, timing, and graph independently

Enable node-pair candidates/sources, topology candidates, and runtime graph
staging before callbacks begin. Upload and validate media sources as described in
[chapter 12](11-exporting-and-importing-media.md). A node-pair bank declares each
slot's previous owner and optional `NodePairInitial`: the fresh target, frame
count, audio/MIDI sources, gain, balance, and whether input state is preserved.
Appending a valid edit transfers its source ownership into that bank. A refused
append consumes none of those sources.

Stage a matching topology and graph, including both playback/capture decision
bindings for every primitive loop. Ready means prepared, not published. Keep the
expected graph and topology epochs; recheck both at activation. The runnable
[configuration cut example](../../examples/configuration_cut.rs) demonstrates
the staged lifecycle and coordinated media/timing/graph cut.

## Choose ordinary activation or complete replacement

`ConfigurationActivate` requires edited old owners to be stopped. Scheduling a
Stop does not establish that condition until Stop has actually executed.

`ConfigurationReplace` is the complete replacement variant. It may retire old
Playing owners at the publication cut, so the host can keep the old session
audible throughout preparation. Its stricter preflight requires:

- every assigned old owner is covered by the bank;
- every candidate owner has a fresh logical identity, even when physical slots
  are reused;
- media, lengths, topology, graph membership/bindings, epochs, and retirement
  capacity all agree;
- affected rows have no active/prepared recording, interval, or frozen take;
  capture groups are Idle and can release their retained content.

The operation is admitted only at callback start. After preflight it publishes
the fresh stopped owners, media, topology, and graph together, clears retained
Idle capture-group content, and requests runtime device MIDI cleanup at the same
cut. Old scheduled work cannot address the fresh identities. An ordinary refused
preflight leaves old playback and MIDI recovery unchanged; it is not implemented
as Stop followed by a fallible rebuild. This is a complete replacement, not a
way to replace an arbitrary playing subset or force a recording to stop.

Retained old exports remain readable independently of the new owners. For either
activation variant, recycle the applied node-pair bank and displaced topology,
and release the Taken graph stage. Before publication, cancel the node-pair bank,
cancel the topology stage, and release the Ready graph stage instead. Sources
already transferred to the bank are released by its cancellation. Host-owned
processor presets, file tasks, and reliable operation results have their own
lifetimes; the loop operation does not manage those application responsibilities.

The [configuration scenarios](../../tests/support/configuration_scenarios.rs)
exercise replacement while playing, recording refusal, retained old exports,
and unchanged recovery on a refused cut. The
[topology activation reference](../../docs/topology-activation.md) describes the
lower-level identities and cancellation/recycling operations in detail.

[Previous: Exporting and importing media](11-exporting-and-importing-media.md) · [Next: Capacities, refusals, and results](../part-7-operating/13-capacities-refusals-and-results.md)
