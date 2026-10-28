# 1. The loop engine and the host

[Contents](../README.md) · [Next: Preparing an audio loop](02-preparing-an-audio-loop.md)

`shoop_loop_engine` records and plays audio/MIDI loops over buffers supplied by your application. It handles loop positions, wraps, scheduled transitions, synchronization, and prepared arrangements of other loops. It runs on `rt_runtime`; it does not open an audio device, create threads, or provide a user interface.

We will start with one stereo loop. It records three sample frames, plays the recording immediately, repeats it, and stops:

```text
Absolute frame:  0   1   2   3   4   5   6   7
Mode:           recording   playing         stopped
Loop position:  0   1   2   0   1   2   0   —
                            ^           ^
                            play        wrap
```

All of this can happen inside one eight-frame host callback. The engine divides processing at the transitions and wrap; the host does not need to make a separate call for each one.

## Control and rendering

Preparation returns two objects:

- The **controller** submits requests and collects their results. Keep it on the control side of your application.
- The **engine** owns the running loops and their media. The host calls `Engine::process` with borrowed input/output buffers.

```text
Control side                            Render side

prepare loops, storage, and routes
Controller: submit Record/Play/Stop ───► Engine: settle timing, process buffers
Controller: collect results        ◄─── Engine: report admission and outcomes

Host input buffers ───────────────────► Engine ───────────────────► host outputs
```

There is one control producer and one render owner. A live application can put them on different threads, but it must not access the engine concurrently. Our offline example alternates submission, processing, and result collection on one thread.

## One loop, two signal roles

A loop has one musical identity and timeline. Its **capture** role receives the signal to record; its **playback** role sends recorded content into the graph.

Keeping these roles separate lets you choose what is recorded and what is heard. The first example connects host input to capture and playback to host output. There is no live-input monitoring route, so the output is silent while the empty loop records.

The engine's resolver decides the loop's mode and position before processing each interval. Playback and capture follow those decisions; they do not maintain competing loop clocks. You use the integrated `runtime::Engine` here, not a separate resolver that you must drive yourself.

## Prepare, process, stop

Prepare storage, loop definitions, and signal routes before rendering. The engine starts stopped at absolute frame zero. Submit the desired transitions, then supply contiguous host blocks through `process`. Requests use absolute sample frames; loop positions are relative to recorded content.

Collect both admission completions and musical execution results. Successfully submitting Play is not proof that the loop could play—for example, an empty loop has no content yet. The example checks the actual outcomes as well as the samples.

Callback code uses prepared storage and must not allocate or free heap memory, lock, wait, or perform external I/O. Your own processors and host adapter must follow the same rules. Storage limits do not guarantee that every workload meets an audio deadline.

For shutdown, settle any required musical work while callbacks still run, then stop and join the callback owner before reclaiming engine storage. The example executes Stop and explicitly cleans up its stopped engine. Destruction alone does not send device output or turn pending requests into successful transitions.

The next two chapters build and run this example. Familiarity with the [runtime's audio graph](../../../rt_runtime/docs/part-1-process-audio/02-your-first-audio-graph.md) is helpful, but no custom processor is needed.

[Next: Preparing an audio loop](02-preparing-an-audio-loop.md)
