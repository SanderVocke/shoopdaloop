# Fabric audio I/O

A lockstep frame interface connects arbitrary audio protocol adapters to quantum
processing. All lanes share frame timing; routing and loop grouping belong to the
graph. The demonstration uses generated input, the real processing core and an
observable output. There is no physical audio protocol or ARM audio transport.

## Interface contract

`audio_clk` clocks the interface; `frame_tick` identifies one sample instant.
`rx_samples`/`tx_samples` pack lane c in bits `[32*c +: 32]`, raw signed Q8.24.
Provide at least four audio-clock cycles between frame ticks for RAM prefetch.
The interface never backpressures a frame. `clk` is an unrelated processing clock.

RX quantum n contains frames `[n*Q,(n+1)*Q)`. Output quantum n begins at
`n*Q + Q + P`, for `1 <= P <= Q`; P=Q selects 2Q latency. The audio domain
chooses a complete output bank, or all-channel silence, for the entire quantum.
`frame_prepare` closes eligibility two audio-clock edges before the sample
edge, allowing synchronous RAM selection/read. A completion visible at that
prefetch edge is eligible; one visible later is late. Startup before Q+P frames is expected silence.

Input/output banks remain private until descriptor publication and are never
reused before acknowledgement. Per-bank two-phase request/acknowledge mailboxes
form the bounded descriptor rings. Payload metadata remains stable from request
until acknowledgement; only request/acknowledge bits cross synchronizers. This
replaces a general asynchronous descriptor FIFO: for two or three banks, the
mailboxes encode bank ownership directly and support arbitrary ring sizes without
binary/Gray FIFO pointer constraints. Sample words use dual-clock BRAM per lane.

The core has dedicated synchronous input reads and output writes indexed by
`channel*Q + frame`. A run handshake latches its input/output bank allocation.
Core completion releases RX and publishes TX only on whole-pipeline success.
The engine remains the sole authority for logical state and clock advancement.

Commands are admitted in a bounded sequence-tagged fabric queue. A command's
quantum is its logical execution boundary, independent of an output underrun or
backlog. Prepared setup is loaded before arm. The demo's finite loader/command
ROM comes from graph compilation; it does not compute sample results.

## Failure and reset contract

Output deadline misses discard only output: finish processing, preserve recording
and state, and continue in input order at fixed playback times. An input bank
collision, malformed sequence, command overflow, watchdog expiry or core/memory
fault requests recovery and stops admission. All outputs become silent.

`recovery_requested` is sticky until coordinated reset. Recovery first quiesces
memory, then resets both I/O domains and the core, reloads a prepared image, and
rearms. Both clocks must run during reset; assert the common reset in both domains
and release it synchronously in each domain. Reset clears control/descriptors,
not sample RAM. A new epoch is assigned on rearm. Old AXI responses must have been
retired before core reset; I/O epoch tags do not make an unsafe AXI reset safe.

The physical audio frame counter continues while stopped/recovering; session
frame zero is the arm boundary. Rearm is forbidden before the reload/quiescence
handshake. Losing input does not silently advance an incomplete session.

## Initial profile

Q=32, chunk=32, 16 RX/TX lanes, three RX/TX banks (12 KiB logical samples).
Two-bank profiles and Q=16/64 are required checks. Local transport tests also use
Q=1 and odd Q independently of supported processing/media profiles.

The 16-loop graph maps lane c to input port c, a mono capture/playback view for
logical loop c, and output port c. Input/Output and BeginCapture/Capture/ReadView
remain ordinary jobs. Monitoring and playback fan-in is an explicit graph mix.
