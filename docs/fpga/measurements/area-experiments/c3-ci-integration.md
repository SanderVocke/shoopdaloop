# C3 CI integration follow-up

Measured C3 remains `607c8a0f` (HDL `fe2184ea`) on immutable B0 `f2180875`.
During its PR run the backend base advanced to `34f04abd` (Remote Carla FX).
GitHub tested merge `1ba135dd`, whose parents are that base and C3. No FPGA HDL
changed in the new base. Keep this integration separate from experiment controls.

Reproduced failures and focused corrections on `area/base-integration`:

- Firmware run 45082037270: nested `Cargo.lock` omits the new `rtrb` dependency
  of `shoop_rt_engine`. The original C3 head passes the locked build with pinned
  Rust 1.99.1; the merged graph fails `--locked`. Adding that single dependency
  entry passes the pinned locked ARM release check, without version upgrades.
- Packaged native checks in workspace run 45082037273: dynamic remote Carla
  kinds without a default server advertise bus support, while the bus catalog
  correctly requires a default. Apply that requirement to the endpoint capability
  too; preserve per-track remote targets. An actual offline prepared engine with
  remote-only providers excludes bus support; adding a local provider restores it.
- Browser release: native-reference backend initialization reads desktop-only
  `carla.hosting_mode`, which its registry deliberately omits on WebAssembly.
  Restrict that lookup to desktop builds. The registered-default settings test
  now also resolves processor settings on each target, with disabled local Carla
  on WebAssembly and the original in-process choice on desktop.
- FPGA standalone job 137392124825: strict `--Wall` rejects intentionally
  unused page-alignment bits and the high bits of the wide cache task operand
  after C3 narrowing. Constant-zero unused reductions follow the existing RTL
  convention; retain the warning gate. These have no observable outputs. Final
  selected physical/source qualification must include the resulting source hashes.

Focused native engine capability and settings tests pass with `-D warnings`.
Formatting and the test-attribute guard pass. The locked ARM check passes.
WebAssembly compilation passes with warning denial (4m07s). Both media_store
and media_audio_pipeline pass the unchanged strict lint and generic/ECP5
synthesis gates. These focused passes do not replace the remaining full CI gate.

The same merged coverage run passes its first 3,551 tests, then fails three
hosted-Carla checks: two expect preallocated instance counts while the new base
uses dynamic providers; driver-switch recovery expects refusal for an oversized
period that the new provider clamps. Preserve these failures for integration
review. No FPGA experiment changes those contracts, and this branch does not
weaken their assertions or apply an unproven host timing fix.

The same integration failures persist on selected C4's tested merge. A pinned
Linux Carla component from successful C2 CI artifact14773879496 was verified
against the current runtime lock and all 430 file hashes. With the real runtime
required, the first companion test reproduces the obsolete preallocated-count
assertion exactly. Updating its startup assertions to on-demand availability,
zero preallocated chains and zero startup reservation preserves every existing
real audio/MIDI/state/recovery assertion. Both hosting modes pass (15.71 seconds).
Formatting, test-attribute policy and a warning-denying companion build pass.
See `c4-ci-carla-on-demand-*`. The wide-chain and driver-period legacy expectations
remain unresolved; this is not a claim of green CI or completed integration.
