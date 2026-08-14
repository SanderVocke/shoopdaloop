//! Runtime guard for project-owned mutex acquisition in realtime sections.
//!
//! Unlike allocation, mutex acquisition has no process-wide hook. [`Mutex`]
//! therefore wraps project-owned standard mutexes and checks each acquisition
//! against a thread-local realtime scope.

use std::cell::Cell;
use std::fmt;
use std::panic::Location;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{LockResult, Mutex as StdMutex, MutexGuard, TryLockResult};

static ENABLED: AtomicBool = AtomicBool::new(false);
static FIRST_VIOLATION: AtomicPtr<Location<'static>> = AtomicPtr::new(std::ptr::null_mut());

thread_local! {
    static REALTIME_DEPTH: Cell<u32> = const { Cell::new(0) };
    static PERMISSION_DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct DepthGuard {
    depth: &'static std::thread::LocalKey<Cell<u32>>,
}

impl DepthGuard {
    fn enter(depth: &'static std::thread::LocalKey<Cell<u32>>) -> Self {
        depth.with(|value| value.set(value.get().saturating_add(1)));
        Self { depth }
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        self.depth.with(|value| value.set(value.get() - 1));
    }
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn forbid_locks_if_enabled<T>(f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }
    let _scope = DepthGuard::enter(&REALTIME_DEPTH);
    f()
}

pub fn allow_lock<T>(_reason: &'static str, f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }
    let _scope = DepthGuard::enter(&PERMISSION_DEPTH);
    f()
}

pub fn first_violation() -> Option<&'static Location<'static>> {
    let location = FIRST_VIOLATION.load(Ordering::Acquire);
    if location.is_null() {
        None
    } else {
        // SAFETY: only `&'static Location` pointers are stored here.
        Some(unsafe { &*location })
    }
}

#[cfg(test)]
fn clear_first_violation() {
    FIRST_VIOLATION.store(std::ptr::null_mut(), Ordering::Release);
}

#[track_caller]
fn check_lock_attempt() {
    if !enabled() {
        return;
    }
    let forbidden = REALTIME_DEPTH.with(|realtime| realtime.get() > 0)
        && PERMISSION_DEPTH.with(|permission| permission.get() == 0);
    if !forbidden {
        return;
    }

    let location = Location::caller();
    let _ = FIRST_VIOLATION.compare_exchange(
        std::ptr::null_mut(),
        std::ptr::from_ref(location).cast_mut(),
        Ordering::AcqRel,
        Ordering::Acquire,
    );

    #[cfg(test)]
    panic!("unapproved mutex acquisition in realtime section");

    #[cfg(not(test))]
    std::process::abort();
}

pub struct Mutex<T: ?Sized> {
    inner: StdMutex<T>,
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: StdMutex::new(value),
        }
    }

    pub fn into_inner(self) -> LockResult<T> {
        self.inner.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    #[track_caller]
    pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        check_lock_attempt();
        self.inner.lock()
    }

    #[track_caller]
    pub fn try_lock(&self) -> TryLockResult<MutexGuard<'_, T>> {
        check_lock_attempt();
        self.inner.try_lock()
    }

    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        self.inner.get_mut()
    }

    pub fn is_poisoned(&self) -> bool {
        self.inner.is_poisoned()
    }

    pub fn clear_poison(&self) {
        self.inner.clear_poison();
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    #[track_caller]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        check_lock_attempt();
        self.inner.fmt(formatter)
    }
}

#[macro_export]
macro_rules! realtime_allow_lock {
    ($reason:literal, $acquire:expr) => {{
        $crate::realtime_lock_guard::allow_lock($reason, || $acquire)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tracy_nextest_capture::tracy_capture_test]
    fn realtime_detection_contract() {
        clear_first_violation();
        set_enabled(false);
        let mutex = Mutex::new(1_u32);
        forbid_locks_if_enabled(|| {
            *mutex.lock().unwrap() += 1;
        });
        assert_eq!(*mutex.lock().unwrap(), 2);
        assert!(first_violation().is_none());

        set_enabled(true);
        let violation = std::panic::catch_unwind(|| {
            forbid_locks_if_enabled(|| {
                let _guard = mutex.lock().unwrap();
            });
        });
        assert!(violation.is_err());
        assert!(first_violation().is_some());

        forbid_locks_if_enabled(|| {
            {
                let mut guard =
                    crate::realtime_allow_lock!("test permission", mutex.lock()).unwrap();
                *guard += 1;
            }
            forbid_locks_if_enabled(|| {
                let _guard =
                    crate::realtime_allow_lock!("nested test permission", mutex.try_lock())
                        .unwrap();
            });
        });

        let leaked = std::panic::catch_unwind(|| {
            forbid_locks_if_enabled(|| {
                let _guard = mutex.try_lock().unwrap();
            });
        });
        assert!(leaked.is_err());

        let shared = Arc::new(Mutex::new(0_u32));
        let other = Arc::clone(&shared);
        std::thread::spawn(move || {
            *other.lock().unwrap() = 1;
        })
        .join()
        .unwrap();
        assert_eq!(*shared.lock().unwrap(), 1);
        set_enabled(false);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn wrapper_preserves_poisoning_and_mutable_access() {
        let mut mutex = Mutex::new(1_u32);
        *mutex.get_mut().unwrap() = 2;
        assert_eq!(mutex.into_inner().unwrap(), 2);

        let mutex = Arc::new(Mutex::new(0_u32));
        let poisoned = Arc::clone(&mutex);
        let _ = std::thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("poison checked mutex");
        })
        .join();
        assert!(mutex.is_poisoned());
        mutex.clear_poison();
        assert!(!mutex.is_poisoned());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn checked_guard_interoperates_with_condvar() {
        let pair = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let worker = Arc::clone(&pair);
        let thread = std::thread::spawn(move || {
            let (mutex, cv) = &*worker;
            *mutex.lock().unwrap() = true;
            cv.notify_one();
        });
        let (mutex, cv) = &*pair;
        let mut ready = mutex.lock().unwrap();
        while !*ready {
            ready = cv.wait(ready).unwrap();
        }
        thread.join().unwrap();
    }
}
