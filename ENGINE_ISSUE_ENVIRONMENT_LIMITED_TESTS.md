# Engine verification is limited by the test environment

## Issue

Some engine and driver behavior cannot be exercised in environments that lack the required audio or MIDI backend devices. The resulting skipped or overridden tests leave parts of integration behavior unverified in those runs.

## Known limitations

- MIDI driver tests cannot use ALSA sequencing when `/dev/snd/seq` is unavailable. Runs using `SHOOP_ALLOW_MISSING_BACKENDS=1` tolerate the absent backend rather than exercising it.
- CPAL device integration tests can be skipped when no suitable CPAL settings or device are available.
- JACK, virtual MIDI, and physical-device behavior depends on services and devices outside the test process.

## Impact

A passing test run in a restricted environment does not confirm operation against the unavailable backends. Device discovery, connection handling, callback behavior, and teardown can differ from mock or skipped paths, so the affected integration coverage remains incomplete until run in a suitably provisioned environment.
