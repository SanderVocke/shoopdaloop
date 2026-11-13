# Offline application contract

The common version-1 trace in `fixtures/app-sync-loop.json` uses the actual
application service and UI client. It connects the logical Sync input and one
Sync output to Master, then exercises record/play, gain, mute, stop, waveform
selection, clear, another recording and a self-synchronized stop. Inputs are
integer-derived in-memory samples with peak below 0.125; logical progress is
explicit for replay. Nothing opens an audio device. Normal simulator execution
will run continuously at maximum speed; the replay bounds iterations for exact
comparison cuts and never sleeps to meet an audio rate.

`app::description()` retains Q=32, 48 kHz, a 240-frame control ramp, two prepared
strip channels and stereo Master. It bounds the prepared pool to one strip,
one loop and one Master bus, with no MIDI ports. The real service creates one
mono MIDI-free Sync strip and configures 3 ms (144 frames) of loop smoothing.
Unused prepared channels are distinct from the mono active width.

The native startup probe confirms eight graph nodes: playback, fader, strip
input, capture, strip output, bus input, Master output and the bulk reader.
Configuration arrives through NodePairBegin/Append, TopologyBegin/Append/Validate,
StageGraph and Configure before ConfigurationActivate. In the deterministic
probe, the first activation is requested at frame 96 and a later GraphActivate
establishes Master connections. Other block partitions can change request frames;
compare the same clock and honor the runtime scheduler's quantum rounding.
Default smoothing is applied after the loop exists. Candidate recycle, old media
retirement, state queries and owning graph release are part of startup.

Waveforms follow a single selected loop (`IconClicked`), not a targeted loop
(`IconDoubleClicked`). The default Sync strip has no routed sample input/output;
the replay uses public connection actions before asserting nonzero audio.
The actual control-side submissions and graph/payload descriptions are captured
under the engine's optional `testing` feature. Instrumentation is absent from
normal engine builds. `work/fpga-app-contract` contains native probe results.

## Numeric contract

Hardware and exact expectations use production Q8.24 arithmetic, preserving
operation order, clipping, ramp time domains and coefficient construction.
Input float samples and gain parameters convert once at their declared backend
boundaries; media reads convert Q8.24 samples to ordinary float encoding.

For the common bounded trace, compare native float output to decoded fixed output
with maximum absolute error 0.00002 and RMS error 0.000005. These conservative
bounds allow propagated Q8.24 coefficient/sample rounding through up to eight
amplitude stages with gain at most two, plus smoothing. For each multiply the
absolute-error bound propagates as `|gain| * prior_error + |sample| * gain_error`
plus one fixed-point LSB and native float rounding; sums add their operand bounds.
Validate actual graph stages and parameter bounds against that budget. Extend
its derivation for a wider workload instead of increasing thresholds in response
to a failing comparison. Exact silence/identity cases and intermediate Q8.24
production-vs-RTL operations retain bitwise comparisons. Saturation, boundary
frames and lifecycle results are checked separately and cannot be hidden by the
float tolerance.
