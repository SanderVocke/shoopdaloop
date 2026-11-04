# FunDSP bounded reset patch

Source: the published `fundsp` 0.23.0 crate at
https://crates.io/crates/fundsp/0.23.0 . Original archive SHA-256:
`6969e5c9d5c5c704723f547237494fde9b1addf16f718f65cd786b3b6e320a4f`.
The original manifest and both MIT/Apache-2.0 licenses are retained.

The opt-in `realtime-reset` feature invalidates Delay, Tap and TapLinear contents
with a valid-write count rather than clearing the whole allocated delay line.
Reads of samples preceding reset return zero. Preparation touches every allocated
delay sample before activation. Tapped SIMD processing uses scalar
processing until the line is initialized, then resumes its original fast path.
The feature also resets the four plate-reverb prediffusers, which the published
reverb reset omitted. This prevents prediffuser history surviving a reset.

Ordinary builds retain the original clearing behavior. Published text has trailing
whitespace normalized and the crate permits deprecated APIs locally, matching the
registry dependency's lint treatment on newer toolchains. No registry cache is
modified. Keep this source, provenance and both licenses with distribution recipes.
