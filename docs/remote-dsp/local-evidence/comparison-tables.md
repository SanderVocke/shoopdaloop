# Reproduced DSP comparison tables

Output misses = late + missing measured output slots. Concealments cover the whole stream, including warm-up. These are different defects and their counts must not be conflated.

## Calculated digital audio latency

At 48 kHz, packetization is P/fs, the allowance after packet readiness is N*P/fs, and modeled source-to-output audio latency is ((N+1)*P+A)/fs. All fixed-comparison rows use A=0. Aggregation (k-1)*P/fs and network/DSP turnaround must fit inside the allowance; do not add them again. This assumes sample-paced playback beginning at each packet's consumption deadline. It is not measured analogue input-to-output latency: ADC/DAC, audio-device/callback and additional endpoint buffers are not included. Earlier reply arrival does not reduce the fixed presentation delay.

## Complete campaign totals

| Campaign | Records | Measured slots | Verified samples | Output misses | Concealed input | Retired | Healthy measured runs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cloud sweep | 288 | 2,149,596 | 71,534,176 | 356,839 | 1,609 | 2 | 90 |
| Local sweep | 288 | 2,149,596 | 77,201,408 | 87 | 71 | 0 | 222 |
| Cloud tuning | 184 | 4,495,600 | 124,441,600 | 6,813 | 1,613 | 0 | 57 |
| Local tuning | 232 | 4,828,624 | 133,575,168 | 5,751 | 78 | 1 | 218 |
| Local matched tuning additions | 26 | 5,377,528 | 149,767,168 | 13 | 115 | 0 | 23 |
| Local matched sweep additions | 1 | 360,000 | 5,760,000 | 2 | 0 | 0 | 0 |

Totals mix configurations; local tuning also has two extra priority conditions. Use matched comparisons below.

## Strict baseline boundary (all three repetitions fully healthy)

Each cell is cloud → local minimum N, with calculated audio latency in parentheses. An em dash means no tested allowance was fully healthy in all repetitions.

| P | k=1 | k=2 | k=4 | k=8 |
| --- | --- | --- | --- | --- |
| 8 | — → 8 (1.500 ms) | — → — | — → 16 (2.833 ms) | — → — |
| 16 | 16 (5.667 ms) → 1 (0.667 ms) | — → 16 (5.667 ms) | — → 16 (5.667 ms) | — → — |
| 32 | 8 (6.000 ms) → 1 (1.333 ms) | — → 2 (2.000 ms) | — → 8 (6.000 ms) | — → 8 (6.000 ms) |
| 64 | 1 (2.667 ms) → 1 (2.667 ms) | — → 4 (6.667 ms) | — → 8 (12.000 ms) | — → 8 (12.000 ms) |
| 128 | 2 (8.000 ms) → 1 (5.333 ms) | — → 2 (8.000 ms) | — → 8 (24.000 ms) | — → 8 (24.000 ms) |

## Original sweep: fixed 60-second configurations

| P/k/N | Audio latency ms | Cloud miss % | Local miss % | Cloud late/missing | Local late/missing | Cloud/local concealments | Cloud/local retired | Cloud/local batch p99 us |
| --- | ---: | ---: | ---: | --- | --- | --- | --- | --- |
| 8/1/8 | 1.500 | 95.10194 | 0.00000 | 165/342,202 | 0/0 | 52/0 | True/False | 62.9/4.0 |
| 8/1/16 | 2.833 | 0.07778 | 0.00056 | 280/0 | 2/0 | 0/0 | False/False | 55.1/3.6 |
| 16/4/8 | 3.000 | 0.17000 | 0.00000 | 306/0 | 0/0 | 278/0 | False/False | 74.0/9.0 |
| 16/4/16 | 5.667 | 0.01889 | 0.00000 | 34/0 | 0/0 | 268/5 | False/False | 73.2/10.4 |
| 128/8/8 | 24.000 | 0.02222 | 0.00000 | 5/0 | 0/0 | 17/0 | False/False | 114.4/23.7 |
| 128/8/16 | 45.333 | 0.00000 | 0.00000 | 0/0 | 0/0 | 24/0 | False/False | 148.4/29.0 |

## Original tuning: same 60-second conditions on both hosts

Counts are output misses / concealed input. Baselines are fresh yield-free runs using the tuning executable.

