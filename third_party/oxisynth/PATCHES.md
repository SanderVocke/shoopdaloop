# OxiSynth realtime preparation patch

Source: the published `oxisynth` 0.1.0 crate from
https://crates.io/crates/oxisynth/0.1.0 . Original crate SHA-256:
`68d089fc07e074f57ac6698292ce3da05eac1d282b821eb4156d4c7d8e36c6ec`.
The original manifest and LGPL-2.1 license are retained beside this file.
The existing logo is unchanged. Shoop uses this pinned source through Cargo's
crates.io patch; it does not modify the registry cache.

Changes:

- `Synth::prepare_realtime()` explicitly initializes all six conversion tables
  and interpolation tables, reserves the configured full voice capacity, and
  touches spare storage as `MaybeUninit` without creating invalid voice values.
  Call before activation and again off-render after changing sample rate/polyphony.
- Note-on initializes its fixed 64-entry modulator array directly, avoiding the
  upstream temporary Vec allocation and free for every note.
- The opt-in `realtime` feature suppresses dependency logging locally, without
  changing the global `log` crate's compile-time filters for the rest of the host.
- Published example declarations are omitted because example assets are not vendored.

The synthesis algorithm, public event API, preset semantics and default behavior
without the realtime feature are preserved. Source changes are distributed with
the application; keep this source/license and patch provenance in release recipes.

`reset_realtime()` clears voices/channels, cached 64-frame output and tails
without reallocating or rebuilding chorus tables. The two sibling patched effect
crates add allocation-free tail-clearing methods; their provenance is documented
independently. Ordinary Panic/reset paths retain their existing tail semantics.
