# Iterative node/capacity arithmetic experiment

Control is selected C4 `10cddabd`, measured HDL `7d2d7cd7`. Restore the
unselected audio-address unit and verify all 38 control HDL hashes before
applying this candidate. The scratch-address variant is also absent.
Only media_metadata.sv changes. Keep all public widths/capacities, frame/time,
opaque values, handles, ownership, validation and error priority unchanged.

Use one registered 34-bit add/subtract datapath for node ceil/accumulation,
TABLE_PAGES-high_water, adding free_pages, required-existing nodes and the final
capacity comparison. For any raw32 count and LEVELS<=8, the closed-form sum of
ceil(count/16^k) is at most286331153 (<2^29). A 33-bit temporary preserves the
ceil input+15 carry; a 29-bit sum is exact. Existing 64-bit saved capacity words
can remain, with explicit sign/zero extension and unused-bit markers.

Supported TABLE_PAGES is a positive signed32 parameter. For arbitrary raw32
high_water/free_pages, signed mathematical availability lies between roughly
-2^32 and 3*2^31, inside signed34. Sign-extend its 34-bit representation back to
64 bits to preserve the original unsigned64 subtraction/wrap, including when
adding free_pages wraps the result back to a small positive value. If available
is negative, its unsigned64 value exceeds any possible additional-node count;
otherwise a signed34 available-additional result is negative exactly when the
old unsigned64 comparison would refuse. Keep the separate public32 diagnostics
unchanged. No malformed counter is silently truncated into apparent capacity.

COUNT_CEIL and COUNT_SUM alternate one adder over LEVELS. Count the new extent,
then the old extent unless used0. Two more stages calculate availability; the
existing CAPACITY and CHECK_CAPACITY states calculate the difference/borrow.
Primary LEVELS5 growth adds12 clocks for used0 or22 otherwise. When new_count<=
used, requested extra pages are zero and unsigned0>available is always false;
retain the original COUNT_NODES/CAPACITY/CHECK clocks and skip unused arithmetic.
Most non-growing append checks therefore keep their clocks. Deep first-fill
metadata writes all grow and may nearly double their setup cost; measure it.

Preparation only: strict lint passes default/min/odd/primary. A separate
big-integer closed-form reference matches an emulation of the bounded arithmetic
for144000 boundary cases and100000 seeded random cases (seed20261123), including
underflow, carry-back-to-positive, u32 maxima and LEVELS1..8. This does not qualify
RTL behavior or area. Preserve this script/result with the eventual candidate.

Before selection run both-width boundary/malformed-counter tests and deep setup,
public store/retained exports, deferred/ownership/validity/full metadata/media,
held inputs, reset and memory errors. Measure actual grow/no-grow/error costs,
cache/AXI traffic and primary connected cold/steady/boundary callbacks. Screen
complete mux/register/control overhead, route if viable, and identify the new
critical path. Repeat small or surprising wins; qualify clocks only from passing
physical timing. Preserve the original tests and completion guards.
