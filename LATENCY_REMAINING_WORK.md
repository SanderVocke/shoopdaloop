# Dry/wet latency follow-up

The ordinary-recording limitation previously documented here is addressed by
`LATENCY_DRYWET.md` and its implementation.

A regular simultaneous dry/wet recording now derives per-channel annotations
from two independent effective values:

- Direct and Dry capture alignment: `R`
- Wet capture alignment: `R + P`

`R` is the signed recording alignment and `P` is the non-negative processor
latency. Retention is prepared per channel, so the dry and wet windows can require
different preroll or postroll. Normal Wet playback and logical export use the
stored Wet annotation, while play-dry-through-wet advances Dry media by `P`; the
processor value is therefore applied exactly once.

Recording alignment and processor latency each have Automatic, Manual, and
Automatic + trim modes. Carla is not inspected or inferred: its processor
automatic baseline is zero, and users provide compensation through Manual or
Automatic + trim.

Completed processed takes expose both a common alignment correction and a
wet-relative processor correction. Both are retained-window checked and atomic.
