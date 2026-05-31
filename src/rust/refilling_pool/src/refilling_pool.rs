//! A lock-free, auto-refilling object pool for real-time applications.
//!
//! This module provides `RefillingPool<T>`, a concurrent data structure designed
//! for scenarios like real-time audio processing where blocking for a new object
//! is unacceptable. It maintains a queue of pre-allocated objects, and when the
//! number of available objects drops below a threshold, a background thread
//! automatically replenishes the pool.

use crossbeam_queue::ArrayQueue;
use std::fmt;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};

/// An error that can occur during the creation of a `RefillingPool`.
#[derive(Debug, PartialEq, Eq)]
pub struct PoolCreationError {
    description: String,
}

impl fmt::Display for PoolCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pool creation error: {}", self.description)
    }
}

impl std::error::Error for PoolCreationError {}

/// A concurrent, lock-free pool of pre-allocated objects that automatically
/// refills itself in the background.
///
/// The `RefillingPool` is ideal for real-time systems where object allocation
/// on a critical thread must be avoided. It provides a `get` method that
/// pulls a ready-to-use object from a queue. If the queue is empty, it
/// creates a new object on-the-fly as a fallback, preventing the consumer
/// from ever blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefillPolicy {
    Auto,
    Manual,
}

pub struct RefillingPool<T: Send + Debug + 'static> {
    /// The lock-free queue holding the pre-allocated objects.
    queue: Arc<ArrayQueue<Box<T>>>,
    /// The user-provided closure to create new objects.
    factory: Arc<dyn Fn() -> Box<T> + Send + Sync>,
    /// An atomic flag to indicate a refill is needed.
    refill_needed: Arc<AtomicBool>,
    /// The number of items at which a refill is triggered.
    low_water_mark: usize,
    /// A signal to gracefully shut down the refilling thread.
    shutdown_signal: Arc<AtomicBool>,
    /// The handle to join the background refilling thread.
    refill_thread: Option<JoinHandle<()>>,
    /// The thread handle to wait-free unpark the background thread.
    refill_thread_handle: Thread,
    /// An atomic counter for all newly created buffers.
    buffers_created_counter: Arc<AtomicUsize>,
}

