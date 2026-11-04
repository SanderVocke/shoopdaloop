# Timing and capacity limits

The runtime prepares its storage and work budgets before processing. Rendering
uses heap guards in regression tests; resource exhaustion and command saturation
have explicit outcomes. These checks establish bounded behavior, not a device
callback deadline guarantee.

Production tests cover audio/MIDI partition invariance, exact event offsets,
synchronized recording and delayed playback for multiple processing quanta.
Native callback adapters, the real browser worklet and protocol ownership have
separate integration coverage. Compare callback latency and xruns on the actual
host and device before relying on a particular audio configuration.

Historical measurements retain their original source and platform scope. They do
not describe current source automatically.
