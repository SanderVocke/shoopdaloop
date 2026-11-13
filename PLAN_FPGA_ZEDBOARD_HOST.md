# Drive the FPGA core from the ZedBoard ARM

Status: goals and acceptance criteria approved 2026-11-13; in progress. Branch
`feat/zedboard-fpga-hardware-backend`, targeting `proto/fpga-rust-reference`.

## Goal and scope

Run the RTL backend core in the ZedBoard PL under the control of the FreeRTOS
firmware on the PS, with the desktop app attached over the existing network
protocol. The ARM takes the role that the Verilator `Simulator` plays in the
Marlin app integration: it runs `rt_fpga_adapter` and drives the core's ports.
The first milestone matches the Marlin app demo's single-loop feature set.
Later work extends the Marlin and hardware deployments together.

Not included: audio or MIDI I/O, the codec, an ARM audio data path, real-time
deadline pacing by an audio clock, SD/SPL deployment of bitstreams, multi-loop
features beyond what the shared adapter supports, and timing closure work.

## Investigation summary

- **App side.** The app stack needs no changes. `shoop_host_freertos` already
  composes `shoop_app::Service`, `RtBackend` and the in-process carrier to
  `shoop_rt_engine`'s control endpoint. Execution is swappable through
  `shoop_rt_engine::prepare_with_consumer`. `rt_fpga_sim`'s app worker already
  uses it with `rt_fpga_adapter::engine::Adapter`.
- **Adapter seam.** The adapter drives hardware through the `Device`,
  `LoopDevice` and `EngineDevice` traits. Their only implementation is
  `rt_fpga_sim::Simulator`, which also holds hardware-independent logic: load
  sequencing, instruction/value packing and run handshakes. The adapter crate
  uses `std`, though only for metrics `Instant`, `BTreeMap` and the serde/schemars
  image derives. The firmware is `no_std` + `alloc`.
- **RTL.** `loop_pipeline_axi` exposes valid/ready ports: run, transition command
  and response, cfg, audio_cfg, store_cmd/store_cfg, status, plus an AXI3 master.
  Host traffic is accepted only between quanta. No AXI slave, register map or
  interrupt toward a CPU exists. The benchmark proved the core's AXI3 master
  against PS DDR through HP0. It generates input in the PL and digests output.
- **Firmware.** The firmware has no PL access. Its ZedBoard DDR map leaves no room
  for the core's arena at `0x01000000`. The render task is paced by the FreeRTOS
  tick in 256-frame steps. The PS init matches AMD's ZedBoard preset, the same
  one the bitstream build now uses, with FCLK0 at 100 MHz.
- **CI.** The Zynq QEMU workflow does not trigger for PRs to
  `proto/fpga-rust-reference`. The FPGA simulation workflow ignores
  `rt_fpga_sim` and `rt_fpga_adapter` changes. Marlin's app tests are not in CI,
  and Marlin builds only on Linux and macOS.

## Target structure

```text
desktop app ──TCP, unchanged app protocol──▶ FreeRTOS firmware (PS)
  Service → RtBackend → in-process carrier → ControlEndpoint
    prepare_with_consumer → rt_fpga_adapter::engine::Adapter<PortDevice<Bridge<Mmio>>>
                                          │ AXI-lite on M_AXI_GP0 (0x4000_0000)
PL: host bridge (generated register windows over the core's ports, status, counters)
    → loop_pipeline_axi (EXTERNAL_IO=1, PL test-signal source, digest sink)
    → AXI3 master → S_AXI_HP0 → reserved DDR arena
```

Rust layering, all in `rt_fpga_adapter` unless noted:

| Layer | Responsibility | Implementations |
| --- | --- | --- |
| `PortDevice<P>` | Device traits: load, run, packing and verification, moved from `simulator.rs` | one |
| `PortIo` | One transaction per core port: tp, cfg, audio_cfg, store_cmd/store_cfg, run, reset | Marlin pins in `rt_fpga_sim` (existing top); `Bridge<B>` |
| `Bridge<B>` | Port transactions over the generated register map | one |
| `Bus` | 32-bit register read/write | Marlin AXI-lite pins in `rt_fpga_sim`; volatile MMIO in the firmware |

