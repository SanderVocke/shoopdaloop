# Workload controls

The fixed playback matrix uses the same connected SP/JP top and compiled
64-stereo-loop profile for audio, MIDI, and mixed traffic. Each case runs with
MIDI tracking off/on and added DDR response delays 0/7. It records the playback
start, sixteen warmup quanta, and sixteen steady quanta separately. Audio-only
tracking on/off intentionally executes equivalent work. Preserve all 33 rows,
including traffic and cache counters; a median alone does not establish equality.

The original short playback fixture remains unchanged. It is not a cache-conflict,
capture-transition, or long-span benchmark. These directed controls supplement it:

| Purpose | Existing control | Execution policy |
| --- | --- | --- |
| Capture, playback, retained slices, refused mutations, protocol/reset | `cargo test -p rt_fpga_testbench --features sim --test media` | Run for storage changes and final regression |
| Full pools, block sharing, descriptor restore/duplicates, sparse IDs, reset with stale RAM | `--test ownership` | Run for occupancy, allocator, and ownership changes |
| Detached cleanup, bounded maintenance, faults, preemption, stale hints | `--test deferred` | Run for cache/ownership/controller changes |
| Radix levels, large arena, coherent cache and transport; 32/64-bit AXI | `--test metadata` | Run for cache, width, and address changes |
| Capture/repeat/seek/stop, expression restoration, quantum boundaries, AXI stalls | `--test midi_expression` | Run for MIDI changes and final regression |
| Concurrent upload and graph switches preserving recording/ramp/history | `--test loading` | Run for working-state, reset, and publication changes |
| Exact sample arithmetic, saturation, streaming/reset, production numerics | `--test audio --test smoothing --test state` | Run for arithmetic/precision changes |
| AXI bounds, strobes, retirement, faults | `--test axi` | Run for address/offload changes |
| DRAM timing, refresh, conflicts, queue/reset, physical accounting | `--test timing` and retained DRAMsim3 audio/MIDI examples | Final memory qualification |

Commands use the documented activated build environment and `--locked`.
These are required controls, not assertions that every suite has already run;
the ledger and per-candidate logs record actual execution.

Stage 2 additionally supplies deterministic set-conflict and cache/block/tree
boundary positions and either long playback through two 16,384-frame leaf spans
or equivalent directed boundary fixtures. New architecture and precision cases
are added alongside these controls, with independent contract-specific oracles.

Physical timing and functional simulation are separate evidence. B0 fails
100 MHz setup timing; nominal cycle-to-time conversions do not qualify a
666.667 microsecond callback deadline.