impl<T: Send + Debug + 'static> RefillingPool<T> {
    /// Creates a new `RefillingPool` with automatic background refilling.
    pub fn new<F>(
        capacity: usize,
        low_water_mark: usize,
        factory: F,
    ) -> Result<Self, PoolCreationError>
    where
        F: Fn() -> Box<T> + Send + Sync + 'static,
    {
        Self::new_with_policy(capacity, low_water_mark, RefillPolicy::Auto, factory)
    }

    /// Creates a pool without spawning the background refill thread.
    /// Intended for deterministic tests.
    pub fn new_without_refill_thread<F>(
        capacity: usize,
        low_water_mark: usize,
        factory: F,
    ) -> Result<Self, PoolCreationError>
    where
        F: Fn() -> Box<T> + Send + Sync + 'static,
    {
        Self::new_with_policy(capacity, low_water_mark, RefillPolicy::Manual, factory)
    }

    /// Creates a new `RefillingPool` with configurable refill behavior.
    ///
    /// The pool is immediately pre-filled to its `capacity`.
    /// If `refill_policy` is [`RefillPolicy::Auto`], a background thread is
    /// spawned to keep it refilled. If it is [`RefillPolicy::Manual`], no
    /// background refiller is started.
    pub fn new_with_policy<F>(
        capacity: usize,
        low_water_mark: usize,
        refill_policy: RefillPolicy,
        factory: F,
    ) -> Result<Self, PoolCreationError>
    where
        F: Fn() -> Box<T> + Send + Sync + 'static,
    {
        if low_water_mark >= capacity {
            return Err(PoolCreationError {
                description: "low_water_mark must be less than capacity".to_string(),
            });
        }

        let queue = Arc::new(ArrayQueue::new(capacity));

        // Wrap the user's factory to increment an atomic counter on each creation.
        // This covers pre-filling, refilling, and on-demand allocations.
        let buffers_created_counter = Arc::new(AtomicUsize::new(0));
        let user_factory = Arc::new(factory);
        let factory: Arc<dyn Fn() -> Box<T> + Send + Sync> = {
            let counter_clone = Arc::clone(&buffers_created_counter);
            Arc::new(move || {
                counter_clone.fetch_add(1, Ordering::Relaxed);
                user_factory()
            })
        };

        // Pre-fill the queue to capacity.
        for _ in 0..capacity {
            if queue.push(factory()).is_err() {
                return Err(PoolCreationError {
                    description: "Failed to pre-fill the queue".to_string(),
                });
            }
        }

        let shutdown_signal = Arc::new(AtomicBool::new(false));
        let refill_needed = Arc::new(AtomicBool::new(false));

        let (refill_thread, refill_thread_handle) = if refill_policy == RefillPolicy::Auto {
            // Clone Arcs for the background thread.
            let queue_clone = Arc::clone(&queue);
            let factory_clone = Arc::clone(&factory);
            let shutdown_signal_clone = Arc::clone(&shutdown_signal);
            let refill_needed_clone = Arc::clone(&refill_needed);

            let refill_thread = thread::spawn(move || {
                while !shutdown_signal_clone.load(Ordering::Relaxed) {
                    if refill_needed_clone.swap(false, Ordering::Acquire) {
                        loop {
                            let items_needed = capacity - queue_clone.len();
                            if items_needed == 0 {
                                break;
                            }

                            for _ in 0..items_needed {
                                let new_item = factory_clone();
                                if queue_clone.push(new_item).is_err() {
                                    break;
                                }
                            }
                        }
                    } else {
                        thread::park();
                    }
                }
            });

            let refill_thread_handle = refill_thread.thread().clone();
            (Some(refill_thread), refill_thread_handle)
        } else {
            (None, thread::current())
        };

        Ok(Self {
            queue,
            factory,
            refill_needed,
            low_water_mark,
            shutdown_signal,
            refill_thread,
            refill_thread_handle,
            buffers_created_counter,
        })
    }

    /// Retrieves an object from the pool.
    ///
    /// This method tries to pop an object from the internal lock-free queue.
    /// - If successful, it returns the pre-allocated object immediately.
    /// - If the queue is empty, it creates a new object on the current thread
    ///   as a fallback and returns it. This avoids blocking but may introduce
    ///   latency from the allocation.
    ///
    /// After an item is retrieved, if the number of available items has fallen
    /// below the low-water mark, the background refilling thread is notified.
    pub fn get(&self) -> Box<T> {
        let item = self.queue.pop().unwrap_or_else(|| {
            // Slow path: queue was empty.
            // Create a new item on this thread to avoid blocking.
            (self.factory)()
        });

        // In either case, check if we need to signal the refiller.
        if self.refill_thread.is_some() && self.queue.len() <= self.low_water_mark {
            self.notify_refiller();
        }
        item
    }

    /// Puts an object back into the pool for reuse.
    /// If the pool is full, the object is dropped.
    pub fn release(&self, item: Box<T>) {
        // We don't care about the result. If the queue is full,
        // the item is dropped, and memory is reclaimed.
        let _ = self.queue.push(item);
    }

    /// Notifies the background thread that a refill may be needed. This is wait-free.
    fn notify_refiller(&self) {
        // Set the flag first to ensure the refiller sees the need for a refill.
        self.refill_needed.store(true, Ordering::Release);

        // Then, wake up the thread. `unpark` provides a token and requires no locks.
        self.refill_thread_handle.unpark();
    }

    /// Returns the number of pre-allocated buffers currently available in the pool.
    ///
    /// This method is lock-free and can be called from any thread without
    /// significant performance impact.
    pub fn n_buffers_available(&self) -> usize {
        self.queue.len()
    }

    /// Returns the number of new buffers created since the last call to this method.
    ///
    /// This counter includes buffers created during initial pre-filling, background
    /// refilling, and on-demand fallback allocations in `get()`. It is reset to
    /// zero after being read.
    ///
    /// This method is lock-free and can be called from any thread.
    pub fn n_buffers_created_since_last_checked(&self) -> usize {
        self.buffers_created_counter.swap(0, Ordering::AcqRel)
    }
}

