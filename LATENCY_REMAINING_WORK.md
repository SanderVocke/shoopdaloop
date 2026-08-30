Yes, with one important qualification.

### Normal simultaneous recording

For a fresh ordinary recording, all recording channels on the track—including dry and wet channels—latch the same track-level recording alignment.

The system does **not** calculate:

- dry capture alignment = X
- wet capture alignment = X + processor latency

Therefore, if the wet signal passes through a delayed processor during ordinary simultaneous recording, that relative delay remains embedded in the wet media. Automatic recording alignment does not correct it separately.

The take-alignment control also applies a common delta to all channels, deliberately preserving existing dry/wet differences rather than changing their relative timing.

### Dry-into-wet rendering

Processor compensation is applied in the special **Record dry into wet** workflow:

- Dry playback is advanced by the configured **Processor** amount.
- The delayed wet result is recorded at canonical timing.
- The resulting wet channel does not apply that processor advance again during normal playback.

### Practical limitation

So if the expected behavior is:

> “Record dry and processed wet simultaneously and have them automatically phase/time aligned despite processor latency”

then the current model does **not** provide that. The supported compensated workflow is:

1. Record the dry take with recording alignment.
2. Configure the manual **Processor** advance.
3. Render/record dry into wet.

There is no independent wet-channel alignment control or automatic processor-latency detection. That is a real limitation of the simplified design, not merely a UI omission.
