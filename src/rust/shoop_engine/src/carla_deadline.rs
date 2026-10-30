use std::io;
use std::time::{Duration, Instant};

pub(crate) fn encode(deadline: Instant) -> io::Result<u64> {
    let now = monotonic_nanos()?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    Ok(now.saturating_add(remaining.as_nanos().min(u64::MAX as u128) as u64))
}

pub(crate) fn decode(deadline: u64) -> io::Result<Option<Instant>> {
    if deadline == 0 {
        return Ok(None);
    }
    let now = Instant::now();
    let remaining = deadline.saturating_sub(monotonic_nanos()?);
    now.checked_add(Duration::from_nanos(remaining))
        .map(Some)
        .ok_or_else(|| io::ErrorKind::InvalidData.into())
}

#[cfg(unix)]
fn monotonic_nanos() -> io::Result<u64> {
    let mut time: libc::timespec = unsafe { std::mem::zeroed() };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((time.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(time.tv_nsec as u64))
}

#[cfg(windows)]
fn monotonic_nanos() -> io::Result<u64> {
    #[link(name = "kernel32")]
    extern "system" {
        fn QueryPerformanceCounter(counter: *mut i64) -> i32;
        fn QueryPerformanceFrequency(frequency: *mut i64) -> i32;
    }
    let mut counter = 0;
    let mut frequency = 0;
    if unsafe { QueryPerformanceFrequency(&mut frequency) } == 0
        || unsafe { QueryPerformanceCounter(&mut counter) } == 0
        || frequency <= 0
        || counter < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(((counter as u128 * 1_000_000_000) / frequency as u128).min(u64::MAX as u128) as u64)
}

#[cfg(not(any(unix, windows)))]
fn monotonic_nanos() -> io::Result<u64> {
    Err(io::ErrorKind::Unsupported.into())
}
