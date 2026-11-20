# Cache-less MIDI RTL validation

The implementation starts from `feat/backend-replacement` at `147c2457`, including
both merged Rust MIDI milestones. The Rust interpreter, content store, difference
service, transition component and complete session fixture are independent
oracles. No production Rust/backend selection or application MIDI capability was
changed.

## Functional checks

All sixteen MIDI regression cases passed with the fixed per-loop descriptors in
primary BRAM. A further focused independent-input/nonzero-output-offset check
passed after adding the optional explicit-output and recovery offset fields.

```sh
cargo test --locked -p rt_fpga_testbench --features sim --test midi -- --test-threads=1
cargo test --locked -p rt_fpga_reference --all-features -p rt_fpga_compiler
```

The reference/compiler gate passed 124 tests. Formatting, warning-denying
workspace build, test-policy and retained-Marlin-coverage checks passed.

| Area | Checks |
| --- | --- |
| Interpretation | Mixed channel packets, pedals, banks/programs, reset, unsupported/invalid packets and memory refusal; complete state compared after each update. |
| Differences | Full/partial initialization, changed-cell updates and values returning to target; bitmap equality with Rust. |
| Restoration | Exact ordered output, loaded/pending banks, pressure-only changes, held/pedal-latched notes and cleanup under output pressure. |
| Storage | Block sharing, nested/nonaligned/empty content ranges, writer-prefix discard, generations, recording exhaustion and bounded retirement. |
| Sessions | Record, restore, repeat, stop, coherent export during retake, duplicate and imports prepared during playback. |
| History | Quantum sizes 1/16/32/64/128, exact checkpoints and a start needing 31 distinct-controller updates; independent inputs and incremental map derivation. |
| Publication | One shared grab into 128 bindings, playback at eight destinations, group replace/clear, duplicate-target refusal and shared metadata grants. |
| Capacity | Imports exhaust their permitted resources while admitted capture continues; private history/recording writers reject UI edits. |
| Ownership/recovery | Existing owner emits its note-off before handoff; coordinated refusal recovery and stop cleanup with one accepted packet per call. |
| DDR transport | Actual AXI32/64 endpoint, variable pressure/delayed responses, word lanes, bounds/faults and output recovery without DDR after a memory fault. |

The end-to-end fixtures run the actual Rust SP/audio pipeline, then supply its
settled records to RTL. They compare emitted packets, sample offsets and complete
input/output state. They do not replace RTL interpretation or storage with a
software implementation.

## Memory and synthesis

The default reserved arena spans 38,968,492 bytes (37.163 MiB), plus caller
staging space. This includes a 16 KiB private address window for primary-BRAM
loop descriptors; DDR accesses do not use that window. The other regions hold
the 19 MiB event arena, snapshots, histories, per-loop activity and comparisons.
There are no on-chip arrays sized by recording duration or total event capacity.

The default standalone AXI32 core is checked with Yosys 0.52 for Xilinx 7-series:

```sh
yosys <<'YOSYS'
read_verilog -sv -Ifpga/midi/rtl fpga/midi/rtl/*.sv fpga/axi_memory/rtl/word_axi.sv fpga/axi_memory/rtl/word_dma.sv fpga/axi_memory/rtl/axi_master.sv
synth_xilinx -family xc7 -top midi_runtime_axi -noiopad -noclkbuf
stat
YOSYS
```

| Mapped resource | Count |
| --- | ---: |
| LUT1–LUT6 cells | 29,561 |
| FDRE/FDSE registers | 12,557 |
| DSP48E1 | 70 |
| RAM32M | 12 |
| RAMB36E1 | 4 |

The four block RAMs hold the bounded 16 KiB loop descriptors. They are primary
storage; there is no DDR mirror or cache policy. Small remaining working values
map to registers/LUT RAM. Synthesis is standalone and unplaced, not a complete
audio-plus-MIDI board-fit or clock-closure result. There is no cloud timing gate
or required 128-grab elapsed-time threshold.

## Integration boundary

The component consumes settled loop records and supplies the MIDI side of content
publication. Its embedding commits the matching SP content lengths/revisions
before the next quantum. Application graph/control enablement and routed-board
integration remain separate, matching the Rust reference milestone's boundary.
The shared word-memory service can later acquire caching or burst transfers
without changing the view/state ownership model.

Validation was focused on the new MIDI RTL, its reference/compiler oracles and
the reused AXI endpoint. The native application suite was not repeated for this
isolated component addition. CI results and automated review were not consulted,
as requested; the PR remains subject to the user's merge decision.
