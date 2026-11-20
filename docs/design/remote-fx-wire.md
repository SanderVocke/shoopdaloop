# Remote FX wire protocol

Remote Carla uses a versioned TCP control connection and a dedicated UDP socket pair per prepared chain. The looper owns time and instantiates the graph with the accepted timing. The desktop service applies Carla processing to complete batches in source order.

## Control

TCP records carry a 32-bit little-endian byte count followed by UTF-8 JSON. A version 1 greeting selects service inspection, local-identity shutdown, or chain preparation. A chain request includes the sample rate, quantum and batch lengths, slack in quanta, ordered audio widths and MIDI limits for both directions, maximum datagram size, and copy count. The service echoes the exact accepted configuration or returns a refusal. The initial records have a 4 KiB limit. Opened stream control records have a 23 MiB limit to accommodate encoded Carla checkpoints.

The accepted reply includes the UDP port and a randomly generated 128-bit stream identity. The client proves the return address by sending the identity to the UDP port and requiring its echo. The server checks the UDP source IP against the TCP peer and connects its UDP socket to the proven address before accepting media. Only trusted network endpoints should expose the service. The identity fences streams and provides return address verification.

Control requests carry strictly increasing request IDs. The service returns the cached response for a repeat of the most recent request on a connection. Status, activation, editor visibility, and checkpoint capture and restore share this path. Replies include confirmed activity, lifecycle, editor visibility, and stream counters. The client polls status while idle. An expired control lease releases the slot. The client reconnects under a new stream identity and restores confirmed state before resuming media.

## UDP media

Integers and PCM samples are little-endian. The default maximum UDP payload is 1,200 bytes and the default copy count is two in both directions. Each quantum has its own fragments. Both copies of a fragment carry identical bytes.

| Field | Size in bytes |
| --- | --- |
| Magic and version | 4 |
| Stream identity | 16 |
| Buffered generation | 8 |
| Batch sequence | 8 |
| Batch source frame | 8 |
| Quantum index within the batch | 2 |
| Fragment index within the quantum | 2 |
| Total quantum payload length | 4 |
| First batch sequence for this stream and generation | 8 |
| Fragment payload | Up to the accepted datagram size minus 60 |

A quantum payload contains the ordered planar float32 audio lanes followed by MIDI events. Each MIDI event has an unsigned 16-bit frame offset within the quantum, a 16-bit byte count, and that many bytes. The receiver checks the negotiated widths and per-quantum MIDI limits and only submits a fully assembled batch. The batch tag in each return matches the corresponding source tag.

The service drops stale or malformed datagrams and deduplicates fragments within a bounded receive window. A gap that survives the reorder interval advances the window. The next complete batch reaches the worker with its original source identity so Carla can perform its discontinuity cleanup. The looper releases wet audio on its prepared schedule, applying its existing missing-output and MIDI cleanup behavior if the result is unavailable. Peer clocks are used for local expiry only. [Buffered release contract](../../src/rust/rt_buffered/README.md#worker-transport)
