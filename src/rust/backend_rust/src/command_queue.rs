//! Lock-free single-producer single-consumer queue for passing functors
//! to be executed in another thread (typically the audio processing thread).

use crossbeam_queue::ArrayQueue;
use std::sync::atomic::Ordering as MemOrder;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Command callback storage (set by C++ via set_command_callback)
static COMMAND_CALLBACK: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Set the command callback from C++.
/// This is called from the CXX bridge when C++ registers the callback.
/// # Safety
/// callback_ptr must be a valid function pointer cast to usize, or 0 to clear.
pub fn set_command_callback(callback_ptr: usize) {
    COMMAND_CALLBACK.store(callback_ptr as *mut std::ffi::c_void, MemOrder::SeqCst);
}

/// A lock-free single-producer single-consumer queue for passing functors
/// to be executed in another thread (typically the audio processing thread).
/// Provides a fallback mechanism when the processing thread appears inactive.
pub struct CommandQueue {
    queue: ArrayQueue<Box<dyn FnOnce() + Send>>,
    timeout_ms: u32,
    poll_interval_us: u32,
    passthrough_all: AtomicBool,
    last_processed: AtomicU64,
}

impl CommandQueue {
    /// Create a new CommandQueue with the specified fixed size.
    pub fn new(fixed_size: u32, timeout_ms: u32, poll_interval_us: u32) -> Self {
        Self {
            queue: ArrayQueue::new(fixed_size as usize),
            timeout_ms,
            poll_interval_us,
            passthrough_all: AtomicBool::new(false),
            last_processed: AtomicU64::new(0),
        }
    }