| P/k/N | Audio latency ms | Cloud-selected alternative (held fixed locally) | Cloud baseline | Local baseline | Cloud alternative | Local same alternative |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| 8/1/4 | 0.833 | spin-free | 1,190 / 0 | 147 / 0 | 899 / 0 | 0 / 0 |
| 8/1/8 | 1.500 | server-spin-pinned | 887 / 0 | 0 / 0 | 236 / 0 | 0 / 0 |
| 8/1/16 | 2.833 | yield-pinned | 527 / 0 | 0 / 0 | 129 / 0 | 0 / 0 |
| 16/4/4 | 1.667 | spin-pinned | 369 / 138 | 3 / 1 | 612 / 232 | 5 / 28 |
| 16/4/8 | 3.000 | spin-pinned-gap1000 | 102 / 227 | 0 / 0 | 139 / 65 | 0 / 0 |
| 16/4/16 | 5.667 | spin-pinned-gap1000 | 10 / 174 | 0 / 0 | 21 / 123 | 0 / 0 |
| 128/8/8 | 24.000 | client-spin-pinned | 1 / 3 | 0 / 0 | 0 / 9 | 0 / 0 |
| 128/8/16 | 45.333 | spin-pinned | 13 / 14 | 0 / 1 | 0 / 8 | 0 / 0 |

## Newly available real-time scheduling: matched 60-second runs

All three conditions use yielding, the same pinned CPUs, priority 1 for FIFO/RR, and a 200 us input gap. Counts are output misses / concealed input. Ordinary scheduling has priority 0 and niceness 0.

| P/k/N | Allowance ms | Audio latency ms | Ordinary pinned | FIFO pinned | RR pinned |
| --- | ---: | ---: | ---: | ---: | ---: |
| 8/1/4 | 0.667 | 0.833 | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/8 | 1.333 | 1.500 | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/16 | 2.667 | 2.833 | 0 / 0 | 0 / 0 | 0 / 0 |
| 16/4/4 | 1.333 | 1.667 | 13 / 38 | 0 / 0 | 0 / 0 |
| 16/4/8 | 2.667 | 3.000 | 0 / 73 | 0 / 0 | 0 / 0 |
| 16/4/16 | 5.333 | 5.667 | 0 / 0 | 0 / 0 | 0 / 0 |
| 128/8/8 | 21.333 | 24.000 | 0 / 0 | 0 / 0 | 0 / 0 |
| 128/8/16 | 42.667 | 45.333 | 0 / 4 | 0 / 0 | 0 / 0 |

## Short screening, matched workloads (24 runs per condition)

Each condition has all eight configurations × three repetitions, mixing calculated audio latencies of 0.833–45.333 ms. CPU cost covers both processes and their full lifetime, not DSP alone.

| Host | Condition | Output misses / slots | Concealed input | Retired | Fully healthy | Mean pair CPU cores |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Cloud | yield-free | 207 / 166,512 | 120 | 0 | 7 | 2.070 |
| Local | yield-free | 0 / 166,512 | 2 | 0 | 23 | 2.014 |
| Cloud | yield-pinned | 233 / 166,512 | 107 | 0 | 9 | 1.994 |
| Local | yield-pinned | 0 / 166,512 | 6 | 0 | 21 | 1.981 |
| Cloud | spin-free | 109 / 166,512 | 106 | 0 | 9 | 2.070 |
| Local | spin-free | 0 / 166,512 | 13 | 0 | 23 | 2.013 |
| Cloud | spin-pinned | 423 / 166,512 | 100 | 0 | 8 | 1.994 |
| Local | spin-pinned | 5,596 / 166,512 | 23 | 1 | 21 | 1.964 |
| Cloud | client-spin-pinned | 155 / 166,512 | 95 | 0 | 8 | 1.993 |
| Local | client-spin-pinned | 0 / 166,512 | 0 | 0 | 24 | 1.982 |
| Cloud | server-spin-pinned | 228 / 166,512 | 69 | 0 | 5 | 1.993 |
| Local | server-spin-pinned | 0 / 166,512 | 4 | 0 | 22 | 1.981 |
| Cloud | spin-pinned-gap1000 | 323 / 166,512 | 23 | 0 | 11 | 1.993 |
| Local | spin-pinned-gap1000 | 0 / 166,512 | 0 | 0 | 24 | 1.981 |
| Local | yield-pinned-fifo1 | 0 / 166,512 | 0 | 0 | 24 | 1.987 |
| Local | yield-pinned-rr1 | 0 / 166,512 | 0 | 0 | 24 | 1.987 |
