//! Lock-free single-producer single-consumer queue for passing functors
//! to be executed in another thread (typically the audio processing thread).

use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct CxxCommand {
    user_data: usize,
    executed: bool,
}

impl CxxCommand {
    fn new(user_data: usize) -> Self {
        Self {
            user_data,
            executed: false,
        }
    }

    fn execute(mut self) {
        unsafe { crate::command_queue_cxx::ffi::cxx_command_execute_ptr(self.user_data) };
        self.executed = true;
    }
}

impl Drop for CxxCommand {
    fn drop(&mut self) {
        if !self.executed {
            unsafe { crate::command_queue_cxx::ffi::cxx_command_destroy_ptr(self.user_data) };
        }
    }
}

pub struct CommandQueue {
    queue: ArrayQueue<Box<dyn FnOnce() + Send>>,
    timeout_ms: u32,
    poll_interval_us: u32,
    passthrough_all: AtomicBool,
    last_processed: AtomicU64,
}

impl CommandQueue {
    pub fn new(fixed_size: u32, timeout_ms: u32, poll_interval_us: u32) -> Self {
        Self {
            queue: ArrayQueue::new(fixed_size as usize),
            timeout_ms,
            poll_interval_us,
            passthrough_all: AtomicBool::new(false),
            last_processed: AtomicU64::new(0),
        }
    }
    fn millis_since_epoch() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64
    }
    fn last_processed(&self) -> u64 {
        self.last_processed.load(Ordering::Relaxed)
    }
    fn is_processing_inactive(&self) -> bool {
        Self::millis_since_epoch().saturating_sub(self.last_processed()) > self.timeout_ms as u64
    }
    pub fn passthrough_on(&self) {
        self.passthrough_all.store(true, Ordering::Relaxed);
    }
    pub fn clear(&self) {
        while self.queue.pop().is_some() {}
    }

    fn queue_internal(&self, fn_box: Box<dyn FnOnce() + Send>) {
        if self.is_processing_inactive() {
            fn_box();
            return;
        }
        let start = Instant::now();
        let mut fn_box = fn_box;
        loop {
            match self.queue.push(fn_box) {
                Ok(()) => return,
                Err(retrieved) => fn_box = retrieved,
            }
            if self.passthrough_all.load(Ordering::Relaxed) {
                fn_box();
                return;
            }
            if start.elapsed() > Duration::from_millis(self.timeout_ms as u64) {
                panic!("Command queue: queue timeout");
            }
            std::thread::sleep(Duration::from_micros(self.poll_interval_us as u64));
        }
    }

    fn queue_internal_and_wait(&self, fn_box: Box<dyn FnOnce() + Send>) {
        if self.is_processing_inactive() {
            fn_box();
            return;
        }
        let finished = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let finished_clone = Arc::clone(&finished);
        let failed_clone = Arc::clone(&failed);
        let wrapped_fn = Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fn_box();
            }));
            if result.is_err() {
                failed_clone.store(true, Ordering::SeqCst);
            }
            finished_clone.store(true, Ordering::SeqCst);
        });
        self.queue_internal(wrapped_fn);
        let start = Instant::now();
        loop {
            if finished.load(Ordering::Acquire) || self.passthrough_all.load(Ordering::Relaxed) {
                break;
            }
            if start.elapsed() > Duration::from_millis(self.timeout_ms as u64) {
                panic!("Command queue: exec wait timeout");
            }
            std::thread::sleep(Duration::from_micros(self.poll_interval_us as u64));
        }
        if failed.load(Ordering::Acquire) {
            panic!("Command in queue threw an exception.");
        }
    }

    pub fn queue(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue_internal(fn_box);
    }
    pub fn queue_and_wait(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue_internal_and_wait(fn_box);
    }
    pub fn exec_all(&self) {
        while let Some(cmd) = self.queue.pop() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cmd();
            }));
            if result.is_err() {
                log::error!("Failed to execute command (panic).");
            }
        }
        self.last_processed
            .store(Self::millis_since_epoch(), Ordering::Relaxed);
    }
    pub fn exec_process_thread_command(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue_and_wait(fn_box);
    }
    pub fn queue_process_thread_command(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue(fn_box);
    }
    pub fn handle_command_queue(&self) {
        self.exec_all();
    }

    pub unsafe fn cxx_queue(&self, user_data: usize) {
        self.queue_internal(Box::new(move || CxxCommand::new(user_data).execute()));
    }
    pub unsafe fn cxx_queue_and_wait(&self, user_data: usize) {
        self.queue_internal_and_wait(Box::new(move || CxxCommand::new(user_data).execute()));
    }
    pub unsafe fn cxx_exec_process_thread_command(&self, user_data: usize) {
        self.cxx_queue_and_wait(user_data);
    }
}
