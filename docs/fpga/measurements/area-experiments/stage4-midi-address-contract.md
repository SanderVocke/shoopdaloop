# Shared MIDI storage address datapath

Control C3 is selected head `607c8a0f`, with measured RTL `fe2184ea`. This
candidate changes only `midi_store.sv`. It preserves public widths, capacities,
timestamps, generations, logical records, errors and ownership. It trades extra
clocks for shared arithmetic; it does not inherit rejected MIDI width reductions.

Latch region, index, field, operation, data and return state at the original
request edge. Reuse the existing memory-address register to hold the sampled
arena/raw base while memory-valid is low. Three formatter states normalize the
index, add its region offset, and add the held base through one explicit 64-bit
adder. A record-address-only action uses the same unit without issuing a bus
transaction. Existing read/write request and response states retain ownership.
Valid requests hold address, operation and write data under backpressure.

The view and table helpers intentionally wrap the complete relative offset to
32 bits before adding the arena base; preserve this, including table-index
multiply/add overflow. Reference/free/event/retirement addresses retain raw
32-bit IDs and wide offsets. Event indexing preserves its existing 32-bit
product-plus-ordinal wrap before the byte shift. Batch packets remain at byte
offset 12 of each 16-byte input record. Raw source and arena bases remain 64 bits.

Derive saved offset width from a 65-bit maximum of all region bases plus
`2^36-1`, with 64-bit fallback. The primary profile needs 37 bits. This avoids
assuming that a wrapped total layout size proves every intermediate region is
small. Final base addition preserves all 64-bit carry/wrap behavior. No unchecked
DDR-derived ID or public input is silently truncated.

The design adds three formatter clocks per memory request, plus three for each
record-address setup. Measure actual end-to-end costs with memory stalls and
shared execution included: format, allocation, capture, playback, boundaries,
retakes, reference updates, retirement and faults. Existing completion guards
remain in the first run; preserve any failure and distinguish excessive latency
from incorrect progress before proposing a different bound.

Before changing RTL, C3 passes the full MIDI suite and original-RTL reserved-word
controls. The zero-base memory extension reproduces 54 original request rows
exactly; high/wrapping arena controls reproduce 108 matched rows. Those tests
qualify the standalone word interface, not board DDR mappings. After the change,
compare logical replies and transfer sequences independently of changed cycles,
then run both AXI widths, expression/restoration, backpressure, poisoned command
inputs, memory errors/reset, connected quantum measurements and physical area.
Include all operand muxes, staging registers and control logic in the comparison.
