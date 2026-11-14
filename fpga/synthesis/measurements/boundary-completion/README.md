# Boundary timing experiments

These matched Vivado 2021.1 experiments are intermediate measurements, not final
100 MHz qualification. Each directory preserves frozen sources, the registered
context, constraints, tool/provenance records, raw compressed reports and logs.
`measurements.json` gives extracted counts. No clock or capacity was reduced.

The TP shared context is L64/R272/E400, xc7z020clg484-1, 100 MHz with 0.2 ns
uncertainty. Clearing the wide host reply at command acceptance removes late
zero-result writes from ordinary error paths. Synthesis changes from 13,340 to
13,121 LUTs; the routed count changes from 13,312 to 13,078, FF 8,813 to 8,773,
BRAM stays 28.5, and setup/hold change from +0.105/+0.007 to +0.236/+0.008 ns.
These are single matched runs, without a route-variation study or a promise of
final system savings. All five TP capacities with 20 seeds pass.

The full core uses L64/R128/E256, program/state/jobs 512, 8 inputs/outputs,
AXI64, Q32 and xc7z020clg400-1, with the same clock and uncertainty.

| Checkpoint | LUT | FF | BRAM36 | Setup / hold ns |
| --- | ---: | ---: | ---: | ---: |
| #993 reference | 29,419 | 19,502 | 57.5 | -1.845 / +0.009 |
| Validated resident tag widths | 29,038 | 19,291 | 57.5 | -0.752 / +0.013 |
| Separate manual/automatic budget mux | 29,331 | 19,294 | 57.5 | -0.717 / +0.006 |
| Reserve PROCESS before media becomes ready | 29,056 | 19,212 | 57.5 | -0.704 / +0.009 |
| Wide TP reply change | 28,607 | 19,215 | 57.5 | -0.317 / +0.002 |
| Separate speculative hint selection | 28,508 | 19,288 | 57.5 | -0.194 / +0.016 |
| Separate manual media grant | 28,638 | 19,478 | 57.5 | -0.290 / +0.009 |
| Registered speculative hint | 28,213 | 24,715 | 57.5 | -0.266 / +0.008 |
| Registered DMA logical address | 28,320 | 26,574 | 57.5 | -0.047 / +0.019 |

Later changes can alter placement, replication and the worst path; the results
are not monotonic. None of these complete-core runs closes timing. The autonomous
I/O wrapper has additional image, action and reply storage and needs its own
measurement; it is not included in this older complete-core context. The first
wrapper route misses setup by 2.105 ns (hold +0.005 ns). Its raw reports are in
`autonomous-wrapper-m4/`; the registered control/action variant is a later run.
The registered admission/action wrapper misses setup by 1.170 ns; unconditional
action-row capture and hint registration improve this to -0.963 ns (hold +0.005
ns, 35,259 LUT, 33,938 FF, 102 BRAM36). These are earlier M4 wrappers, before
public native rows/capture integration, and cannot qualify the final M5 system.
Their sources and negative reports remain in `autonomous-wrapper-pipelined/` and
`autonomous-wrapper-row-register/`.
`fullcore-dma-select/` adds registered fill/flush selection and a metadata miss
phase; it also misses setup (-0.351 ns, hold +0.005 ns). The later registered DMA address retains full 64-bit validation and adds one
clock per DMA request; its -0.047 ns setup result still fails the 100 MHz gate.
These misses remain open.

`reclaimer-equivalence/` proves unbounded sequential equivalence of the budget
mux restructuring (890/890 equivalence points). Only renamed internal grant
signals are excluded; state and outputs are proved. The later foreground
manual-grant command adds one acknowledged cycle outside that proof's scope.
Chunk tests cover four geometries, four memory models, AXI32/64 and retirement
races. Raw malformed chunk indices remain 32 bits and are rejected before
narrowing validated resident tags.

Replay source overlays under their recorded `source/` paths and run the saved
Tcl/constraints, or use `fpga/synthesis/run.py` and
`fpga/transition_processor/measure.py` with the frozen source/context. The public
Vivado runtime/catalog provenance remains documented in the ZedBoard catalog
notes. Physical hardware is unavailable and no physical claim follows.
