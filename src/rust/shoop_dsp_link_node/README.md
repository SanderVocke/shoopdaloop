# rt_runtime DSP-link processor

`RemoteNode<E>` implements `Processor<L,Q>` for a portable link `Endpoint`. It
accumulates partial input blocks, accepts bounded returned work and commits each
output block at its fixed graph-frame offset. Missing output is silent with MIDI
recovery. Late results never patch a previously committed block.

Prepare the link and node outside rendering. `prepare_observed` additionally
returns a bounded observation consumer; a slow observer drops diagnostics rather
than blocking rendering. Network services and processor state changes remain
outside `process`. Destruction of the node and endpoint belongs to the runtime's
off-render retirement owner. Runtime graph epochs do not reset link state.

Both crates build without `std` on Cortex-A32/A64 and Cortex-M. Native network,
future IPC, and firmware services bind the same endpoint contract. No board driver
or automatic graph delay compensation is supplied here.
