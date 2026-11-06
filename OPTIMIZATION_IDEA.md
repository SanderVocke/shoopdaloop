# Optimization idea: a flat render program for the graph executor

Status: idea, not implemented. It targets worst-case render cost per quantum,
so it keeps a fixed 32-frame quantum and the same work every quantum.

## Problem

On a ZedBoard (Cortex-A9, 667 MHz), the per-node profiling table shows a floor
of about 0.4–0.5% of a 32-frame quantum (about 2,000–3,000 cycles) for nodes
whose own work is trivial, such as loop playback and capture. With one sync
loop plus one track of 8 loops (33 nodes, 69.5% overall):

- roughly 13% of the budget is per-node executor work counted inside node times,
- part of the further ~13% outside the nodes is executor and callback overhead
  (the planner and resolver account for the rest),
- the sampler shows the executor's generic code paths as a steady share:
  `run_graph`, `route_offset`, `Buffers::clear` (memset), `mix`, `AudioPorts::new`.

This floor is paid by every node in every quantum, so it scales directly with
the number of loops and strips.

## What happens today, per node and per quantum

`GraphRuntime::run_graph` (`src/rust/rt_runtime/src/executor.rs`) walks the
active plan's steps. For each step it:

1. clears all of the node's audio input and output buffers and MIDI ports
   (`Buffers::clear`), whether or not they are about to be overwritten;
2. routes every incoming connection through `route_offset`, which re-resolves
   source and target slots (`Option` checks, `split_at_mut`, error paths) and
   mixes audio or merges MIDI;
3. merges and clears injected and recovering MIDI ports;
4. builds `AudioPorts` (validating shapes with integer division) and a
   `ProcessBlock`, then calls the processor;
5. post-processes MIDI recovery, updates diagnostics counters one at a time and
   records a profiling timestamp.

All the lookups and validation are repeated every quantum, although they only
change when the graph changes.

## Proposal

When a plan is prepared on the control side (where allocation is allowed),
compile it into a `RenderProgram`: a flat sequence of operations with every
lookup already resolved. The render side only runs that sequence.

- **Resolved routes:** each audio route becomes a pair of precomputed buffer
  ranges. For each input port, the first route copies and later routes add, so
  routed inputs need no clearing. Only ports with no routes are zero-filled,
  from a precomputed list.
- **No blanket output clears:** processors either must write all their audio
  outputs (a documented contract, checked in debug builds and tests), or the
  program lists the ports a processor may leave unwritten and clears only those.
- **Validation at compile time:** slot existence, port shapes, `source !=
  target` and MIDI port ranges are checked once. The render loop has no
  per-edge `Option`/`Result` handling, only debug assertions.
- **Cached port views:** each step stores its prepared `AudioPorts` geometry
  (stride, counts, ranges), so building the view is arithmetic-free.
- **Aggregated bookkeeping:** diagnostics counters are added once per quantum
  (`nodes_processed += steps`). Per-node profiling timestamps stay, but only
  while profiling is recording.
- **Host routes keep a dynamic offset:** routes to and from host buffers still
  take the callback's `start` offset at run time. Everything else is static.

The program is built together with the plan and activated atomically with it,
at the same graph-epoch switch the executor already performs.

## Worst-case properties

Program length and work are fixed per topology: no data-dependent branches,
no event-driven skipping. A "perfect storm" quantum costs the same as a quiet
one, and the program can be timed exactly.

## Expected gain

On the ZedBoard session above: an estimated 12–22 points of the 69.5%
(about 1.35x), mostly by removing the per-node floor and the clears. The
relative gain should be similar on other cores; the absolute cost matters most
on small ones. It combines with node fusion (fewer, larger nodes): with a flat
program, fusion mainly saves intermediate buffer traffic.

## Risks and open questions

- MIDI merge order and recovery semantics must stay identical; MIDI probably
  keeps more runtime logic than audio.
- The "processors write all outputs" contract has to hold for every node kind,
  including hosted FX, or those must be listed for clearing.
- Memory for compiled programs per prepared plan, and the staging and rollback
  paths for plans that never activate.
- Profiling layout and per-node attribution must keep working.

## Incremental path

Each step can be measured on its own with the per-node profiling table:

1. Aggregate diagnostics counters and drop repeated per-step validation.
2. Precompiled route table with copy-first semantics, replacing clear + mix
   for routed inputs.
3. Skip output clears under the processor contract.
4. Cached port views and a prebuilt per-step `ProcessBlock` frame.

Before starting, measure with profiling off (the DSP bar) to see how much of
today's per-node floor is profiling instrumentation itself.
