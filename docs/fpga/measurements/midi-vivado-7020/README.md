# Retained Vivado 2021.1 study

See the [report](../../midi-vivado-7020.md) for conclusions and scope. Four fresh runs compare audio alone with audio plus MIDI at 100/150 MHz on `xc7z020clg484-1`. All timings use 0.2 ns uncertainty and single-cycle register paths, with no multicycle/false-path timing exceptions.

Each run directory retains the actual registered context, constraints, implementation script, provenance, original/measured source hashes, compatibility patches, console log and stage reports. `before-*` reaches full routing; `after-*` fails placement. Reports named `post_opt` are unplaced, and `timing-estimates` reads that optimized checkpoint to examine each component separately. It does not produce a routed MIDI clock result.

Checkpoints and vendor files are not committed. Source snapshots can be regenerated from the recorded Git revisions and compatibility patches. The tool provenance records the verified public image layers, original vendor-file hash, compiler version and Docker runtime. The runtime adds a non-root account to the original OS filesystem and mounts the original vendor tree read-only; it applies no later image overrides.

The source preparer and implementation scripts are under `fpga/synthesis/` at the repository root. Run its commands from the repository root, then run Vivado from each generated output directory. With Python/Matplotlib installed, run `collect_results.py` here to reconstruct `results.json` from the retained reports, then `plot_resources.py` to regenerate the resource figure.

### Build configuration

| Setting | Audio | MIDI |
| --- | ---: | ---: |
| Loop/session sizing | 64 stereo loops / 128 mono sequences | 64 sequence bindings |
| Input / output ports | 2 / 2 sample ports | 1 / 8 packet destinations |
| Quantum / audio chunk | 32 / 32 frames | 32 frames / no audio-style chunking |
| Instructions / state words / jobs | 1,024 per program bank / 256 / 1,024 | Separate MIDI controller |
| Program banks / scratch buffers | 2 / 512 × 32 frames | — |
| Audio view / table-page / retirement capacities | 512 / 16,119 / 256 | — |
| Audio allocation geometry | 5,763,072 chunks / 32 chunks per block | — |
| Audio sample caches | 4 read / 4 write buffers | No cache |
| Audio metadata caches | 128 table / 4 reference / 2 free-stack lines | No cache |
| Event / block capacities | — | 8,192 per sequence / 2,432 × 256 events |
| View / snapshot / retirement capacities | — | 256 / 384 / 128 |
| Accepted input events / maintenance ceiling | — | 256 / 64 per quantum |

The audio capacities allow a straightforward graph with separate playback, gain, mix and capture work for each mono channel, and shared stereo I/O. They are capacities, not 1,024 jobs forced to execute per quantum. The current media wrapper inherits 256 SP loop-record slots from its state-word capacity, even though this session uses 64 loops. Both sides include this same reserved RAM.

### Rebuild

With Vivado 2021.1 on `PATH`, run the following for each of `before` / `after` and `100` / `150`:

```sh
python3 fpga/synthesis/midi_session.py before --mhz 100 --output work/midi-vivado/before-100
```

Then enter the prepared output directory and run:

```sh
vivado -mode batch -nojournal -log vivado.log -source run.tcl
```

Use a new output directory for each run. Preparation copies the exact historical source versions, applies the recorded compatibility patches, hashes the resulting sources and writes the complete context and constraints. Placement/routing runs even when setup timing is negative; a completed run is not automatically a timing pass.


### DDR layout

At 48 kHz, 128 audio channels each reserve 1,407 allocation blocks of 1,024 frames. The sample pool occupies 737,673,216 bytes. Reference counts/free IDs, directory pages and retirement records add 2,488,768 bytes. MIDI reserves 19,560,876 bytes, including its logical descriptor window hosted in 8 KiB primary BRAM. The total is 759,722,860 bytes (724.528 MiB), plus caller staging/OS/application memory.

Audio physical placement starts at `0x01000000` and ends at `0x2d1df9c0`. MIDI starts at `0x30000000`; a 64 MiB arena is reserved, ending at `0x34000000`. The ranges do not overlap and fit within 1 GiB. Extra takes, exports and retained history consume finite pools and can still exhaust them.

### Functional validation of compatibility changes

- Full configured measurement composition: Verilator 5.032 fatal lint, no warnings.
- `fpga-testbench statistics --memory-model fixed`: four existing checks pass with view reuse disabled/enabled, including retired storage.
- `fpga-testbench reuse --memory-model fixed`: two existing reuse/refusal checks pass.
- `cargo test -p rt_fpga_testbench --features sim --test midi midi_ddr_session_record_restore_repeat_stop_and_history_grab_match_rust -- --exact`: one reference/RTL comparison passes with warning-denying Rust flags.

The MIDI test covers recording, restore/repeat, stop cleanup and shared history grabs. These are targeted checks for the declaration-order and bit-count compatibility changes; the earlier full MIDI conformance evidence remains in the component's original validation report.
