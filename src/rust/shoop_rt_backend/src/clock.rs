//! How engine time passes, as seen from the adapter.

use std::time::Duration;

/// Supplied by the composition root together with the carrier. A host that renders on its
/// own thread passes time by itself; an offline host renders when asked.
pub trait EngineClock {
    /// The application let `elapsed` pass, as in `Backend::advance`.
    fn advance(&mut self, elapsed: Duration) {
        let _ = elapsed;
    }

    /// Lets the engine make progress while the adapter waits for its answers. Returns false
    /// where waiting is impossible, such as on a browser's main thread.
    fn wait(&mut self) -> bool;
}

/// For environments that must not block: waiting returns at once.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoWait;

impl EngineClock for NoWait {
    fn wait(&mut self) -> bool {
        false
    }
}

/// For hosts that render on their own thread: waiting sleeps for `interval`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
pub struct Sleep {
    pub interval: Duration,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for Sleep {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(1),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl EngineClock for Sleep {
    fn wait(&mut self) -> bool {
        std::thread::sleep(self.interval);
        true
    }
}
