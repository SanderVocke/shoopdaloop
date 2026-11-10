# Single-core audio backend search

This follow-up to `proto/audio-compact` keeps the prototypes' existing audio-only
scope. It retains a small set of source changes, records 33 experiments, and
preserves unsuccessful approaches so their microbenchmark leads are not mistaken
for whole-callback gains.

All timings are from an AMD EPYC x86_64 VM, with render, host and controller work
pinned to one CPU. Windows are short and comparison order is rotated. Cortex-A76
cross-builds and emulated functional checks passed; **there are no physical
Raspberry Pi 5 measurements**. Tail results remain mixed.

## Retained implementation

- Skip inactive MIDI-free Current nodes while preserving output clearing,
  activity decisions and executed/skipped accounting; allow its MIDI-free clock
  path to use the existing steady advancement.
- Evaluate finite, nonzero constant reference gains without per-sample envelope
  interpolation. Preserve multiplication, ramps, signed zero and nonfinite cases.
- Borrow the private audio visitor's descriptor instead of copying it.
- Narrow validated page-local audio descriptor fields under existing storage
  limits. `AudioSequence` shrinks from 144 to 96 bytes on 64-bit targets;
  frames, epochs and global counters keep their widths.
- Cache unchanged stopped primitive publications only in Compact. Active loops,
  pulses and composites retain publication behavior; storage is prepared and
  touched before rendering.
- Remove an unused resolver state copy. Add standalone audio-kernel/callback
  benchmarks, configurable dense voice counts and paired host comparison tooling.

[Detailed experiment log](OPTIMIZATION_LOG.md) contains each control, validation,
rejection and retained patch. [Evidence](results/single-core-2026-11-09) includes
raw callback records, source/compiler/binary identities and build manifests.

## Build choices and measured gains

Each percentage uses its own matched control; percentages must not be added.

| Comparison | Local normal-host median callback result |
| --- | --- |
| Compact stopped-publication cache versus the prior retained source | 3–11% lower across idle/play/record/grab; grab p99 worsened |
| Packed descriptors/default profile versus previous retained source | Compact 7–14% lower; Current 0–5% lower |
| ThinLTO + one codegen unit versus packed/default profile | Compact 9–12% lower; Current 5–9% lower |
| Balanced host PGO versus the same packed ThinLTO/one-unit source | Compact 5–18% lower; Current 6–10% lower |

The repository's Cargo profiles are unchanged. These are explicit build choices;
PGO training includes all four backends and the normal capture/control workflows.
The strongest prior native build is identified by SHA256
`84b77c0b48e56a78da4db31ba825d7966a1bb69e72537f0faab1a140415d5828`.

## Final profiling experiment

The final borrowed-routing candidate was **rejected and reverted**. Fresh PGO
removed the profile-mismatch warnings but made every tested normal-host median
slower. Four rotated comparisons, 256 callbacks per window at buffer/quantum32:

| Backend / scenario | Prior best p50 / p99, µs | Candidate p50 / p99, µs | p50 change |
| --- | ---: | ---: | ---: |
| current / idle | 34.854 / 69.438 | 38.424 / 109.396 | +10.2% |
| current / play | 49.380 / 131.649 | 54.258 / 154.543 | +9.9% |
| current / record | 62.352 / 155.259 | 67.357 / 159.460 | +8.0% |
| current / grab | 61.716 / 109.617 | 67.109 / 162.730 | +8.7% |
| compact / idle | 21.252 / 37.736 | 22.353 / 81.976 | +5.2% |
| compact / play | 37.465 / 92.656 | 38.792 / 96.242 | +3.5% |
| compact / record | 43.369 / 100.067 | 44.205 / 110.233 | +1.9% |
| compact / grab | 39.373 / 93.663 | 40.923 / 122.942 | +3.9% |

The candidate also produced a larger executable: 12,169,352 versus 11,322,264
bytes. Its default-profile offline lead remains an archived Pi contender, with
its patch and source hashes, but is not in the retained implementation. The
prior packed-source PGO build remains the best verified normal-host solution.

