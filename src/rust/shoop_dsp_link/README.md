# Portable asynchronous DSP links

`no_std` plus `alloc` contracts for timestamped planar audio/MIDI, prepared buffer
leases, bounded in-memory endpoints, frame-based returns and DSP block execution.
There is no graph runtime, wire format, OS clock, socket or process dependency.

`Descriptor::validate` checks shape, storage and a conservative frame latency.
`Endpoint` operations are memory-only; refusal retains ownership and release has
a reserved credit. `Buffer` exposes borrowed logical contents so a future mapped
lease need not expose Rust allocation pointers across a process boundary.

`service::Service` adapts logical quantum Q to DSP size B, even when neither divides
the other. `ProcessingBlock` is the shared prepared DSP execution component used
by both the logical service and the existing UDP stream. The reference carrier
uses `memory::pair` and `ReferenceService::poll`, called outside graph rendering.

See [the contract](../../../docs/remote-dsp/link-contract.md) for latency,
ownership, failure policy and the distinction between service lead and UDP N.
