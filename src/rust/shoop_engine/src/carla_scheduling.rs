#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SchedulingRequest {
    pub policy: i32,
    pub priority: i32,
    pub ceiling: i32,
    pub parameters: [u32; 4],
}

impl SchedulingRequest {
    fn new(policy: i32, priority: i32, ceiling: i32) -> Self {
        Self {
            policy,
            priority,
            ceiling,
            parameters: [0; 4],
        }
    }

    pub fn for_consumer(ceiling: i32) -> Self {
        platform::capture(ceiling)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchedulingReport {
    Normal,
    Realtime { policy: i32, priority: i32 },
    Fallback { error: i32 },
}

pub(crate) struct WorkerScheduling {
    last_report: Option<(SchedulingRequest, SchedulingReport)>,
    #[cfg(target_os = "linux")]
    budget: crate::carla_budget::BudgetMonitor,
    #[cfg(target_os = "linux")]
    reported_overruns: u64,
}

impl WorkerScheduling {
    pub fn new() -> std::io::Result<Self> {
        platform::ordinary()?;
        Ok(Self {
            last_report: None,
            #[cfg(target_os = "linux")]
            budget: crate::carla_budget::BudgetMonitor::new()?,
            #[cfg(target_os = "linux")]
            reported_overruns: 0,
        })
    }

    pub fn enter(
        &self,
        request: SchedulingRequest,
        _deadline: Option<std::time::Instant>,
    ) -> (SchedulingGuard, SchedulingReport) {
        #[cfg(target_os = "linux")]
        {
            platform::enter(request, &self.budget, _deadline)
        }
        #[cfg(not(target_os = "linux"))]
        {
            platform::enter(request)
        }
    }

    // Report only after the DSP scheduling scope has restored ordinary scheduling.
    pub fn report(&mut self, request: SchedulingRequest, report: SchedulingReport) {
        #[cfg(target_os = "linux")]
        {
            let overruns = self.budget.overruns();
            if overruns != self.reported_overruns {
                self.reported_overruns = overruns;
                tracing::warn!(
                    overruns,
                    "Carla DSP exceeded its budget and was demoted to ordinary scheduling"
                );
                if !tracing::enabled!(tracing::Level::WARN) {
                    eprintln!(
                        "Carla DSP budget overruns: {overruns}; demoted to ordinary scheduling"
                    );
                }
            }
        }
        if self.last_report == Some((request, report)) {
            return;
        }
        self.last_report = Some((request, report));
        if report != SchedulingReport::Normal && !tracing::enabled!(tracing::Level::INFO) {
            eprintln!("Carla audio worker scheduling: {request:?} => {report:?}");
        }
        match report {
            SchedulingReport::Fallback { .. } => tracing::warn!(
                ?request,
                ?report,
                "Carla worker scheduling unavailable; using bounded sleeping completion waits"
            ),
            _ => tracing::info!(
                ?request,
                ?report,
                "Carla worker scheduling negotiated and verified"
            ),
        }
    }
}

pub(crate) use platform::Guard as SchedulingGuard;

#[cfg(any(target_os = "linux", windows))]
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub fn capture(ceiling: i32) -> SchedulingRequest {
        unsafe {
            let raw_policy = libc::sched_getscheduler(0);
            let policy = raw_policy & !libc::SCHED_RESET_ON_FORK;
            let mut parameters: libc::sched_param = std::mem::zeroed();
            if raw_policy < 0 || libc::sched_getparam(0, &mut parameters) != 0 {
                return SchedulingRequest::new(-1, 0, ceiling);
            }
            if matches!(policy, libc::SCHED_FIFO | libc::SCHED_RR) {
                // A sleeping caller yields to DSP but must preempt it at the deadline.
                let ceiling = ceiling
                    .min(parameters.sched_priority)
                    .min(libc::sched_get_priority_max(policy));
                return SchedulingRequest::new(policy, ceiling.saturating_sub(1), ceiling);
            }
            if matches!(
                policy,
                libc::SCHED_OTHER | libc::SCHED_BATCH | libc::SCHED_IDLE
            ) {
                SchedulingRequest::new(libc::SCHED_OTHER, 0, ceiling)
            } else {
                SchedulingRequest::new(-1, 0, ceiling)
            }
        }
    }

    pub fn ordinary() -> std::io::Result<()> {
        let parameters = libc::sched_param { sched_priority: 0 };
        if unsafe { libc::sched_getscheduler(0) } != libc::SCHED_OTHER
            && unsafe { libc::sched_setscheduler(0, libc::SCHED_OTHER, &parameters) } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { libc::sched_getscheduler(0) } != libc::SCHED_OTHER {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }

    pub(crate) struct Guard {
        previous: Option<(i32, libc::sched_param)>,
        _budget: Option<crate::carla_budget::BudgetTicket>,
    }

    pub fn enter(
        request: SchedulingRequest,
        monitor: &crate::carla_budget::BudgetMonitor,
        deadline: Option<std::time::Instant>,
    ) -> (Guard, SchedulingReport) {
        unsafe {
            let unchanged = Guard {
                previous: None,
                _budget: None,
            };
            if request.policy == libc::SCHED_OTHER && request.priority == 0 {
                return (unchanged, SchedulingReport::Normal);
            }
            if !matches!(request.policy, libc::SCHED_FIFO | libc::SCHED_RR)
                || request.priority <= 0
                || request.priority >= request.ceiling
            {
                return (
                    unchanged,
                    SchedulingReport::Fallback {
                        error: libc::EINVAL,
                    },
                );
            }
            let previous_policy = libc::sched_getscheduler(0);
            let mut previous: libc::sched_param = std::mem::zeroed();
            if previous_policy < 0 || libc::sched_getparam(0, &mut previous) != 0 {
                return (unchanged, SchedulingReport::Fallback { error: errno() });
            }
            let Some(deadline) = deadline else {
                return (
                    unchanged,
                    SchedulingReport::Fallback {
                        error: libc::EINVAL,
                    },
                );
            };
            let parameters = libc::sched_param {
                sched_priority: request.priority,
            };
            if libc::sched_setscheduler(0, request.policy, &parameters) != 0 {
                return (unchanged, SchedulingReport::Fallback { error: errno() });
            }
            let mut guard = Guard {
                previous: Some((previous_policy, previous)),
                _budget: None,
            };
            let mut actual: libc::sched_param = std::mem::zeroed();
            if libc::sched_getscheduler(0) != request.policy
                || libc::sched_getparam(0, &mut actual) != 0
                || actual.sched_priority != request.priority
                || std::time::Instant::now() >= deadline
            {
                drop(guard);
                return (
                    Guard {
                        previous: None,
                        _budget: None,
                    },
                    SchedulingReport::Fallback {
                        error: libc::EINVAL,
                    },
                );
            }
            guard._budget = match monitor.arm(request, deadline) {
                Ok(budget) => Some(budget),
                Err(error) => {
                    drop(guard);
                    return (
                        unchanged,
                        SchedulingReport::Fallback {
                            error: error.raw_os_error().unwrap_or(libc::ETIMEDOUT),
                        },
                    );
                }
            };
            // A previous expired ticket may demote us before the new acknowledgement.
            if libc::sched_getscheduler(0) != request.policy
                || libc::sched_getparam(0, &mut actual) != 0
                || actual.sched_priority != request.priority
                || std::time::Instant::now() >= deadline
            {
                drop(guard);
                return (
                    unchanged,
                    SchedulingReport::Fallback {
                        error: libc::EAGAIN,
                    },
                );
            }
            (
                guard,
                SchedulingReport::Realtime {
                    policy: request.policy,
                    priority: actual.sched_priority,
                },
            )
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some((policy, parameters)) = self.previous {
                // Continuing into control work with an elevated policy is not safe.
                if unsafe { libc::sched_setscheduler(0, policy, &parameters) } != 0 {
                    std::process::abort();
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    const TIME_CONSTRAINT: i32 = 3;

    fn read() -> Result<Option<[u32; 4]>, i32> {
        unsafe {
            let thread = libc::pthread_mach_thread_np(libc::pthread_self());
            let mut parameters = [0_u32; 4];
            let mut count = libc::THREAD_TIME_CONSTRAINT_POLICY_COUNT;
            let mut default = 0;
            let error = libc::thread_policy_get(
                thread,
                libc::THREAD_TIME_CONSTRAINT_POLICY as libc::thread_policy_flavor_t,
                parameters.as_mut_ptr().cast(),
                &mut count,
                &mut default,
            );
            if error != libc::KERN_SUCCESS {
                return Err(error);
            }
            if count != libc::THREAD_TIME_CONSTRAINT_POLICY_COUNT {
                return Err(libc::KERN_INVALID_ARGUMENT);
            }
            Ok((default == 0).then_some(parameters))
        }
    }

    fn set(parameters: Option<[u32; 4]>) -> Result<(), i32> {
        unsafe {
            let thread = libc::pthread_mach_thread_np(libc::pthread_self());
            let error = if let Some(mut parameters) = parameters {
                libc::thread_policy_set(
                    thread,
                    libc::THREAD_TIME_CONSTRAINT_POLICY as libc::thread_policy_flavor_t,
                    parameters.as_mut_ptr().cast(),
                    libc::THREAD_TIME_CONSTRAINT_POLICY_COUNT,
                )
            } else {
                libc::thread_policy_set(
                    thread,
                    libc::THREAD_STANDARD_POLICY as libc::thread_policy_flavor_t,
                    std::ptr::null_mut(),
                    libc::THREAD_STANDARD_POLICY_COUNT as u32,
                )
            };
            if error == libc::KERN_SUCCESS {
                Ok(())
            } else {
                Err(error)
            }
        }
    }

    pub fn capture(ceiling: i32) -> SchedulingRequest {
        match read() {
            Ok(Some(parameters)) => SchedulingRequest {
                policy: TIME_CONSTRAINT,
                priority: 0,
                ceiling,
                parameters,
            },
            Ok(None) => SchedulingRequest::new(0, 0, ceiling),
            Err(_) => SchedulingRequest::new(-1, 0, ceiling),
        }
    }

    pub fn ordinary() -> std::io::Result<()> {
        set(None).map_err(|error| std::io::Error::other(format!("Mach scheduling: {error}")))?;
        match read() {
            Ok(None) => Ok(()),
            _ => Err(std::io::ErrorKind::PermissionDenied.into()),
        }
    }

    pub(crate) struct Guard {
        previous: Option<Option<[u32; 4]>>,
    }

    pub fn enter(request: SchedulingRequest) -> (Guard, SchedulingReport) {
        let unchanged = Guard { previous: None };
        if request.policy == 0 {
            return (unchanged, SchedulingReport::Normal);
        }
        if request.policy != TIME_CONSTRAINT
            || request.parameters[1] == 0
            || request.parameters[1] > request.parameters[2]
            || request.parameters[3] > 1
        {
            return (
                unchanged,
                SchedulingReport::Fallback {
                    error: libc::KERN_INVALID_ARGUMENT,
                },
            );
        }
        let previous = match read() {
            Ok(previous) => previous,
            Err(error) => return (unchanged, SchedulingReport::Fallback { error }),
        };
        if let Err(error) = set(Some(request.parameters)) {
            return (unchanged, SchedulingReport::Fallback { error });
        }
        let guard = Guard {
            previous: Some(previous),
        };
        if read() != Ok(Some(request.parameters)) {
            drop(guard);
            return (
                Guard { previous: None },
                SchedulingReport::Fallback {
                    error: libc::KERN_FAILURE,
                },
            );
        }
        (
            guard,
            SchedulingReport::Realtime {
                policy: TIME_CONSTRAINT,
                priority: 0,
            },
        )
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous {
                if set(previous).is_err() {
                    std::process::abort();
                }
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::c_void;
    const MMCSS: i32 = 4;
    const AVRT_PRIORITY_HIGH: i32 = 1;

    #[link(name = "avrt")]
    extern "system" {
        fn AvSetMmThreadCharacteristicsW(task: *const u16, index: *mut u32) -> *mut c_void;
        fn AvSetMmThreadPriority(handle: *mut c_void, priority: i32) -> i32;
        fn AvRevertMmThreadCharacteristics(handle: *mut c_void) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn GetThreadPriority(thread: *mut c_void) -> i32;
        fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
    }

    pub fn capture(ceiling: i32) -> SchedulingRequest {
        SchedulingRequest::new(MMCSS, AVRT_PRIORITY_HIGH, ceiling)
    }

    pub fn ordinary() -> std::io::Result<()> {
        unsafe {
            let thread = GetCurrentThread();
            if SetThreadPriority(thread, 0) == 0 || GetThreadPriority(thread) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    pub(crate) struct Guard {
        handle: *mut c_void,
    }

    pub fn enter(request: SchedulingRequest) -> (Guard, SchedulingReport) {
        let unchanged = Guard {
            handle: std::ptr::null_mut(),
        };
        if request.policy == 0 {
            return (unchanged, SchedulingReport::Normal);
        }
        if request.policy != MMCSS || request.priority != AVRT_PRIORITY_HIGH {
            return (unchanged, SchedulingReport::Fallback { error: 87 });
        }
        let task = [80_u16, 114, 111, 32, 65, 117, 100, 105, 111, 0];
        let mut index = 0;
        let handle = unsafe { AvSetMmThreadCharacteristicsW(task.as_ptr(), &mut index) };
        if handle.is_null() {
            return (unchanged, SchedulingReport::Fallback { error: errno() });
        }
        let guard = Guard { handle };
        if unsafe { AvSetMmThreadPriority(handle, request.priority) } == 0
            || unsafe { GetThreadPriority(GetCurrentThread()) } == i32::MAX
        {
            let error = errno();
            drop(guard);
            return (
                Guard {
                    handle: std::ptr::null_mut(),
                },
                SchedulingReport::Fallback { error },
            );
        }
        (
            guard,
            SchedulingReport::Realtime {
                policy: MMCSS,
                priority: request.priority,
            },
        )
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if !self.handle.is_null()
                && unsafe { AvRevertMmThreadCharacteristics(self.handle) } == 0
            {
                std::process::abort();
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::*;
    pub fn capture(ceiling: i32) -> SchedulingRequest {
        SchedulingRequest::new(-2, 0, ceiling)
    }
    pub fn ordinary() -> std::io::Result<()> {
        Ok(())
    }
    pub(crate) struct Guard;
    pub fn enter(_request: SchedulingRequest) -> (Guard, SchedulingReport) {
        (Guard, SchedulingReport::Fallback { error: -2 })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn unavailable_priority_is_explicit_and_preserves_callers_policy() {
        let before = unsafe { libc::sched_getscheduler(0) };
        let request = SchedulingRequest::new(libc::SCHED_FIFO, 1, 0);
        let scheduling = WorkerScheduling::new().unwrap();
        let (guard, report) = scheduling.enter(
            request,
            Some(std::time::Instant::now() + std::time::Duration::from_secs(1)),
        );
        assert_eq!(
            report,
            SchedulingReport::Fallback {
                error: libc::EINVAL
            }
        );
        assert_eq!(unsafe { libc::sched_getscheduler(0) }, before);
        drop(guard);
        assert_eq!(unsafe { libc::sched_getscheduler(0) }, before);
    }
}
