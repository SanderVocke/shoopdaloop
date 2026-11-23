# Public storage operation comparison

All six controllers pass the same 1,282 transactions at primary capacity: 128
captures of 1,057 frames, 33-frame exports from offset 1,000, clear/retake, retained
reads before and after cleanup, and unique recovery of every allocated block.
The independent Rust vectors and original default-policy tests are preserved.
This scope includes the public media controller, caches and ideal AXI memory;
it excludes SP/JP and DDR-added delay. Times are clocks, not board deadlines.
Each cell is median / maximum admission-through-cleanup clocks. Traffic and
every raw command/result are retained in the linked unit archives.

| Operation | [C1](c1-store-operations-unit-summary.json) | [Four-way](s2-fourway-store-operations-unit-summary.json) | [Banked](s2-banked-store-operations-unit-summary.json) | [Full leaf](s2-leaf-store-operations-unit-summary.json) | [One block](s2-block-store-operations-unit-summary.json) | [Directory path](s2-path-store-operations-unit-summary.json) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| capture | 4174 / 4336 | 4174 / 4336 | 4484 / 4711 | 4255 / 4408 | 4152 / 4305 | 4152 / 4305 |
| retain offset slice | 1372 / 1664 | 1372 / 1664 | 1728 / 2134 | 1431 / 1723 | 1372 / 1664 | 1372 / 1664 |
| clear retained capture | 156 / 156 | 156 / 156 | 161 / 176 | 165 / 165 | 165 / 165 | 165 / 165 |
| retake | 522 / 600 | 522 / 600 | 831.5 / 1000 | 625 / 703 | 513 / 591 | 513 / 591 |
| retake read | 74 / 74 | 74 / 74 | 89 / 104 | 65 / 65 | 65 / 65 | 65 / 65 |
| retained read | 308 / 348 | 308 / 348 | 355 / 370 | 212 / 262 | 265 / 357 | 265 / 357 |
| remove | 71 / 71 | 71 / 71 | 116 / 131 | 114 / 114 | 114 / 114 | 114 / 114 |
| maintenance drain | 183544 / 183544 | 183544 / 183544 | 296056 / 296056 | 183544 / 183544 | 183544 / 183544 | 183544 / 183544 |
| final maintenance drain | 100238 / 100238 | 100238 / 100238 | 165006 / 165006 | 102990 / 102990 | 102818 / 102818 | 102818 / 102818 |

View creation is unchanged (median 265, maximum519) and reset takes514 clocks.
The two maintenance rows drain bounded backlog separately. Private translations
help retake reads but have different eviction and retirement costs; the connected
playback improvements do not imply that every storage operation becomes faster.
