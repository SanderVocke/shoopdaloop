//! Whether a missing system backend is a test failure or a skip.
//!
//! It used to be neither: tests that could not reach JACK, ALSA or a MIDI port returned
//! early, which reports as a **pass**. CI has never run a `jackd`, so the JACK integration
//! tests were green for years without executing a line -- including the one covering the
//! output path that turned out to be broken.
//!
//! Default is now to fail. `SHOOP_ALLOW_MISSING_BACKENDS=1` downgrades that to a skip, for
//! a developer machine or container that genuinely has no audio stack.

/// Call when a backend could not be reached. Panics unless skipping was opted into.
///
/// Returning rather than diverging so a caller can `return` from the test itself, which
/// keeps the bail-out visible at the call site instead of buried in here.
pub fn require_backend(what: &str, detail: &str) {
    if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() {
        eprintln!("skipping: {what} unavailable ({detail}); SHOOP_ALLOW_MISSING_BACKENDS is set");
        return;
    }
    panic!(
        "{what} is required by this test but unavailable: {detail}.\n\
         Start the backend (CI runs `jackd -d dummy`), or set \
         SHOOP_ALLOW_MISSING_BACKENDS=1 to skip backend-dependent tests."
    );
}