    /// Returns milliseconds since Unix epoch.
    fn millis_since_epoch() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64
    }

    /// Get the last processed timestamp in milliseconds since epoch.
    fn last_processed(&self) -> u64 {
        self.last_processed.load(Ordering::Relaxed)
    }

    /// Check if the processing thread appears inactive (no activity for > timeout).
    fn is_processing_inactive(&self) -> bool {
        let now = Self::millis_since_epoch();
        let last = self.last_processed();
        (now.saturating_sub(last)) > self.timeout_ms as u64
    }

    /// Set passthrough mode - all commands execute immediately on calling thread.
    pub fn passthrough_on(&self) {
        self.passthrough_all.store(true, Ordering::Relaxed);
    }

    /// Clear the queue.
    pub fn clear(&self) {
        while self.queue.pop().is_some() {
            // Drain all items
        }
    }

    /// Internal queue implementation - accepts a Box<dyn FnOnce() + Send> and handles
    /// the fallback/execute directly logic.
    fn queue_internal(&self, fn_box: Box<dyn FnOnce() + Send>) {
        if self.is_processing_inactive() {
            // Processing is apparently not active. Just execute the command directly.
            fn_box();
        } else {
            let start = Instant::now();
            let mut fn_box = fn_box;
            loop {
                match self.queue.push(fn_box) {
                    Ok(()) => {
                        // Push succeeded
                        return;
                    }
                    Err(retrieved) => {
                        // Push failed, retrieved contains our value
                        fn_box = retrieved;
                    }
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
    }

    /// Internal queue and wait implementation.
    fn queue_internal_and_wait(&self, fn_box: Box<dyn FnOnce() + Send>) {
        if self.is_processing_inactive() {
            // Processing is apparently not active. Just execute the command directly.
            fn_box();
        } else {
            let finished = Arc::new(AtomicBool::new(false));
            let failed = Arc::new(AtomicBool::new(false));

            let finished_clone = Arc::clone(&finished);
            let failed_clone = Arc::clone(&failed);

            let wrapped_fn = Box::new(move || {
                // Execute the original function and catch any panics
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fn_box();
                }));
                match result {
                    Ok(_) => {}
                    Err(_) => {
                        failed_clone.store(true, Ordering::SeqCst);
                    }
                }
                finished_clone.store(true, Ordering::SeqCst);
            });

            // Push the wrapped command (it will execute and signal completion)
            self.queue_internal(wrapped_fn);

            // Wait for completion
            let start = Instant::now();
            loop {
                if finished.load(Ordering::Acquire) {
                    break;
                }
                if self.passthrough_all.load(Ordering::Relaxed) {
                    // Already executed via passthrough
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
    }

    /// Queue a command for execution on the processing thread.
    /// If the processing thread is inactive, executes directly.
    pub fn queue(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue_internal(fn_box);
    }

    /// Queue a command and wait for completion.
    /// If the processing thread is inactive, executes directly.
    pub fn queue_and_wait(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue_internal_and_wait(fn_box);
    }

    /// Execute all queued commands (called from processing thread).
    /// Exceptions are caught and logged.
    pub fn exec_all(&self) {
        while let Some(cmd) = self.queue.pop() {
            // Catch panics from command execution
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

    // Convenience methods
    /// Execute a command on the processing thread, waiting for completion.
    pub fn exec_process_thread_command(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue_and_wait(fn_box);
    }

    /// Queue a command for execution on the processing thread (fire-and-forget).
    pub fn queue_process_thread_command(&self, fn_box: Box<dyn FnOnce() + Send>) {
        self.queue(fn_box);
    }

    /// Process/handle the command queue (called from processing thread).
    pub fn handle_command_queue(&self) {
        self.exec_all();
    }

    // =================================================================
    // CXX bridge methods (user_data pointer approach)
    // =================================================================

    /// CXX bridge: Queue a command from user_data pointer.
    /// SAFETY: user_data must be valid until command is executed or queue is cleared.
    pub unsafe fn cxx_queue(&self, user_data: usize) {
        if user_data == 0 {
            return;
        }
        let wrapper = Box::new(move || {
            // Get callback from static storage (initialized by C++)
            let callback_ptr = COMMAND_CALLBACK.load(MemOrder::Acquire);
            if !callback_ptr.is_null() {
                let callback: extern "C" fn(usize) = std::mem::transmute(callback_ptr as usize);
                callback(user_data);
            }
        });
        self.queue_internal(wrapper);
    }

    /// CXX bridge: Queue and wait from user_data pointer.
    /// SAFETY: user_data must be valid until command is executed.
    pub unsafe fn cxx_queue_and_wait(&self, user_data: usize) {
        if user_data == 0 {
            return;
        }
        let wrapper = Box::new(move || {
            let callback_ptr = COMMAND_CALLBACK.load(MemOrder::Acquire);
            if !callback_ptr.is_null() {
                let callback: extern "C" fn(usize) = std::mem::transmute(callback_ptr as usize);
                callback(user_data);
            }
        });
        self.queue_internal_and_wait(wrapper);
    }

    /// CXX bridge: Execute a command on the processing thread, waiting for completion.
    /// SAFETY: user_data must be valid until command is executed.
    pub unsafe fn cxx_exec_process_thread_command(&self, user_data: usize) {
        self.cxx_queue_and_wait(user_data);
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    // Helper to create a boxed closure that captures an Arc counter
    fn make_counter_op(counter: &Arc<AtomicU32>, value: u32) -> Box<dyn FnOnce() + Send> {
        let counter = Arc::clone(counter);
        Box::new(move || {
            counter.fetch_add(value, Ordering::SeqCst);
        })
    }

    #[test]
    fn test_basic_queue_and_exec() {
        let queue = CommandQueue::new(1024, 1000, 100);
        let counter = Arc::new(AtomicU32::new(0));

        // Queue some commands
        queue.queue(make_counter_op(&counter, 1));
        queue.queue(make_counter_op(&counter, 10));

        // Initially inactive, so commands executed directly
        assert_eq!(counter.load(Ordering::SeqCst), 11);

        // Now queue more and exec_all
        queue.queue(make_counter_op(&counter, 100));

        // Simulate processing thread by updating last_processed
        queue
            .last_processed
            .store(CommandQueue::millis_since_epoch(), Ordering::Relaxed);

        queue.queue(make_counter_op(&counter, 1000));

        queue.exec_all();
        assert_eq!(counter.load(Ordering::SeqCst), 1111);
    }

    #[test]
    fn test_passthrough_mode() {
        let queue = CommandQueue::new(1024, 1000, 100);
        let counter = Arc::new(AtomicU32::new(0));

        // Enable passthrough
        queue.passthrough_on();

        // With passthrough, commands execute immediately
        queue.queue(make_counter_op(&counter, 1));
        queue.queue(make_counter_op(&counter, 2));

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_queue_and_wait() {
        let queue = CommandQueue::new(1024, 1000, 100);
        let counter = Arc::new(AtomicU32::new(0));

        // Queue and wait
        queue.queue_and_wait(make_counter_op(&counter, 1));
        queue.queue_and_wait(make_counter_op(&counter, 2));

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_clear() {
        let queue = CommandQueue::new(1024, 1000, 100);
        let counter = Arc::new(AtomicU32::new(0));

        // Simulate processing thread active
        queue
            .last_processed
            .store(CommandQueue::millis_since_epoch(), Ordering::Relaxed);

        // Queue some commands
        queue.queue(make_counter_op(&counter, 1));
        queue.queue(make_counter_op(&counter, 2));

        // Clear the queue
        queue.clear();

        // exec_all should not run any commands
        queue.exec_all();
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_inactivity_detection() {
        let queue = CommandQueue::new(1024, 1000, 100);
        let counter = Arc::new(AtomicU32::new(0));

        // When inactive, commands execute directly
        assert!(queue.is_processing_inactive());

        queue.queue(make_counter_op(&counter, 1));
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Now simulate active processing
        queue
            .last_processed
            .store(CommandQueue::millis_since_epoch(), Ordering::Relaxed);

        assert!(!queue.is_processing_inactive());

        // Commands should go to queue and execute via exec_all
        queue.queue(make_counter_op(&counter, 10));

        queue.exec_all();
        assert_eq!(counter.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn test_convenience_methods() {
        let queue = CommandQueue::new(1024, 1000, 100);
        let counter = Arc::new(AtomicU32::new(0));

        // exec_process_thread_command (queue_and_wait)
        queue.exec_process_thread_command(make_counter_op(&counter, 1));
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // queue_process_thread_command (fire-and-forget when inactive)
        queue.queue_process_thread_command(make_counter_op(&counter, 10));
        assert_eq!(counter.load(Ordering::SeqCst), 11);

        // handle_command_queue (same as exec_all)
        queue.handle_command_queue();
        assert_eq!(counter.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn test_panic_propagation_in_queue_and_wait() {
        let queue = CommandQueue::new(1024, 1000, 100);

        // Set up processing as active
        queue
            .last_processed
            .store(CommandQueue::millis_since_epoch(), Ordering::Relaxed);

        // Queue a panicking command wrapped in catch_unwind
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            queue.queue_and_wait(Box::new(|| {
                panic!("test panic");
            }));
        }));

        // Should catch the panic
        assert!(result.is_err());
    }
}
