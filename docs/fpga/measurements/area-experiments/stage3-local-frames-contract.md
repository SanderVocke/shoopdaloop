# Optional bounded local audio frames

This sibling starts from C2, without either rejected bookkeeping-width trial.
`LOCAL_FRAME_BITS` defaults to 64. The measured alternative uses 21 bits: each
audio view has at most 2,097,151 frames (43.6906458 seconds at 48 kHz), covering
the unchanged 30-second primary workload. Global transport time, MIDI frames,
public command/configuration widths, sample precision and payload capacity stay
unchanged. This is an explicit duration restriction, not a default replacement.

Descriptor RAM stores the selected number of frame bits and reconstructs the
original 160-bit configuration representation on reads. Configuration writes
with a larger frame count fail without changing the descriptor. Appends whose
full-width new frame count exceeds the limit return `MEMORY_FULL` before payload
consumption or allocation. Slice/read bounds are checked at their original wide
precision before narrowing their accepted results. Retained slices, publication,
reset, ownership and memory-fault rules are otherwise unchanged.

Only the stored descriptor frame field, foreground frame count, sample cursor,
new frame count, last-frame comparison and span-to-wrap registers are narrowed.
Capacity calculations retain their original width and overflow behavior. Stored
offsets/chunk IDs stay wide; raw DDR words and arena bases are not truncated.

Validate the unbounded 64-bit configuration against C2 before attributing any
result to the 21-bit variant. A reduced 5-bit limit allows real recording to
reach 31 frames and refuse the next frame. Test held/poisoned requests, large
counts, invalid high configuration/command bits, wrap, nested offset slices,
retakes, reclamation, stale RAM/reset, both AXI widths and memory stalls/faults.
Use independent sample vectors and explicit integer duration bounds. Any
synthetic maximum-length descriptor is an admission-only test: its nonexistent
payload must not be read or called a full-length recording validation.

Then compare public storage operations and the common connected workload against
the original reference. Include all descriptor packing and validation logic in
the physical profile with `LOCAL_FRAME_BITS=21`; keep the canonical default at
64 bits. Report latency, memory traffic, rejection behavior, BRAM, registers,
LUTs, carries, control sets and routed slices, including unsuccessful outcomes.
