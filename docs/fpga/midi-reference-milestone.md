# Rust reference MIDI milestone: validation

Implementation scope and API are described in the
[component guide](../../src/rust/rt_fpga_reference/src/midi/README.md).
The execution checklist is in [the plan](../../midi_RTL_new.md).

## Local checks

Linux x86-64, five virtual CPUs reporting Intel Xeon Platinum 8573C; Rust 1.100.0.
Rust compilation used `RUSTFLAGS="-D warnings"`. The full native validation used
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and
`CARGO_INCREMENTAL=0`, matching the FPGA CI resource settings. Assertions remain
enabled. The ordinary debug workspace build had also succeeded; building every
native test with debug symbols exceeded the workspace disk capacity.

| Check | Result |
| --- | --- |
| Reference and compiler, all reference features | 99 passed, including 25 new MIDI tests |
| Workspace build | Passed; existing Cargo manifest unused-dependency notices remain |
| Formatting, test attributes, feature parity | Passed |
| Complete native nextest suite | 3,456 passed, 13 failed, 11 skipped |

The complete native command was:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

The environment flag permits existing tests to decline unavailable host
facilities. Passing this run would not certify physical MIDI or audio hardware.
No RTL simulation, synthesis, or board measurements are claimed here.

The failures are in unchanged native app/host paths. Twelve application checks
fail while starting the native engine with `Runtime(Capacity)`; the representative
`a_new_connection_cancels_old_inputs_and_cannot_consume_old_file_callbacks` fails
in ten isolated runs too, and reproduces on untouched base commit `9eb71d04`. The host test
`siblings_survive_the_launcher_exit_and_later_control_needs_no_child_handle`
fails while polling shutdown results after the endpoint closes; it also fails in
ten nextest stress iterations and reproduces on untouched base commit
`9eb71d04`. These checks are not reported as passing, and no
test or timeout was changed to hide them. The prototype is not enabled in these
native execution paths.

## Resource and CPU sample

Command: `cargo run --locked --release -p rt_fpga_reference --example midi_profile`.
This records and reads 8,192 events, at 256 events per 128-frame window, applying
their note updates. Input construction and preparation are excluded. Each of 101
trials starts with the same state. This is a shared-host CPU microbenchmark;
it does not include audio execution, output transmission, or initial restoration.

| Observation | Measured value |
| --- | --- |
| Prepared engine with 8,192 record slots | 158,224 bytes |
| One state bank | 8,976 bytes |
| Median capture/read/state time | 276,600 ns |
| Minimum / maximum | 234,877 / 3,763,353 ns |
| Ordinary note / pedal release / System Reset | 1 / 129 / 4,226 update steps |
| Cleanup / restoration / emergency recovery | 2,096 / 10,433 / 32 visited fields |

The boundary sample runs after System Reset: cleanup emits zero messages,
restoration emits 65, and recovery emits 32. Occupied notes and known controllers
change message counts. Refused output adds retries. Field/update steps express
software work, not hardware clock cycles, and the timing sample establishes no
worst-case callback deadline.

Integration tests compare exact note/control ordering at Q=1/16/32/64/128,
single/multiple-quantum calls, silent takes, synchronization waits, direct play-to-record transitions,
initial state, stop/repeat cleanup, exhausted storage, and output refusal. Cleanup
can resume with one accepted message per call. Both ordinary and partial-restore
pedal ownership survive emergency recovery. Heap guards cover processing and
recovery after preparation; independent traces and a held-key map supplement
comparison with the existing state implementation.

Per the user's updated instruction, CI results and review feedback are excluded
from this milestone's completion criteria and are disregarded unless requested.
