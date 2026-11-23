# Working-state and validity experiment contracts

Preparation only. Stage 4's iterative capacity decision is still pending. Freeze
the selected Stage 4 control before starting these candidates, run them one at a
time, and record the actual parent/source hashes for each. Promote a compatible
winner only after its behavior, cycles, mapped resources, routed slices and
timing have been measured. Retain the faster/smaller alternatives explicitly.

No public numeric precision, capacity, descriptor layout, handle generation,
ownership, fault, publication or accepted-command semantics change in this
family. Existing fixtures and completion guards remain. Distinguish initialization,
allocation, transfer and steady callback clocks. Inference and packing must be
measured; logical bit counts are hypotheses, not predicted mapped savings.

## Cold MIDI descriptor words

Keep the thirteen hot words in registers. Store opaque reserved descriptor words
10, 11 and 14 in a four-row, 32-bit synchronous-read distributed RAM. Three
validity bits make new/reused descriptors emit zero while preserving loaded
opaque values exactly. Reset clears these bits, not the RAM. The current
descriptor staging array already has no reset; do not count this as reset removal.

Read ahead in existing SAVED_VIEW states for words 9, 10 and 13, so the next
SAVE_VIEW sees the needed cold word without a new state. Capture loaded reserved
words on their original completion edge. Reset/creation discards all three old
values. A failed descriptor load cannot be accepted as a valid working descriptor;
the next successful load must overwrite every reserved word before use.

Verify the reserved-word reuse/reset and high/wrapping arena-base controls,
all raw storage operations, both AXI widths, command latching, backpressure,
memory faults, MIDI loading/restoration and mixed/musical callbacks. Require
exact replies, packets/order, DDR words and traffic. Measure actual RAM cells,
FFs, mux/decoder cost, controls and cycles; RAM inference alone is insufficient.

## Disjoint MIDI packet storage lifetime

In a separate trial, avoid keeping a fourth payload word. Its read result is
already returned directly from data_word; its append value is the accepted
packet/length pair. Those command registers stay unchanged from APPEND_BLOCK_ID
through all four writes and are replaced only for the next batch packet or
command. Use that stable pair for word 3, and retain three stored frame/order
words. Preserve the original sampling edge at the memory request formatter.

Run the same MIDI/descriptor/relocation and callback controls. Check partial
records, dense batches, reads followed by append, interrupted/reset commands,
held writes and restored expression state. Report whether the nominal 32-bit
copy existed in the mapped control and whether its removal saves connected area.

## Private datapath reset elision

Separately remove resets from root_data, memory_origin, parent_address and
cache_address only. Keep phase, validity, allocator state and public reply
register resets. Establish these assignment-before-use invariants:

- root_data is captured in ROOT_CAPTURE before ROOT consumes it. Root and leaf
  validity gate stale RAM contents and cached paths.
- memory_origin is captured with every accepted request before a cache bus
  request can become valid.
- parent_address is assigned after a child fetch before allocation can consume
  it; the root-parent path bypasses it.
- access_cache assigns cache_address before entering CACHE_SEND. Invalid bus
  address values do not establish a transaction.

Run full metadata/media/deferred/ownership/validity tests, both-width boundary
and deep-directory controls, retained exports and mixed callbacks. Exercise
reset during outstanding work and after stale RAM is populated. Inspect actual
BRAM output-register absorption, FF sites, reset/enable control sets and packing.

## Validity bits in the roots RAM

Separately replace the per-view root_valid and leaf_valid vectors with two bits
in each roots row and an eight-bit leaf epoch. Keep root payload/path words.
Reset initializes every valid row before request_ready; measure VIEWS clocks
directly and any overlap with store initialization separately. A 298-bit primary
row may add half a BRAM tile versus 288 bits; verify actual inference.

Global invalidation advances the epoch. Before an epoch can repeat, synchronously
read/write every row to clear leaf validity while preserving root validity and
all payload/path data. The wrap scrub costs VIEWS+1 extra clocks; no new request
can interleave. Reset during initialization/scrub restarts initialization and
refuses all old extents. Publishing sets root validity and sets leaf validity
exactly when opcode<2, matching the existing lifetime.

Preserve the existing masked-row side effect of an invalid high-ID trim: clear
the addressed leaf flag when the masked row exists, without changing other row
fields. Do not write odd-capacity padding rows. The original-RTL validity fixture
leaves a view untouched across 256 invalidations, checks that DDR reload occurs,
resets five clocks after another accepted invalidation, and completes another
257 invalidations. Run it across AXI32/64 and 1/5/512 views, together with the
full audio-storage regressions and raw boundary/deep/store/callback controls.

Retain all per-request, reset-ready and maintenance measurements. Changes to
admission or epoch-wrap latency must remain visible even when ordinary playback
cycles match. Control-set tool settings remain separate optional experiments;
they are not mixed into any of these RTL candidates.
