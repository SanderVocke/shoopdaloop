# Production Rust / RTL verification results

The production Current backend now supplies both floating-point and Q8.24
execution. The integrated RTL pipeline is compared against that production path
from common logical sessions. The parallel FPGA Rust backend, its app selector,
CLI, schemas and execution fixtures have been removed. The coverage disposition
is in [MIGRATION.md](MIGRATION.md); setup and replay commands are in
[README.md](README.md).

## Behavioral and numerical gates

Every output/media word and supported discrete observation is compared exactly
between production Q8.24 and TP/SP/JP/media/cache/AXI RTL. The driver lowers the
production compiler's ordering once; it does not feed oracle jobs or decisions
to hardware each quantum. Cases cover capture, playback, retakes, retained
exports, wrapping, seek/retrigger, gain/ramp interruption, mixing/fan-out,
scheduling, sync chains/cycles, cancellation, conflicts and completion retention.

[accuracy-gates.json](accuracy-gates.json) records directed/seeded and long
campaigns, alternate geometries, then integration with DDR metadata and 32-chunk
allocation blocks. Repeated scenarios are counted per campaign, not as unique
features. Additional final coverage includes:

| Gate | Coverage | Result |
| --- | --- | --- |
| DDR metadata integration | 44 sessions, 835 quanta, stalled AXI64 | Exact |
| Block allocation integration | 18 sessions, 214 quanta, stalled AXI64 | Exact |
| Long capture/playback | 328 quanta, 16,640 output/media samples | Exact |
| Stereo gain/fan-out | 8 quanta, ramps of 0/1/33/129 samples | Exact |
| Continuous fabric I/O | 16 lanes, Q32/P17, fan-out/monitoring, stalled AXI64 | Exact audio/media/state at 49-frame startup latency |

Float remains the ultimate numerical oracle. Each common session runs through
float and fixed production execution; checked error budgets follow actual
arithmetic order, conversions, truncations, float rounding and ramp interruption.
The long capture case has maximum error 5.7742e-8, RMS 3.6187e-8, signed bias
-9.1198e-9 and first-to-last drift 1.2224e-9, within its 6.2281e-8 bound.
Intentional saturation is audited and reported separately, including intermediate
clipping when the final float value is in range. Float tolerances never weaken
the fixed/RTL comparison. Contract tests deliberately inject both one-bit RTL
mismatches and above-budget float errors.

[subsystem-gates.json](subsystem-gates.json) records the migrated TP, SP, JP,
media, resolver, cache and AXI32/64 configuration matrix. No full substitute
execution model supplies those expectations. Extra gates retain grouped block
ownership/retirement, queued/in-flight prefetch invalidation, dirty forwarding,
bus errors, stale generations, exhaustion, old-response reset and sticky silence.
A watchdog test's alternate-clock stimulus race was fixed without changing RTL.

## Production and integration validation

[final-gates.json](final-gates.json) records commands, counts, configuration and
provenance. Warning-denying native workspace and relevant Wasm/worklet builds
pass, as do arithmetic/schema, affected production, heap, dependency-boundary,
formatting and test-policy checks. Wasm compilation is not a browser test result.

The complete native run executed 3,200 tests: 3,180 passed, 20 failed and 11 were
skipped. Nineteen failures passed focused reruns with competing builds stopped.
One LAN profile-publication test still exceeded its 15-second deadline; the same
test also failed with the original `e5c015be` host binary. Alternating startup
measurements of that original host also exceeded the tests' 10/15-second
deadlines (16.3–27.6 seconds); debugger samples show unchanged MIDI checkpoint
initialization. These are recorded validation limits;
the full native run is not reported as green. No deadline was increased or test
disabled to suppress the failures.

## CPU, RTL area and DDR evidence

[cpu-final.json](cpu-final.json) and [PRODUCTION_NUMERICS.md](PRODUCTION_NUMERICS.md)
record paired release measurements. Default-float median after/before ratios
are 1.003 playback, 0.955 record/playback, 0.941 recording and 1.008 clearing.
All measured callbacks have zero allocations, reallocations and frees. The
64-mono-loop Q32 production workload measures median 56.628 us float versus
89.1675 us Q8.24, a 1.575 ratio, on the same shared host. Scheduler outliers are
retained; these measurements do not establish a worst-case deadline.

Local lint, generic synthesis and ECP5 mapping passed before the last upstream
changes; their unchanged synthesis evidence is retained alongside upstream
reports. The [block32 estimate](../media_store/measurements/blocks32/resources.json)
is 29,433 LUT4, 16,141 FF, 94 DP16KD and four multipliers for its documented
eight-loop configuration. The separate
[16-lane I/O checkpoint](../audio_io/measurements/validation.json) records its
own source/configuration and area. Neither is placed/routed timing, a latest
combined-top remeasurement, or evidence of Zynq-7010 fit.

[ddr64.json](ddr64.json) preserves the measured 64-loop Q32 DRAMsim3 run from
before the DDR-metadata change: 14,879 cycles cold and 13,327–13,350 thereafter
at an assumed 100 MHz, with roughly 2.88% steady-state data-bus utilization.
Its JSON records AXI width, DDR profile/model hashes and stage timing. This uses
production ordering and includes TP. The user explicitly waived repeating it
after upstream changes; no new 64-loop timing measurement is claimed. A smaller
live DRAMsim3 production comparison passed with DDR metadata; subsequent block
and I/O integration was checked with stalled fixed-latency AXI memory. The
upstream I/O report separately retains its measured live-DRAM case.

There is no MIDI, FX or composite-loop RTL claim in this profile. Those features
remain available through normal production float execution. Board deployment,
ARM transport and physical timing closure remain separate work.
