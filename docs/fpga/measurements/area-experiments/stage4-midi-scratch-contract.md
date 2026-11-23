# Shared MIDI scratch and input-record address datapath

Control C4 is selected head `10cddabdbfe015f8a64c1105449aa5634be2f2fc`, whose
38 production HDL hashes match measured `7d2d7cd7`. This trial changes only
`midi_data_jobs.sv`. Keep C4's store formatter and all other RTL unchanged.
Do not inherit rejected MIDI type narrowing or any future audio variant.

Preserve public capacities, raw 64-bit arena/source addresses, 32-bit indices,
packet fields/order, timestamps, generations, errors, ownership and publication.
One 64-bit adder alternates the scratch-region addition and final base addition.
Reuse `own_address` to hold the sampled base while request-valid is low. Sample
index/word/base at the original request edge; save a conservative offset derived
from a 65-bit `CONTENT_BYTES + 2^36-1`, falling back to 64 bits on overflow.
All full-width final carry/wrap behavior remains intact.

Two formatter states precede each local word read/write and record-address setup.
The existing request/wait states retain memory ownership and payload stability.
Input records stay at source+16*index, scratch records at arena+CONTENT_BYTES+
16*index, and words at +4*field. Read requests retain their old held write-data
value. Only scratch operations add CONTENT_BYTES; raw input and saved-record
bases must not receive it. Writes/record reads preserve their original word order.

Before implementation the C4 MIDI store trial passes all 23 affected tests,
1,041 extended mixed and 40 musical non-cycle rows. Run affected full MIDI and
expression tests on this sibling, including AXI32/64 stalls/faults, poisoned
inputs, scratch exhaustion, dense merges, seek/stop, retakes and held responses.
The MIDI-store HDL is unchanged, so its standalone raw operation/cycle control
remains C4's; the changed job/scratch controller needs connected measurements.

Measure short connected and musical callback costs against C4 before selecting;
record setup, boundary and cleanup costs from tests/benchmarks. Screen optimized
LUT/FF/carry/mux/control-set use including all staging overhead, then route if
viable. Repeat a small physical win. Keep nominal clock conversions separate
from physically passing timing; C4 still fails setup timing at 100 MHz.
