# Native Marlin testbench suite

Target: `proto/fpga-rust-reference`, integrated through revision `7e855f70`.

## Goal

Run the entire RTL regression suite through Rust/Marlin with readable sequential
tests, shared transport/scheduling infrastructure and production Rust references.
Keep every protocol, fault, reset, ownership, numeric and integration check.

## Architecture

The optional `rt_fpga_testbench/sim` feature owns Verilator elaboration and the
dynamic port adapter, integer-picosecond clock scheduling, autonomous protocol
monitors, memory agents and reusable transaction drivers. Integration drivers
compose component drivers. Suites own stimulus and assertions. Production
comparisons call the real compiler and numeric references directly.

Python remains for RTL generation, hardware-image preparation, shared scenario
generation, an independent CPU float-accuracy check and native-runner aliases.
It does not drive HDL or implement a second memory service.

VCD is opt-in: normal runs and CI create no waveform files. Enable `--waves` or
an explicit trace path only for a focused investigation. The small trace-lifetime
regression opens two traces and removes them after checking timestamps/lifetime.

## Completion gates

- [x] Implement one difficult continuous bench and compare frames, states, media
  and counters exactly with the frozen baseline.
- [x] Port all 74 initial definitions and the new smoothing-history regression
  from the current target branch. Track all 75 implementations and Cargo targets.
- [x] Centralize clocks, ports, memory transports, protocol checks and transaction
  helpers; compose them for component and full-pipeline suites.
- [x] Preserve real DRAM timing, posted-write/drain accounting, failure injection,
  reset, reclamation stress, long sessions and 64-loop workloads.
- [x] Remove the retired Python HDL drivers, memory services, comparison runner,
  dependencies and framework references from source, docs and historical labels.
  Keep recorded measurement values and identify their historical provenance.
- [x] Move CI to native component/integration matrices, scale/long replay,
  independent float accuracy and generated-source checks. Keep synthesis gates.
- [x] Verify VCD defaults and document architecture, replay and extension points.
- [x] Complete regression, formatting, warning-denying build and source checks
  against the integrated target branch.
- [ ] Commit and push the implementation branch; open a PR targeting
  `proto/fpga-rust-reference`.

## Evaluation

The synchronous Verilator suite fits ordinary Rust transactions and autonomous
agents. Production encoders, compile paths and numeric oracles are reused directly.
The suite owns a small scheduler and dynamic port adapter; their width, instance,
protocol and trace-lifetime behavior has dedicated regression coverage. The
frozen continuous cases retain exact output and observable-counter parity.

Repeated runtime creation also exposed a native bridge lifetime defect. Library
initialization must not call the legacy global tracing initializer after an
owned context has been destroyed. Tracing is enabled on the new model's context
instead. The repeated-runtime regression and all 19 runtime tests pass.

The recovery campaign found a demo reset-handshake defect: the reset duration
must count only while `session_reset_ready` is asserted. The RTL template and
generated demo now enforce that condition. Fault placement waits for the freshly
loaded epoch so draining old work cannot consume the intended injected error.

The broad workspace run at `0f9a31b1` passed 3,246 tests, with 11 skipped and
two existing protocol byte-fixture failures. Those failures were reproduced on
the untouched baseline. Target update `7e855f70` fixes the protocol fixtures and
introduces portable runtime configuration and regular-host integration. The
merged solution preserves those host CI checks and uses the shared runtime build
configuration. The 75 baseline definitions and RTL sources are unchanged.


On the integrated target, the focused core/continuous/demo/smoothing checks,
19 runtime tests, 11 regular-application RTL tests and nine codec tests pass.
The complete native matrices passed all 28 Rust entry tests. Warning-denying
workspace/native builds, formatting, coverage/test policy, generated sources,
independent float accuracy and default dependency isolation also pass. Routine
CLI output and completed waveform regressions leave no VCD files.