Seventy-two short user-only CPU-clock recordings collected 3,336 samples with no
losses. The diagnostic harness enables sampling after warmup and disables it
before final telemetry validation. Its window includes input fill, output checks
and controller polling; incomplete stacks and 13–22% unknown symbols limit
attribution. Instrumented timings are excluded from performance rankings.

Recurring Compact sample addresses followed the helper returning a 96-byte
routing descriptor. Borrowing that descriptor and prepared metadata passed 156
unit tests, 34 integration tests and 24 ARM functional comparisons. Six balanced
offline rotations improved Compact medians by 5–13%, with other backends broadly
flat. The first host comparison used existing PGO data; compiler warnings showed
that changed CFG profiles were discarded, and host results were mixed. The final
check independently retrained the candidate on the same 36 host workloads.

A build-script mistake initially let an empty `CARGO_ENCODED_RUSTFLAGS` override
PGO flags. That executable was never timed or ranked. Corrected builds record
Cargo's effective compiler flags. Earlier stale-mtime results were also discarded
and repeated with fresh immutable binaries and source hashes.

## Approaches that did not earn retention

Direct-wire borrowing, grouped stereo reads, overwrite-only import, first-route
clearing, fused copy/peak, cold-array boxing, narrower graph indices, quantum
masks and lookup changes produced narrow or inconsistent gains, or regressed
other workloads. Several passed extensive sample/ownership checks and improved
microbenchmarks, but failed the whole-callback screen. Their patches and results
remain in the evidence rather than in the implementation.

## Reproduce

On a Linux host or Pi, build the benchmark targets and save distinct binaries
before changing source or flags:

```sh
cargo build --locked --release -p shoop_rt_engine --features testing \
  --example audio_render_benchmark

taskset -c 0 target/release/examples/audio_render_benchmark \
  compact play 32 8 512 48000
```

For normal-host comparisons, build `shoop_host` with `--no-default-features
--features testing`, save each host executable separately, and retain the same
`dense_chunk_benchmark` launcher for both:

```sh
taskset -c 0 python3 scripts/compare_audio_candidates.py \
  --candidates control=/path/to/control candidate=/path/to/candidate \
  --launcher /path/to/dense_chunk_benchmark --output /path/to/results \
  --mode compact --buffer 32 --callbacks 256 --repeats 4 --dense \
  --scenarios idle play record grab
```

Set `CARGO_PROFILE_RELEASE_LTO=thin` and
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1` for the tested link profile. Train PGO
separately for each changed control flow and architecture; do not transfer x86
profiles to ARM or rank emulator timings. Use `-C target-cpu=cortex-a76` when
building for the Pi 5. Run no compiler or competing benchmark during measurement.

## Validation

- `cargo fmt --all`, the test-attribute policy, and the warnings-denying workspace build passed.
- Targeted runtime/loop/engine suites and 20 normal-host workflows for Current and Compact passed during the search. Cortex-A76 cross-builds and ARM functional checks passed as recorded in the build manifests.
- The complete native gate was **not completed**. The first run passed 2,223 tests before a host-manager child-readiness fixture stalled with the experiment environment's `RUST_TEST_THREADS=1`. The same fixture passed in 2.78 seconds with the repository default of 8; nextest and children remained on CPU0.
- The corrected full run was stopped at the user's request: 592 passed, one active test received SIGINT, 2,601 remained unrun, and 11 were skipped. This is an interrupted gate, not a claim of a green full suite.
- Native tests used nextest 0.9.116, `RUSTFLAGS="-D warnings"`, `SHOOP_ALLOW_MISSING_BACKENDS=1`, one nextest worker, one build job, CPU0, and test debug symbols disabled. Missing ALSA/JACK development files were supplied in a workspace-local prefix.

See [validation.json](results/single-core-2026-11-09/validation.json) and `final-validation.tar.gz` for commands, source identities and logs. Browser and physical-device checks were not run in this search.

