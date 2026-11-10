Audio RT prototype benchmark for Raspberry Pi 5, 64-bit Linux / Cortex-A76.
Static ARM64-musl executables; Python3 is needed for the runner. No audio device,
root privileges, daemon or network service is required. No Pi timings are bundled.

Run on the Pi (choose one allowed CPU, here3):
  python3 run.py --output results --cpu 3 --rounds 4
To include every prototype:
  python3 run.py --output all-results --cpu 3 --rounds 4 --modes current chunks chunks-descriptors compact

Each timed window is512callbacks at32frames or64callbacks at256frames.
All render/control work stays on the selected CPU. Input history is prefilled
before capture; measured setup uses8tracks,8cells/track,4active voices/track,Q32.
The binary asserts finite/nonzero playback, expected modes, lengths and complete
captures. The runner checks checksums across variants, rotates order, records
hashes/machine/affinity/governor and preserves rawCSV plus medianp50/p99 summaries.

original: original branch69b52609, with the same benchmark harness.
retained: constant visitor + Current inactive pruning/steady clock.
packed-descriptors: borrowed visitor, Compact-only stopped publication cache,
unused compact scratch copy removed, narrower opaque audio descriptor indexes.
Source patches and build hashes are included. The standalone harness retains the
benchmark/support/allocator code and omits the unrelated tracing test dependency.
Its registry dependency versions match the project lock; all target variants use
the same standalone lock and harness. The size win is per descriptor,
not a claim about total process RAM. Short p99 samples do not establish WCET.
