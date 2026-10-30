# 1. The loop library and the host

[Contents](../README.md) · [Next: Preparing an audio loop](02-preparing-an-audio-loop.md)

`shoop_loops` records and plays audio/MIDI loops over buffers supplied by your application. It handles loop positions, wraps, scheduled transitions, synchronization, and prepared arrangements of other loops. It runs on `rt_runtime`; it does not open an audio device, create threads, or provide a user interface.

We will start with one stereo loop. It records three sample frames, plays the recording immediately, repeats it, and stops:

```text
Absolute frame:  0   1   2   3   4   5   6   7
Mode:           recording   playing         stopped
Loop position:  0   1   2   0   1   2   0   —
                            ^           ^
                            play        wrap
```

All of this can happen inside one eight-frame host callback. The runtime divides processing at the transitions and wrap; the host does not need to make a separate call for each one.

## Control and rendering

The loop library prepares parts: a **planner** that owns the loops' timing and media, the loops' **nodes**, and a control-side **client**. Your application installs the planner and nodes in an `rt_runtime` graph runtime, which returns two objects to the calling thread. Neither is attached to a particular thread:

- The **control endpoint** submits requests and collects their results. The loop client builds the loop's requests for it. Use both from your application's command-handling code.
- The **runtime** owns the running loops, their media and any other processors. Store it in your host's audio-processing state and call `GraphRuntime::process` for each block of borrowed input/output buffers.

```text
Control side                                  Render side

prepare loop parts, storage, and routes
Control endpoint: submit Record/Play/Stop ───► Runtime: settle timing, process buffers
Control endpoint: collect results        ◄─── Runtime: report admission and outcomes

Host input buffers ───────────────────────────► Runtime ──────────────► host outputs
```

“Control side” and “render side” describe responsibilities, not automatic thread placement. For a threaded audio host, your application moves the runtime into the audio callback's state—for example, into a Rust `move` closure—and keeps the control endpoint and loop client on the application thread. Each object has one owner; do not access the runtime concurrently. Our offline example keeps both on one thread and calls `process` directly.

## Prepare, process, stop

Prepare storage, loop definitions, and signal routes before rendering. The runtime starts at absolute frame zero with the loop stopped. Submit the desired transitions, then supply contiguous host blocks through `process`. Requests use absolute sample frames; loop positions are relative to recorded content.

### Callback rules

Callback code uses prepared storage and must not allocate or free heap memory, lock, wait, or perform external I/O. Your own processors and host adapter must follow the same rules. Storage limits do not guarantee that every workload meets an audio deadline.

For shutdown, settle any required musical work while callbacks still run, then stop and join the callback owner before reclaiming the runtime's storage. Destruction alone does not send device output or turn pending requests into successful transitions.

The next two chapters build and run this example: chapter 2 prepares a loop with two signal nodes (capture and playback), and chapter 3 submits the transitions and checks both the samples and their musical outcomes. Familiarity with the [runtime's audio graph](../../../rt_runtime/docs/part-1-process-audio/02-your-first-audio-graph.md) is helpful, but no custom processor is needed.

[Next: Preparing an audio loop](02-preparing-an-audio-loop.md)
