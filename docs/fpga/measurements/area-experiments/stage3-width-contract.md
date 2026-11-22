# Stage 3 width experiments

Both first candidates are independent siblings of C2 (`a046e9c5`, production HDL
identical to `17f69fd8`). No precision, capacity, ownership, error or public input
width change is intended. Register-cycle scheduling stays unchanged.

The audio candidate narrows only capacity bookkeeping. For every 32-bit count
and one through eight radix levels, the node total is at most `0x11111111`,
which fits 29 bits. The initial count-plus-15 needs 33 bits. Required/existing/
additional nodes use 29 bits; additional is computed only when count grows, so
the monotone node function cannot underflow. The allocation high-water mark
increments only below TABLE_PAGES, so unallocated pages fit 32 bits. Available
pages retain a 33-bit sum, including carry from any raw 32-bit free-page count.
Public requests, counts, page IDs, payloads, generations and addresses remain
wide. This is separate from capacity-bound root packing and iterative arithmetic.

The MIDI sibling will narrow only provably bounded private traversal/queue
fields: descriptor cursor 0..15, record word 0..3, allocation candidate including
its VIEWS exhaustion sentinel, format scan including its final sentinel,
and retirement count/head/tail. Handle concatenations explicitly retain the
32-bit slot field. Raw DDR-derived view slots, ownership/free counts, public
handles/generations and global 64-bit timestamps remain wide.

C1 and C2 boundary controls precede the RTL changes: metadata has 187 matched
requests per AXI width across minimum, odd and primary-sized counts. MIDI has
564 independent ContentStore requests over full pools and retirement wraps.
Both include high-bit invalid inputs, overflow/fault/reset and held responses;
connected callbacks, media ownership, both AXI widths and DRAMsim3 are separate
checks. Compare each mapped/physical result and actual cycles with C2; raw
bit-count reductions alone do not establish an implementation saving.

Local-frame and arena-relative variants follow as separate candidates, with any
new duration/address restriction specified before implementation. They are not
part of these behavior-preserving bookkeeping changes.
