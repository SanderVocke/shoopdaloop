# Shared audio metadata relative-address datapath

Control is selected C4 `10cddabd`, whose production HDL matches measured
`7d2d7cd7`. Restore the rejected MIDI scratch module before this trial and verify
all 38 control HDL hashes. Change only `media_metadata.sv` after this contract.
Keep the selected MIDI-store formatter, original MIDI scratch implementation,
all capacities/public widths, payload words, generations and timestamps.

Register region, raw index and field before one shared RELATIVE_BITS adder.
It generates page, reference, free-stack and retirement-queue offsets; the
existing separate 64-bit arena-origin adder remains. Retain C3's conservative
65-bit maximum-relative bound and 64-bit fallback, including full raw32 page IDs
and all page0 underflow/carry behavior. PAGE_BASE is 64-byte aligned, so adding
PAGE_BASE-64 to {page,index,2'b0} preserves the prior aligned-page calculation.
Queue word queries and queue records keep their distinct 4/64-byte strides.

Formatting adds one clock before such cache requests. CHILD parent-address
setup uses the same unit and adds one clock. At CHILD, current page and ordinal
field equal the path context saved by the preceding WALK and held throughout
the cache request/wait; use those registered operands directly. FREE_LINK still
uses its saved next-level page/index. Raw saved-parent requests and cache fences
bypass formatting. Narrow the private cache task input to the same relative
width it already truncated to, preserving behavior and strict unused-bit lint.

Before selection, run both-width minimum/odd/primary and deep-directory controls,
public capture/retake/retained-export/retirement workloads, full metadata/media/
ownership/deferred suites, faults/reset and held-input/response checks. The
per-view current-block translation hit bypasses formatting. Measure real cold,
steady, boundary, maintenance, setup and cache/AXI costs rather than assuming all
commands add the same clocks. Compare logical results independently of cycles.

Screen the complete connected LUT/FF/carry/mux/control-set cost against C4; route
if viable and repeat a small physical win. Retain failing setup/hold reports and
keep nominal clock conversions separate from physical timing qualification.