impl<T: Send + Debug + 'static> Drop for RefillingPool<T> {
    /// Gracefully shuts down the refilling thread.
    fn drop(&mut self) {
        if let Some(handle) = self.refill_thread.take() {
            // 1. Signal the thread to shut down.
            self.shutdown_signal.store(true, Ordering::Relaxed);

            // 2. Unpark the thread to ensure it wakes up and sees the shutdown signal.
            self.refill_thread_handle.unpark();

            // 3. Join the thread to ensure it has terminated.
            if let Err(e) = handle.join() {
                println!("Refilling thread panicked during shutdown: {:?}", e);
            }
        }
    }
}

//---

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;
    use std::time::Duration;

    // A simple object for testing purposes.
    struct TestObject(usize);

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    impl Debug for TestObject {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestObject({})", self.0)
        }
    }

    /// Tests that an invalid configuration returns an error instead of panicking.
    #[test]
    fn test_new_with_invalid_config_returns_err() -> Result<(), anyhow::Error> {
        let result = RefillingPool::new(10, 10, || Box::new(TestObject(0)));
        assert!(result.is_err());
        assert_eq!(
            result.err().ok_or(anyhow!("Expected error"))?,
            PoolCreationError {
                description: "low_water_mark must be less than capacity".to_string()
            }
        );

        let result_greater = RefillingPool::new(10, 11, || Box::new(TestObject(0)));
        assert!(result_greater.is_err());
        Ok(())
    }

    /// Tests that the pool is correctly pre-filled to capacity upon creation.
    #[test]
    fn test_initial_state_and_prefill() -> Result<(), anyhow::Error> {
        let creation_counter = Arc::new(AtomicUsize::new(0));
        let factory = {
            let counter = Arc::clone(&creation_counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Box::new(TestObject(0))
            }
        };

        let pool = RefillingPool::new(10, 5, factory).map_err(|e| anyhow!(e.to_string()))?;

        assert_eq!(
            creation_counter.load(Ordering::SeqCst),
            10,
            "Factory should be called 10 times for pre-filling"
        );
        assert_eq!(pool.queue.len(), 10, "Queue should be full after creation");
        Ok(())
    }

    /// Tests the fallback mechanism when the pool is empty.
    #[test]
    fn test_get_and_empty_fallback() -> Result<(), anyhow::Error> {
        let creation_counter = Arc::new(AtomicUsize::new(0));
        let factory = {
            let counter = Arc::clone(&creation_counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Box::new(TestObject(0))
            }
        };

        let pool = RefillingPool::new_with_policy(2, 0, RefillPolicy::Manual, factory)
            .map_err(|e| anyhow!(e.to_string()))?;
        assert_eq!(creation_counter.load(Ordering::SeqCst), 2);

        // Take the two pre-allocated objects
        let _ = pool.get();
        let _ = pool.get();
        assert_eq!(pool.queue.len(), 0);
        assert_eq!(
            creation_counter.load(Ordering::SeqCst),
            2,
            "Factory should not be called again yet"
        );

        // The next get() must trigger the on-demand allocation fallback
        let _ = pool.get();
        assert_eq!(
            pool.queue.len(),
            0,
            "Fallback allocation should not go into the queue"
        );
        assert_eq!(
            creation_counter.load(Ordering::SeqCst),
            3,
            "Factory should be called for the fallback allocation"
        );
        Ok(())
    }

    /// Tests the refilling logic deterministically using a channel for synchronization.
    #[test]
    fn test_deterministic_refilling_logic() -> Result<(), anyhow::Error> {
        let (tx, rx) = mpsc::sync_channel(0); // Rendezvous channel
        let (pool_tx, pool_rx) = mpsc::channel();

        // Spawning the pool creation allows the main thread to receive on `rx`
        // concurrently, preventing deadlock.
        thread::spawn(move || {
            let factory = move || {
                let _ = tx.send(());
                Box::new(TestObject(0))
            };
            if let Ok(pool) = RefillingPool::new(10, 5, factory) {
                let _ = pool_tx.send(pool);
            }
        });

        // Drain the 10 signals from the initial pre-fill. This unblocks the factory.
        for i in 0..10 {
            rx.recv_timeout(TEST_TIMEOUT)
                .map_err(|_| anyhow!("Timeout waiting for initial fill signal {}/10", i + 1))?;
        }

        // Get the fully constructed pool from the creation thread.
        let pool = pool_rx
            .recv_timeout(TEST_TIMEOUT)
            .expect("Failed to receive pool from creation thread");

        // Take 5 items to drop queue size to 5, which is <= the low-water mark of 5.
        // This will trigger the refill signal.
        for _ in 0..5 {
            let _ = pool.get();
        }
        assert_eq!(pool.queue.len(), 5);

        // The pool now needs to refill 5 items to get back to capacity 10.
        // We wait for exactly 5 signals from the refiller's factory calls.
        for i in 0..5 {
            rx.recv_timeout(TEST_TIMEOUT)
                .map_err(|_| anyhow!("Timeout waiting for refiller signal {}/5", i + 1))?;
        }

        // Don't assert immediately. Poll until the queue is full, because the
        // signals arrive before the `push` operations are complete.
        let deadline = std::time::Instant::now() + TEST_TIMEOUT;
        while pool.queue.len() < 10 {
            if std::time::Instant::now() > deadline {
                return Err(anyhow!(
                    "Timeout waiting for pool to be fully refilled. Final size: {}",
                    pool.queue.len()
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(pool.queue.len(), 10, "Pool should be refilled to capacity");
        Ok(())
    }

    /// Tests that multiple concurrent signals don't confuse the refiller.
    #[test]
    fn test_concurrent_signal() -> Result<(), anyhow::Error> {
        let (tx, rx) = mpsc::sync_channel(0);
        let (pool_tx, pool_rx) = mpsc::channel();

        // Spawn the pool creation to avoid deadlock on the rendezvous channel.
        thread::spawn(move || {
            let factory = move || {
                let _ = tx.send(());
                Box::new(TestObject(0))
            };
            if let Ok(pool) = RefillingPool::new(10, 9, factory) {
                let _ = pool_tx.send(Arc::new(pool));
            }
        });

        // Drain initial signals
        for i in 0..10 {
            rx.recv_timeout(TEST_TIMEOUT)
                .map_err(|_| anyhow!("Timeout waiting for initial fill signal {}/10", i + 1))?;
        }

        let pool = pool_rx
            .recv_timeout(TEST_TIMEOUT)
            .map_err(|_| anyhow!("Failed to receive pool from creation thread"))?;

        // Two threads get an item, pushing the count from 10 to 8,
        // crossing the low-water mark of 9 twice in quick succession.
        let pool1: Arc<RefillingPool<TestObject>> = Arc::clone(&pool);
        let t1 = thread::spawn(move || pool1.get());
        let pool2: Arc<RefillingPool<TestObject>> = Arc::clone(&pool);
        let t2 = thread::spawn(move || pool2.get());

        t1.join().map_err(|e| anyhow!("t1 panicked: {:?}", e))?;
        t2.join().map_err(|e| anyhow!("t2 panicked: {:?}", e))?;

        assert_eq!(pool.queue.len(), 8);

        // The pool should refill exactly 2 items.
        rx.recv_timeout(TEST_TIMEOUT)
            .map_err(|_| anyhow!("Did not receive first refill signal"))?;
        rx.recv_timeout(TEST_TIMEOUT)
            .map_err(|_| anyhow!("Did not receive second refill signal"))?;

        // Don't assert immediately. Poll until the queue is full, because the
        // signals arrive before the `push` operations are complete.
        let deadline = std::time::Instant::now() + TEST_TIMEOUT;
        while pool.queue.len() < 10 {
            if std::time::Instant::now() > deadline {
                return Err(anyhow!(
                    "Timeout waiting for pool to be fully refilled. Final size: {}",
                    pool.queue.len()
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }

        // Ensure no more signals are sent (i.e., no over-filling).
        assert!(rx.try_recv().is_err(), "Refiller produced too many objects");
        assert_eq!(
            pool.queue.len(),
            10,
            "Pool should be refilled exactly to capacity"
        );
        Ok(())
    }

    /// Tests that the pool shuts down its background thread cleanly.
    #[test]
    fn test_clean_shutdown() -> Result<(), anyhow::Error> {
        let (tx, rx) = mpsc::channel();

        // The test logic is spawned in a separate thread.
        // If it hangs (e.g., during `drop`), the main thread will time out.
        thread::spawn(move || {
            {
                // This scope ensures the pool is dropped before we send the signal.
                if let Ok(pool) = RefillingPool::new(5, 2, || Box::new(TestObject(0))) {
                    let pool_arc = Arc::new(pool);
                    let pool_clone = Arc::clone(&pool_arc);

                    // Use the pool in another thread to ensure it's active.
                    let worker = thread::spawn(move || {
                        for _ in 0..10 {
                            let _ = pool_clone.get();
                            thread::sleep(Duration::from_millis(1));
                        }
                    });

                    // Wait for the spawned thread to finish its work.
                    let _ = worker.join();
                }
            } // `pool_arc` is dropped here, which is the operation we want to test for hangs.

            // Signal that the test logic has completed without hanging.
            let _ = tx.send(());
        });

        // Wait for the completion signal from the test thread.
        if let Err(e) = rx.recv_timeout(TEST_TIMEOUT) {
            return Err(anyhow!(
                "Test timed out: clean shutdown (drop) likely hung. Error: {}",
                e
            ));
        }
        Ok(())
    }

    /// Tests that releasing an item puts it back in the pool.
    #[test]
    fn test_release() -> Result<(), anyhow::Error> {
        let creation_counter = Arc::new(AtomicUsize::new(0));
        let factory = {
            let counter = Arc::clone(&creation_counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Box::new(TestObject(0))
            }
        };

        let pool = RefillingPool::new_with_policy(2, 1, RefillPolicy::Manual, factory)
            .map_err(|e| anyhow!(e.to_string()))?;
        let item = pool.get();
        assert_eq!(pool.queue.len(), 1);

        pool.release(item);

        // Verify that releasing the item increases the queue length back to 2.
        assert_eq!(pool.queue.len(), 2);
        Ok(())
    }

    /// Tests that exposed counters accurately track queue states and new buffer allocations.
    #[test]
    fn test_counters_and_tracking() -> Result<(), anyhow::Error> {
        let pool =
            RefillingPool::new_with_policy(2, 0, RefillPolicy::Manual, || Box::new(TestObject(0)))?;

        assert_eq!(pool.n_buffers_available(), 2);
        assert_eq!(pool.n_buffers_created_since_last_checked(), 2);
        assert_eq!(pool.n_buffers_created_since_last_checked(), 0); // resets after check

        let _ = pool.get();
        let _ = pool.get();

        assert_eq!(pool.n_buffers_available(), 0);
        assert_eq!(pool.n_buffers_created_since_last_checked(), 0); // No refill triggered yet

        let _ = pool.get(); // Fallback creation
        assert_eq!(pool.n_buffers_created_since_last_checked(), 1);

        Ok(())
    }

    /// Stress test ensuring no deadlocks or lost wakeups occur under heavy contention.
    #[test]
    fn test_heavy_concurrency_stress() {
        let pool = Arc::new(RefillingPool::new(50, 10, || Box::new(TestObject(0))).unwrap());

        let mut handles = vec![];
        for _ in 0..8 {
            let p = Arc::clone(&pool);
            handles.push(thread::spawn(move || {
                for i in 0..2000 {
                    let item = p.get();

                    // Simulate processing
                    if i % 10 == 0 {
                        thread::yield_now();
                    }

                    // Constantly returning items causes high contention
                    // and frequent queue full scenarios against the refiller thread.
                    if i % 3 != 0 {
                        p.release(item);
                    }
                }
            }));
        }

        // If a wakeup is lost, this will hang indefinitely (caught by test runner timeout).
        for h in handles {
            h.join().expect("Thread panicked");
        }

        // Ensure the pool didn't enter a corrupted state.
        let _ = pool.get();
    }
}