The board then runs exactly the driver code that Marlin runs against the bridge
top. Board-specific code is limited to MMIO access, PS/PL bring-up and firmware
composition.

Implementation note (stage 1): `PortDevice` is the shared driver in
`rt_fpga_adapter::port`. It provides blanket `Device`, `LoopDevice` and
`EngineDevice` impls for every `PortIo`, so port implementations need no
wrapper type and existing simulator call sites stay unchanged.

### Existing FreeRTOS CPU engine

Keep it as a firmware build variant next to the FPGA variant.

- **Code retained for it.** Only the render glue is unique: tick pacing and the
  render cap in `host.c`, and `Dummy`/`render_step` in `embedded.rs`, about 300
  lines. The rest is shared:
  - `shoop_rt_engine`, `rt_runtime` and `shoop_loops` are production desktop
    code, and the FPGA variant links them too, for the control endpoint and
    the adapter.
  - One execution task, generic over the CPU render owner or the FPGA adapter,
    replaces the render-specific task.
- **Benefits.**
  - The CPU variant keeps the existing QEMU suite covering everything the two
    variants share: network, sessions, app protocol and control endpoint.
  - On the board, it gives an A/B check. The same firmware, network and session
    run with execution swapped, which separates FPGA faults from firmware or
    app faults.
- **Cost.** The existing QEMU CI time, which is already paid today.

QEMU↔Marlin co-simulation is complementary, not a replacement. A patched QEMU
device would forward the GP0 window over a socket to a host process serving the
bridge top through the same `Bus` layer. This would put the FPGA variant under
CI without hardware. It needs a QEMU patch, a forwarding server, `icount`-style
time decoupling and long timeouts, because Verilator quanta are slow. It is
therefore an optional stage after the hardware path works.

## Immutable acceptance criteria

1. On a ZedBoard with the host bitstream, the FPGA firmware variant:
   - accepts the desktop app over TCP;
   - executes the Marlin app demo's single-loop feature set: record, play,
     stop, gain/balance, loop state and position, and meters;
   - performs all audio processing in the PL.
2. Audio is generated and sunk in the PL. The ARM has no audio data path.
3. Marlin simulation and the board run the same bridge RTL and the same
   `PortDevice`/`Bridge` Rust code.
4. A bus transaction log recorded on hardware during a single-loop session
   replays against the Marlin bridge top. Every read value matches, including
   responses, status and the PL output digest.
5. The CPU-engine firmware variant still builds and passes its QEMU suite. The
   existing `rt_fpga_sim` tests stay green.
6. Bitstream build, xsdb bring-up and firmware build are documented and
   reproducible on the tested board.

## Design rules

- Cut at `prepare_with_consumer`. The app, backend, protocol and network are
  unchanged.
- The ARM controls only. Host traffic stays between quanta. In this milestone
  the ARM paces quanta: control, then run, then wait for done. PL timer pacing
  comes later.
- One generated register map, emitted as SystemVerilog and as Rust constants,
  derived from the core's port list.
- One configuration profile shared by the Marlin bridge top and the bitstream:
  capacities, AXI width and arena base. The board profile owns the arena base.
- `rt_fpga_adapter` builds as `no_std` + `alloc`. Serde, schemars and timing
  metrics sit behind a `std` feature.
- Driver logic never depends on simulation-only signals. Features that need
  them, such as the probe-based memory report, get a bridge-visible equivalent
  or become optional.
- The existing pin-level Marlin top and its tests remain until the bridge path
  covers them.
- No new audio or MIDI paths.

## Staged implementation

Each stage ends green on its listed checks and is committed.

### 1. Separate the driver from the simulator

- [x] Move load sequencing, packing and the run/wait logic from
  `rt_fpga_sim/src/simulator.rs` into `PortDevice<P: PortIo>`.
