# FPGA component retirement validation

Baseline: `247c4974` on `proto/fpga-rust-reference`. Implementation branch:
`feat/fpga-retain-execution-components`; PR:
https://github.com/SanderVocke/shoopdaloop/pull/998.

Environment: Debian 13, Rust 1.100.0, cargo-nextest 0.9.116, Verilator 5.032,
Yosys 0.52+45 (`ea2c1e7ac`), CMake 3.31.6 and pinned DRAMsim3. Toolchains and
missing native development libraries were installed into workspace-owned paths;
no environment-specific paths or workarounds were added to production code.

## Completed checks

| Check | Result |
| --- | --- |
| Original compiled SP, JP and Marlin core baseline | Passed |
| Extracted SP/JP/core/smoothing checks, Rust warnings denied | Passed |
| Complete retained Marlin suite (`--features sim`, Rust warnings denied) | Passed: 20 tests, zero failures |
| Fresh worktree, fresh Rust target and RTL cache: SP, JP, core and complete TP-free pipeline matrix | Passed; covers explicit instructions, exact samples/state/media, partial faults, reset, instant/fixed/DRAMsim3/legacy memory |
| Additional small pipeline replay | Passed: fixed-latency AXI64 and DRAMsim3 AXI32 |
| Production CPU Rust checks | 4 passed, including canonical graph ordering independent of control activity |
| Python CPU schema/numeric checks, ordinary unittest entry point | 6 passed: codecs, raw integer bounds, clipping/error detection, routing order, successive gains, recording/playback |
| Application wire codec after version bump | 9 passed; golden bytes updated to version 38 |
| Warning-denying workspace build | Passed before and after final numeric helper changes |
| SP and JP standalone lint/generic/ECP5 synthesis | Passed |
| Retained AXI32/64 tops and TP-free execution top | Lint and generic/ECP5 synthesis passed |
| DRAMsim3 dependency build in both checkouts | Passed |
| Coverage inventory | All 75 baseline definitions accounted for: 62 retained, 13 explicitly retired |
| Rust formatting/test-attribute policy, generated wrappers, workflow YAML | Passed |
| Source/dependency audit | No active TP, old ARM/app adapter or simulator dependencies; default application has no Marlin dependency |
| Latest observed CI default dependency isolation | Passed |
| Native session on a 1 MiB stack | Passed; native startup function separation and its original regression test retained |
| Serial LAN acceptance | All 9 passed |

The SP/JP/media/cache RTL, production `rt_runtime`/`shoop_loops`, and local Marlin
patch have no implementation changes relative to the base. Removal focuses on
TP, its coordinators and ARM/application interfaces. Simulation seeds/configurations
remain in the retained suites (notably 20261110 and 20261112).

## Long regression runs

The complete retained Marlin suite passed: 20 tests, zero failures, including
all retained component and TP-free pipeline test targets.
The broad native run executed 3,240 tests: 3,221 passed, 19 failed and 11 were
skipped. The failures were investigated rather than hidden:

- Two stale protocol golden fixtures were corrected; all nine codec checks pass.
- A legacy FX timing failure passes in isolation. LAN readiness timeouts under
  concurrent load pass in the complete nine-test serial LAN rerun.
- A concurrent default workspace build had replaced the test-enabled host binary,
  invalidating 14 recovery checks with `unknown field test_signal`. Rebuilding the
  testing binary and rerunning the host package removes that setup failure.
- The host-package rerun passed 57 of 63 tests, with five LAN readiness timeouts
  (subsequently all passing serially) and one launcher-shutdown failure. That exact
  launcher test also fails on untouched upstream `247c4974` with a refused/closed
  management connection. No out-of-scope workaround was added.

Validation also caught native startup flattening during removal. The native
helper and original 1 MiB stack regression test were restored; the focused test
and warning-denying host build pass. No test deadlines were increased. All nine LAN tests subsequently passed serially.
The remaining launcher-shutdown failure reproduces on the untouched upstream base.

CI is checked for failures attributable to this change. An earlier Windows debug
job compiled successfully but failed packaging; its detailed log download was
unavailable. The native stack fix is pushed for renewed CI; the latest observed Windows job
was building the application and had no reported failure. Unrelated failures
and pending checks do not require globally green CI. Review is not a completion gate,
and the PR is not merged automatically.

No new Vivado place/route or physical-board validation is claimed. Earlier board,
clock and full-loop measurements remain historical evidence, not current system
acceptance results.
