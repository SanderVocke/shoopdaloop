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
WebAssembly compilation and the complete standalone media pipeline check were
still running when this note was written; consult the integration commit's final
validation record and subsequent CI before treating those gates as passed.

The same merged coverage run passes its first 3,551 tests, then fails three
hosted-Carla checks: two expect preallocated instance counts while the new base
uses dynamic providers; driver-switch recovery expects refusal for an oversized
period that the new provider clamps. Preserve these failures for integration
review. No FPGA experiment changes those contracts, and this branch does not
weaken their assertions or apply an unproven host timing fix.
