# Generated payload hint experiment

Both alternatives are rejected as behavior-preserving defaults. The original
early SP plus late JP policy remains selected. The comparison control is C1
(`6be27ce0`); canonical wrappers are `d09bd1cb`/`e6fce1e7` with identical HDL.

`READ_HINT_POLICY=2` preserves early and late hints and is the default. Value1
enables only late JP hints; value0 acknowledges late hints and emits no generated
hints. Public media-store hint handling and all read/write operations remain.
The intended contract preserves samples, MIDI, accepted commands, publication,
fault/reset and progress semantics; only speculative traffic and latency may
change. The failed fault checks are retained rather than redefining that contract.

The canonical default reproduces all 33 C1/B0 callback rows exactly and passes
all eight protocol cases. Every mapped resource count matches C1 exactly;
pre-route setup slack differs by 0.001 ns. All screens use the same pinned
connected flow at 100 MHz.

| Policy | Optimized LUT delta | FF delta | BRAM delta | Ordinary clocks | Playback start |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original early + late | 0 | 0 | 0 | 54,146 | 110,356 |
| Late only | -106 | -4 | 0 | 54,146 | 114,990 |
| No generated hints | -417 | -416 | -1 | 60,438 | 121,319 |

Both reduce steady payload requests from 256 to 128. Normal data/MIDI/state
agree exactly on all 33 short rows. Late-only's first warm quantum takes 59,979
clocks versus 54,809; no-hint takes 66,308 and adds 6,292 ordinary clocks.

Both fail the first-buffer publication assertion after an injected second-view
read fault in Instant, Fixed and DRAMsim3 modes. The first cached buffer becomes
ready before the later fault arrives. The probe waits for the actual failing
transaction before releasing its paused response; the original default test still
requires that fault to be in flight at the first scratch write. Neither assertion
is relaxed. Five blocked/Legacy cases pass for each candidate; a failing protocol
case stops before its remaining checks.

The optional `hint_policy_protocol` executable records each case's result and
actual source hashes. `midi_area_experiment` uses the same callback fixture/CLI,
preserving the frozen original executable. Its explicit new parameter allowlist
admits only values0/1/2. Failed variants have no routed-slice or long-boundary
qualification. FIFO capacity tuning is not pursued for a rejected policy.
All raw failures, samples, HDL, fixture sources and mapped reports are retained
in the `s2-hints-*` archives and summaries.