- [x] Make `Simulator` implement `PortIo` only.
- [x] Port `rt_fpga_adapter` to `no_std` + `alloc` with a default `std` feature.
- Evidence: with Verilator 5.032 in WSL, all of the following pass, matching the
  baseline before the change:
  - `cargo test -p rt_fpga_sim --features sim` (CI's command);
  - `--features app`, including all 11 `app_rtl` tests;
  - the adapter's unit tests;
  - the ARM `no_std` check.

  `app_arithmetic` and `app_media` did not compile on `proto` after #986 renamed
  `components` to `dependency_order`; both are updated. The `app` tests are not
  in CI.
- Verification:
  - `rt_fpga_sim` tests with `sim` and `app` features (Linux);
  - workspace tests for the adapter;
  - `cargo check -p rt_fpga_adapter --no-default-features --target armv7a-none-eabihf`.

### 2. Host bridge RTL and register map

- [ ] Write a generator from `loop_pipeline_axi`'s port list that emits the
  AXI-lite bridge and the Rust register constants. The bridge includes:
  - an ID/version register;
  - reset/quiesce control;
  - sticky done/fault status;
  - quantum and cycle counters;
  - the output digest.
- [ ] Implement `Bridge<B: Bus>` against the generated constants.
- Verification: Verilator/cocotb bridge tests (handshakes, back-to-back
  transactions, reset during idle, register readback) and Rust unit tests of
  `Bridge` with a mock `Bus`.

### 3. Marlin bridge top

- [ ] Add a Marlin top containing the bridge, the core with `EXTERNAL_IO=1` and
  the shared configuration profile, the PL source/digest and the memory model.
- [ ] Add a Marlin `Bus` over its AXI-lite pins.
- [ ] Run the existing app and mono scenarios through
  `PortDevice<Bridge<MarlinBus>>`.
- [ ] Add these tests to CI and fix the workflow path filters.
- [ ] Measure the adapter's host-side work per quantum to estimate ARM load.
- Verification:
  - the scenarios pass;
  - loop observations match the pin-level path;
  - digest determinism across repeated runs;
  - the new CI job runs on the PR.

### 4. ZedBoard host bitstream

- [ ] Add a host flavor to `fpga/zedboard`:
  - M_AXI_GP0 into the bridge;
  - the shared profile;
  - the arena base from the board profile;
  - an optional ILA.
- [ ] Keep the benchmark flavor working.
- Verification:
  - routed timing and utilization recorded;
  - on the board, xsdb register access reads the ID register and runs one
    quantum by hand, with the digest and counters advancing.

### 5. Firmware FPGA variant

- [ ] Add a generic execution task with CPU and FPGA implementations.
- [ ] Add an MMIO `Bus` and a startup bridge probe that refuses cleanly without
  a bitstream.
- [ ] Reserve the arena region in the ZedBoard profile, without cached ARM
  access.
- [ ] Add bus transaction logging behind a diagnostic switch.
- [ ] Add `proto/fpga-rust-reference` to the Zynq workflow triggers.
- Verification:
  - the CPU variant passes the QEMU suite;
  - the FPGA variant builds and audits;
  - booted in QEMU, the FPGA variant reports the missing bridge without hanging.

### 6. Hardware end-to-end

- [ ] Document and script the xsdb sequence: bitstream, PS init and post-config,
  ELF.
- [ ] Attach the desktop app and exercise the single-loop feature set.
- [ ] Record a transaction log and replay it in the Marlin bridge top.
- [ ] Run the same session on the CPU variant on the same board for A/B.
- Verification: acceptance criteria 1–4 and 6 hold on the board.

### 7. Optional: QEMU↔Marlin co-simulation

- [ ] Add a QEMU GP0 forwarding device and a host bridge server over the `Bus`
  layer.
- [ ] Add a reduced FPGA-variant integration test in CI.
- Verification: the FPGA variant attaches and runs a short scripted session
  under QEMU.

### 8. Final validation and delivery

- [ ] Rerun every stage check from a clean checkout.
- [ ] Record hardware evidence and update the READMEs.
- [ ] Push the branch and open a PR to `proto/fpga-rust-reference`.
- [ ] Work until CI is green.
- [ ] Address automated review feedback until the reviewer approves.
- [ ] Do not merge without the user's instruction.

## Testing cost and confidence

- **Stages 1–3:** fast host-side checks of a few minutes, plus Marlin runs on
  Linux CI. These catch most driver and protocol defects before hardware.
- **Stage 5:** reuses the existing QEMU suite for the shared firmware and adds
  only a short FPGA-variant boot check.
- **Hardware stages 4 and 6:** manual and short, gated by the transaction replay,
  which ties board behavior back to simulation.
- **Stage 7:** buys CI coverage of the FPGA variant at a large runtime cost and
  is deferred.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
