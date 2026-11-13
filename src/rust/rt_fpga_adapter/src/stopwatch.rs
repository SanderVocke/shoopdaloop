//! Wall-clock measurement for metrics; without `std` every measurement is zero.
use core::time::Duration;

#[derive(Clone, Copy)]
pub struct Stopwatch(#[cfg(feature = "std")] std::time::Instant);

impl Stopwatch {
    pub fn start() -> Self {
        Self(
            #[cfg(feature = "std")]
            std::time::Instant::now(),
        )
    }
    pub fn elapsed(&self) -> Duration {
        #[cfg(feature = "std")]
        return self.0.elapsed();
        #[cfg(not(feature = "std"))]
        Duration::ZERO
    }
}
