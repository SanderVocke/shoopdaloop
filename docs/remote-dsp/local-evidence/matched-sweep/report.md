# Localhost network DSP measurements

Actual separate-process UDP measurements; no hardware NIC or FPGA was used.

Recorded 1 runs/refusals: 1 measured, 0 rejected, 0 correctness/infrastructure failures.
Verified 5,760,000 received audio samples; 0 incorrect samples; 0 explicitly unverified samples.

## Lowest allowance with no observed misses in all baseline repetitions

Values are packet periods. Every repetition must have zero output deadline misses, zero input concealment/source skips, verified samples and a healthy stream. An em dash means no tested allowance met those conditions.

| P | k=1 | k=2 | k=4 | k=8 |
| --- | --- | --- | --- | --- |
| 8 | — | — | — | — |
| 16 | — | — | — | — |
| 32 | — | — | — | — |
| 64 | — | — | — | — |
| 128 | — | — | — | — |

## Follow-up runs

| P | k | N | seconds | miss fraction | reason |
| --- | --- | --- | --- | --- | --- |
| 8 | 1 | 16 | 60 | 5.555555555555556e-06 | fixed cloud followup configuration for matched comparison |

## Interpretation and reproduction

Packet RTT includes aggregation for early packets in each batch. Batch turnaround starts at actual final-input submission and requires a complete, unconcealed batch. Server DSP durations use only its local clock.
Sender lateness is measured against nominal readiness. Consumption deadlines do not move when a process wakes late. Missing and late responses remain in the miss-rate denominator; valid late audio is still checked for sample correctness.
Tail percentiles describe finite observed samples, not worst-case guarantees. Inspect histogram counts before interpreting rare tails. Shared-host process scheduling and loopback omit physical-network behavior.

The archive contains every per-run report, commands, configuration, socket buffers, counters, missing masks, timing histograms (1 microsecond bins), exact observed percentile summaries and sample counts. The manifest records source/binary/runner hashes and host conditions.

```sh
cargo build --release -p shoop_dsp_host --no-default-features
python3 scripts/sweep_remote_dsp.py --profile all --output work/remote-dsp-sweep
python3 scripts/sweep_remote_dsp.py --report-only --output work/remote-dsp-sweep
```

See `manifest.json`, `summary.csv` and `results.jsonl.gz` alongside this report. No RT priority, affinity or kernel/network tuning was applied.
