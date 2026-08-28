# Latency diagnostics and remediation

Latency diagnostics are bounded and snapshot-driven. Audio callbacks publish only atomic scalar channel state; backend control polling samples it into fixed 64-point capture-alignment, render-advance, and active-postroll rings. No callback logging, allocation, lock acquisition, or unbounded history is used.

The application status snapshot exposes monotonic edge counters:

- **unresolved recipes** — an enabled operation component could not produce a bounded total; select a manual value, disable the component, or restore its provider;
- **observation changes** — a provider revision changed after an operation latched; the take remains frozen, so compare the take and current values before deciding whether to consolidate or rerecord;
- **insufficient margins** — prepared retained media was shorter than the frozen requirement; reduce compensation, consolidate compatible media, or rerecord;
- **deferred transitions** — playback/render start waited for safe compensated media or render preroll;
- **finalization overruns** — postroll was still active while an insufficient-margin condition was present;
- **path ambiguities** — a normalized host cue identity matched more than one application output; select one application port or remove the duplicate route;
- **provider failures** — an automatic component had no current observation; use a manual replacement or repair the JACK/Carla/browser provider.

Counters increment only when a condition becomes active, not on every poll. Their `u64` arithmetic saturates. Plot length and cursor are bounded by `LATENCY_DIAGNOSTIC_PLOT_SAMPLES`; browser protocol vectors are copied into that fixed bound. The latency panel displays the non-realtime summary, current/frozen values, warnings, and plots.

Stress coverage repeatedly edits policy through maximum supported latency, churns ordinary and advanced transitions, polls diagnostics, and attempts session capture while content is both settled and changing. Existing callback no-allocation tests cover the compensated channel and provider publication paths.
