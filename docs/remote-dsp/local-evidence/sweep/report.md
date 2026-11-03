# Localhost network DSP measurements

Actual separate-process UDP measurements; no hardware NIC or FPGA was used.

Recorded 288 runs/refusals: 257 measured, 31 rejected, 0 correctness/infrastructure failures.
Verified 77,201,408 received audio samples; 0 incorrect samples; 0 explicitly unverified samples.

## Lowest allowance with no observed misses in all baseline repetitions

Values are packet periods. Every repetition must have zero output deadline misses, zero input concealment/source skips, verified samples and a healthy stream. An em dash means no tested allowance met those conditions.

| P | k=1 | k=2 | k=4 | k=8 |
| --- | --- | --- | --- | --- |
| 8 | 8 | — | 16 | — |
| 16 | 1 | 16 | 16 | — |
| 32 | 1 | 2 | 8 | 8 |
| 64 | 1 | 4 | 8 | 8 |
| 128 | 1 | 2 | 8 | 8 |

## Follow-up runs

| P | k | N | seconds | miss fraction | reason |
| --- | --- | --- | --- | --- | --- |
| 128 | 8 | 16 | 60.0 | 0.0 | adjacent tested allowance to the observed boundary |
| 128 | 8 | 8 | 60.0 | 0.0 | lowest observed miss-free baseline allowance |
| 16 | 4 | 16 | 60.0 | 0.0 | lowest observed miss-free baseline allowance |
| 16 | 4 | 8 | 60.0 | 0.0 | adjacent tested allowance to the observed boundary |
| 8 | 1 | 4 | 60.0 | 3.611111111111111e-05 | adjacent tested allowance to the observed boundary |
| 8 | 1 | 8 | 60.0 | 0.0 | lowest observed miss-free baseline allowance |

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
