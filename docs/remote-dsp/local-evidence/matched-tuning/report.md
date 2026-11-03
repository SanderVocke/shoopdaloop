# Scheduling/polling experiment

26 recorded runs; 23 fully healthy runs.

All outcomes are retained. Pair CPU covers launch, warm-up, measurement and reporting. Service gaps include process work and scheduling delays, not just scheduler latency. Cgroup deltas include other tasks.

| Suite | P | k | N | Mode | Runs | Output misses / slots | Concealed | Retired | Mean pair CPU cores |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| followup | 8 | 1 | 4 | spin-free | 1 | 0 / 360000 | 0 | 0 | 2.015 |
| followup | 8 | 1 | 4 | yield-pinned | 1 | 0 / 360000 | 0 | 0 | 1.983 |
| followup | 8 | 1 | 4 | yield-pinned-fifo1 | 1 | 0 / 360000 | 0 | 0 | 1.988 |
| followup | 8 | 1 | 4 | yield-pinned-rr1 | 1 | 0 / 360000 | 0 | 0 | 1.987 |
| followup | 8 | 1 | 8 | yield-pinned | 1 | 0 / 360000 | 0 | 0 | 1.983 |
| followup | 8 | 1 | 8 | yield-pinned-fifo1 | 1 | 0 / 360000 | 0 | 0 | 1.988 |
| followup | 8 | 1 | 8 | yield-pinned-rr1 | 1 | 0 / 360000 | 0 | 0 | 1.988 |
| followup | 8 | 1 | 16 | yield-pinned | 1 | 0 / 360000 | 0 | 0 | 1.983 |
| followup | 8 | 1 | 16 | yield-pinned-fifo1 | 1 | 0 / 360000 | 0 | 0 | 1.988 |
| followup | 8 | 1 | 16 | yield-pinned-rr1 | 1 | 0 / 360000 | 0 | 0 | 1.988 |
| followup | 16 | 4 | 4 | yield-pinned | 1 | 13 / 180000 | 38 | 0 | 1.983 |
| followup | 16 | 4 | 4 | yield-pinned-fifo1 | 1 | 0 / 180000 | 0 | 0 | 1.991 |
| followup | 16 | 4 | 4 | yield-pinned-rr1 | 1 | 0 / 180000 | 0 | 0 | 1.991 |
| followup | 16 | 4 | 8 | yield-pinned | 1 | 0 / 180000 | 73 | 0 | 1.981 |
| followup | 16 | 4 | 8 | yield-pinned-fifo1 | 1 | 0 / 180000 | 0 | 0 | 1.992 |
| followup | 16 | 4 | 8 | yield-pinned-rr1 | 1 | 0 / 180000 | 0 | 0 | 1.992 |
| followup | 16 | 4 | 16 | spin-pinned-gap1000 | 1 | 0 / 180000 | 0 | 0 | 1.986 |
| followup | 16 | 4 | 16 | yield-pinned-fifo1 | 1 | 0 / 180000 | 0 | 0 | 1.991 |
| followup | 16 | 4 | 16 | yield-pinned-rr1 | 1 | 0 / 180000 | 0 | 0 | 1.991 |
| followup | 128 | 8 | 8 | client-spin-pinned | 1 | 0 / 22504 | 0 | 0 | 1.989 |
| followup | 128 | 8 | 8 | yield-pinned-fifo1 | 1 | 0 / 22504 | 0 | 0 | 1.993 |
| followup | 128 | 8 | 8 | yield-pinned-rr1 | 1 | 0 / 22504 | 0 | 0 | 1.994 |
| followup | 128 | 8 | 16 | spin-pinned | 1 | 0 / 22504 | 0 | 0 | 1.989 |
| followup | 128 | 8 | 16 | yield-pinned | 1 | 0 / 22504 | 4 | 0 | 1.989 |
| followup | 128 | 8 | 16 | yield-pinned-fifo1 | 1 | 0 / 22504 | 0 | 0 | 1.993 |
| followup | 128 | 8 | 16 | yield-pinned-rr1 | 1 | 0 / 22504 | 0 | 0 | 1.993 |
