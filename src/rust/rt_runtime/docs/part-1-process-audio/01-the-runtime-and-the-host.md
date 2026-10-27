# 1. The runtime and the host

[Contents](../README.md) · [Next: Your first audio graph](02-your-first-audio-graph.md)

`rt_runtime` runs graphs of audio and MIDI processors over buffers supplied by your application. It handles routing, sample-frame scheduling, live graph changes, and prepared storage for recorded or uploaded media. It does not open devices, create threads, or provide playback DSP.

## The two sides

The **control side** prepares processors and graphs, submits changes, and collects results and objects returned by the runtime. It can run on an application or worker thread.

The **render side** owns the running graph and processors. The host calls it once per audio callback with borrowed input/output buffers. There is no internal thread or background executor.

```text
Control side                             Render side

prepare processors and storage
compile graphs, submit changes --------> validate changes and render
collect results and old objects <------- return results and ownership

Host/device buffers -------------------> process -------------------> outputs
```

The control endpoint has one producer. An application with several command sources must serialize them on the control side. The runtime likewise has one render owner; its processors are not concurrently accessible from the control side.

## Processors and graphs

A **processor** declares audio/MIDI ports and implements the work done on each processing call. A **graph** connects these ports to one another and to host ports.

Signal graphs are acyclic. Audio fan-in sums signals; routing adds no extra block of latency, clipping, or normalization. The runtime orders processor calls by signal dependencies. Feedback, automatic delay compensation, resampling, and codecs are outside its scope.

The runtime owns absolute sample frames. An optional [planner](../part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md) settles domain decisions before each signal traversal; processors borrow those decisions for their interval. Planning dependencies are validated separately from signal routes. Ordinary processors use `NoPlanner`, as in the next chapter.

Processor slots and port signatures are prepared up front. Connections can change while running, and processors can be replaced within those prepared signatures. [Live graph edits](../part-2-control-a-running-graph/06-routing-and-live-graph-edits.md) cover those operations.

## Prepare, run, stop

```text
PREPARE                         RUN                         STOP

validate limits                 submit/collect on control   stop submissions
construct processors            process borrowed buffers    settle needed work
allocate and initialize pools   deliver audio/MIDI          stop/join callbacks
install initial graph           repeat                      destroy owners
```

Preparation reserves storage for the configured maximum block size, processors, routes, queues, MIDI data, and resources. Limits cannot be resized while running. More prepared storage and more work allowed per callback are separate choices. Internal intervals do not renew callback allowances; [Capacity, work budgets, and diagnostics](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md) covers sizing.

Changes submitted during operation produce completions. Those completions can also return replaced objects, so the control side can destroy or recycle them without freeing memory in the callback. They are not disposable notifications: unread results occupy capacity. [Commands, completions, and ownership](../part-2-control-a-running-graph/05-commands-completions-and-ownership.md) explains this exchange.

Before destroying the runtime, stop and join callbacks. Applications needing graceful completion must explicitly settle domain work and collect results while callbacks still run. Disconnect or destruction does not synthesize Stop, complete an interrupted interval, or deliver device cleanup. There is no blocking flush; see [Stopping, disconnecting, and reclaiming ownership](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md).

## The real-time contract

Runtime callback paths use prepared storage without heap allocation or freeing, locks, waits, or external I/O. Processor implementations and host callback code must obey the same restrictions. Prepare working buffers before callbacks, retain no borrowed callback data, and handle capacity refusal explicitly.

Work depends on the configured block/graph sizes and on processor and planner behavior. Prepared storage does not guarantee that arbitrary DSP will meet a deadline, and the crate does not promise hard-real-time scheduling. Measure the complete callback on the target host.

Use `executor::GraphRuntime` to run processors. The lower-level `RenderRuntime` validates host audio blocks and advances a clock, but has no graph execution.

[Next: Your first audio graph](02-your-first-audio-graph.md)
