# Arena-relative address and saved path context trial

This behavior-preserving sibling starts from C2 (`a046e9c5`, measured production
HDL `17f69fd8`). It inherits neither rejected bookkeeping widths nor the optional
local-frame feature. All public widths, capacities, precision, error priorities,
generations and scheduling remain unchanged.

Keep the 64-bit arena origin and compute original addresses at full precision.
Only saved arena-relative parent/cache offsets are narrowed. A conservative
upper bound is the largest reference/stack/page/queue base plus `2^38-1`:
every raw 32-bit ID is multiplied by at most 64, and entry offsets fit the last
six bits. Use a 65-bit bound and retain 64 address bits if that bound overflows
or page-base subtraction could underflow. Do not truncate any raw DDR page ID,
payload or request index. This preserves addresses for malformed metadata too.

For path entries, reuse existing `path_page` values and store a four-bit entry
index instead of another complete address. A single selected path level chooses
the child-parent lookup or the trim parent's lookup before reconstruction.
Page addresses are 64-byte aligned, so concatenating the entry index and two
zero low bits reproduces the original addition exactly, including unsigned
64-bit wrap. Both original capture sites update the page and index together;
each child consumer follows a walk capture and each trim consumer follows a
complete scan descent. Cached root loads occur before these fresh captures.

The C2 checkpoint already maps the old path-address array to 37 RAM32X1D
primitives and zero flip-flops. Therefore this is a distributed-memory/read-port
and reconstruction-logic tradeoff, not a projected FF reduction. Include the
adder, dynamic index mux and all control logic in connected synthesis and routing.

Validate minimum/odd/primary capacities, all radix levels, parent crossings,
opaque high bits, malformed retirement metadata, invalid requests, fences,
backpressure, faults, reset, allocation/trim/detach/GC and live-view reuse.
Compare original raw responses and cycles, both AXI widths, the public primary
storage fixture, and the connected mixed workload with C2. Screen mapped usage,
then route if viable and retain unsuccessful outcomes.

The frame-only automatic test registration is absent on this sibling because
its RTL parameter is absent. Its complete test, sources and results remain on
`area/s3-local-frames` and revision `3e47163d`; the CLI replay fixtures remain
available with that explicit source root. Existing production tests are retained.
