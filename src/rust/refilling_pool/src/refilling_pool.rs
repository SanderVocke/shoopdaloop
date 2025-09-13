//! A lock-free, auto-refilling object pool for real-time applications.
//!
//! This module provides `RefillingPool<T>`, a concurrent data structure designed
//! for scenarios like real-time audio processing where blocking for a new object
//! is unacceptable. It maintains a queue of pre-allocated objects, and when the
//! number of available objects drops below a threshold, a background thread
//! automatically replenishes the pool.

use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::fmt;
use std::fmt::Debug;

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
pub struct RefillingPool<T: Send + Debug + 'static> {
    /// The lock-free queue holding the pre-allocated objects.
    queue: Arc<ArrayQueue<Box<T>>>,
    /// The user-provided closure to create new objects.
    factory: Arc<dyn Fn() -> Box<T> + Send + Sync>,
    /// A signal to wake up the refilling thread.
    refill_signal: Arc<(Mutex<()>, Condvar)>,
    /// An atomic flag to indicate a refill is needed, avoiding locks on the consumer path.
    refill_needed: Arc<AtomicBool>,
    /// The number of items at which a refill is triggered.
    low_water_mark: usize,
    /// A signal to gracefully shut down the refilling thread.
    shutdown_signal: Arc<AtomicBool>,
    /// The handle to the background refilling thread.
    refill_thread: Option<JoinHandle<()>>,
    /// An atomic counter for all newly created buffers.
    buffers_created_counter: Arc<AtomicUsize>,
}

impl<T: Send + Debug + 'static> RefillingPool<T> {
    /// Creates a new `RefillingPool`.
    ///
    /// The pool is immediately pre-filled to its `capacity`. A background thread is
    /// spawned to handle refilling.
    ///
    /// # Arguments
    ///
    /// * `capacity`: The target number of objects the pool should hold. The queue will
    ///   be sized to this value.
    /// * `low_water_mark`: When the number of objects in the pool drops to this
    ///   level, the background thread will be signaled to refill it.
    /// * `factory`: A closure that produces new `Box<T>` instances.
    ///
    /// # Errors
    ///
    /// Returns a `PoolCreationError` if `low_water_mark` is greater than or
    /// equal to `capacity`.
    pub fn new<F>(
        capacity: usize,
        low_water_mark: usize,
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

        let refill_signal = Arc::new((Mutex::new(()), Condvar::new()));
        let shutdown_signal = Arc::new(AtomicBool::new(false));
        let refill_needed = Arc::new(AtomicBool::new(false));

        // Clone Arcs for the background thread.
        let queue_clone = Arc::clone(&queue);
        let factory_clone = Arc::clone(&factory);
        let refill_signal_clone = Arc::clone(&refill_signal);
        let shutdown_signal_clone = Arc::clone(&shutdown_signal);
        let refill_needed_clone = Arc::clone(&refill_needed);

        let refill_thread = thread::spawn(move || {
            let (lock, cvar) = &*refill_signal_clone;
            let mut guard = lock.lock().expect("Refiller thread failed to acquire lock");

            while !shutdown_signal_clone.load(Ordering::Relaxed) {
                // The classic spurious wakeup loop.
                // Wait as long as no refill is needed and we are not shutting down.
                while !refill_needed_clone.load(Ordering::Acquire) && !shutdown_signal_clone.load(Ordering::Relaxed) {
                    guard = cvar.wait(guard).expect("Refiller thread failed to wait on condvar");
                }

                if shutdown_signal_clone.load(Ordering::Relaxed) {
                    break;
                }

                // This inner loop ensures we refill completely, even if consumers are active.
                // It will only exit when the queue is at full capacity.
                loop {
                    let items_needed = capacity - queue_clone.len();
                    if items_needed == 0 {
                        break; // We are full, exit the refill loop.
                    }

                    for _ in 0..items_needed {
                        queue_clone.push(factory_clone()).expect("Refiller thread failed: queue should have space");
                    }
                    // After pushing items, we loop again to check if consumers have taken more
                    // items during the refill.
                }
                
                // Only reset the flag once we are certain the queue is full.
                refill_needed_clone.store(false, Ordering::Relaxed);
            }
        });

        Ok(Self {
            queue,
            factory,
            refill_signal,
            refill_needed,
            low_water_mark,
            shutdown_signal,
            refill_thread: Some(refill_thread),
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
        if self.queue.len() <= self.low_water_mark {
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

    /// Notifies the background thread that a refill may be needed. This is lock-free.
    fn notify_refiller(&self) {
        // Set the flag first to ensure the refiller sees the need for a refill
        // even if the notification is missed.
        self.refill_needed.store(true, Ordering::Release);
        
        // Then, wake up the thread in case it's sleeping.
        let (_, cvar) = &*self.refill_signal;
        cvar.notify_one();
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

            // 2. Wake up the thread from its `condvar.wait()` call.
            self.notify_refiller();

            // 3. Join the thread to ensure it has terminated.
            handle.join().expect("Refilling thread panicked during shutdown");
        }
    }
}

//---

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_new_with_invalid_config_returns_err() {
        let result = RefillingPool::new(10, 10, || Box::new(TestObject(0)));
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            PoolCreationError {
                description: "low_water_mark must be less than capacity".to_string()
            }
        );

        let result_greater = RefillingPool::new(10, 11, || Box::new(TestObject(0)));
        assert!(result_greater.is_err());
    }

    /// Tests that the pool is correctly pre-filled to capacity upon creation.
    #[test]
    fn test_initial_state_and_prefill() {
        let creation_counter = Arc::new(AtomicUsize::new(0));
        let factory = {
            let counter = Arc::clone(&creation_counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Box::new(TestObject(0))
            }
        };

        let pool = RefillingPool::new(10, 5, factory).unwrap();

        assert_eq!(creation_counter.load(Ordering::SeqCst), 10, "Factory should be called 10 times for pre-filling");
        assert_eq!(pool.queue.len(), 10, "Queue should be full after creation");
    }

    /// Tests the fallback mechanism when the pool is empty.
    #[test]
    fn test_get_and_empty_fallback() {
        let creation_counter = Arc::new(AtomicUsize::new(0));
        let factory = {
            let counter = Arc::clone(&creation_counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Box::new(TestObject(0))
            }
        };

        let pool = RefillingPool::new(2, 1, factory).unwrap();
        assert_eq!(creation_counter.load(Ordering::SeqCst), 2);

        // Take the two pre-allocated objects
        let _ = pool.get();
        let _ = pool.get();
        assert_eq!(pool.queue.len(), 0);
        assert_eq!(creation_counter.load(Ordering::SeqCst), 2, "Factory should not be called again yet");

        // The next get() must trigger the on-demand allocation fallback
        let _ = pool.get();
        assert_eq!(pool.queue.len(), 0, "Fallback allocation should not go into the queue");
        assert_eq!(creation_counter.load(Ordering::SeqCst), 3, "Factory should be called for the fallback allocation");
    }

    /// Tests the refilling logic deterministically using a channel for synchronization.
    #[test]
    fn test_deterministic_refilling_logic() {
        let (tx, rx) = mpsc::sync_channel(0); // Rendezvous channel
        let (pool_tx, pool_rx) = mpsc::channel();

        // Spawning the pool creation allows the main thread to receive on `rx`
        // concurrently, preventing deadlock.
        thread::spawn(move || {
            let factory = move || {
                tx.send(()).unwrap();
                Box::new(TestObject(0))
            };
            let pool = RefillingPool::new(10, 5, factory).unwrap();
            pool_tx.send(pool).unwrap();
        });

        // Drain the 10 signals from the initial pre-fill. This unblocks the factory.
        for i in 0..10 {
            rx.recv_timeout(TEST_TIMEOUT)
                .unwrap_or_else(|_| panic!("Timeout waiting for initial fill signal {}/10", i + 1));
        }

        // Get the fully constructed pool from the creation thread.
        let pool = pool_rx.recv_timeout(TEST_TIMEOUT).expect("Failed to receive pool from creation thread");

        // Take 5 items to drop queue size to 5, which is <= the low-water mark of 5.
        // This will trigger the refill signal.
        for _ in 0..5 {
            let _ = pool.get();
        }
        assert_eq!(pool.queue.len(), 5);

        // The pool now needs to refill 5 items to get back to capacity 10.
        // We wait for exactly 5 signals from the refiller's factory calls.
        for i in 0..5 {
            rx.recv_timeout(TEST_TIMEOUT).unwrap_or_else(|_| {
                panic!("Timeout waiting for refiller signal {}/5", i + 1);
            });
        }

        assert_eq!(pool.queue.len(), 10, "Pool should be refilled to capacity");
    }
    
    /// Tests that multiple concurrent signals don't confuse the refiller.
    #[test]
    fn test_concurrent_signal() {
        let (tx, rx) = mpsc::sync_channel(0);
        let (pool_tx, pool_rx) = mpsc::channel();
        
        // Spawn the pool creation to avoid deadlock on the rendezvous channel.
        thread::spawn(move || {
            let factory = move || {
                tx.send(()).unwrap();
                Box::new(TestObject(0))
            };
            let pool = Arc::new(RefillingPool::new(10, 9, factory).unwrap());
            pool_tx.send(pool).unwrap();
        });

        // Drain initial signals
        for i in 0..10 {
            rx.recv_timeout(TEST_TIMEOUT)
                .unwrap_or_else(|_| panic!("Timeout waiting for initial fill signal {}/10", i + 1));
        }

        let pool = pool_rx.recv_timeout(TEST_TIMEOUT).expect("Failed to receive pool from creation thread");
        
        // Two threads get an item, pushing the count from 10 to 8,
        // crossing the low-water mark of 9 twice in quick succession.
        let pool1 = Arc::clone(&pool);
        let t1 = thread::spawn(move || pool1.get());
        let pool2 = Arc::clone(&pool);
        let t2 = thread::spawn(move || pool2.get());
        
        t1.join().unwrap();
        t2.join().unwrap();
        
        assert_eq!(pool.queue.len(), 8);

        // The pool should refill exactly 2 items.
        rx.recv_timeout(TEST_TIMEOUT)
            .expect("Did not receive first refill signal");
        rx.recv_timeout(TEST_TIMEOUT)
            .expect("Did not receive second refill signal");

        // Ensure no more signals are sent (i.e., no over-filling).
        assert!(rx.try_recv().is_err(), "Refiller produced too many objects");
        assert_eq!(pool.queue.len(), 10, "Pool should be refilled exactly to capacity");
    }

    /// Tests that the pool shuts down its background thread cleanly.
    #[test]
    fn test_clean_shutdown() {
        let (tx, rx) = mpsc::channel();

        // The test logic is spawned in a separate thread.
        // If it hangs (e.g., during `drop`), the main thread will time out.
        thread::spawn(move || {
            {
                // This scope ensures the pool is dropped before we send the signal.
                let pool_arc =
                    Arc::new(RefillingPool::new(5, 2, || Box::new(TestObject(0))).unwrap());
                let pool_clone = Arc::clone(&pool_arc);

                // Use the pool in another thread to ensure it's active.
                let worker = thread::spawn(move || {
                    for _ in 0..10 {
                        let _ = pool_clone.get();
                        thread::sleep(Duration::from_millis(1));
                    }
                });

                // Wait for the spawned thread to finish its work.
                worker.join().expect("Worker thread panicked");
            } // `pool_arc` is dropped here, which is the operation we want to test for hangs.

            // Signal that the test logic has completed without hanging.
            tx.send(()).expect("Failed to send completion signal");
        });

        // Wait for the completion signal from the test thread.
        if let Err(e) = rx.recv_timeout(TEST_TIMEOUT) {
            panic!("Test timed out: clean shutdown (drop) likely hung. Error: {}", e);
        }
    }

    /// Tests that releasing an item puts it back in the pool.
    #[test]
    fn test_release() {
        let creation_counter = Arc::new(AtomicUsize::new(0));
        let factory = {
            let counter = Arc::clone(&creation_counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Box::new(TestObject(0))
            }
        };

        let pool = RefillingPool::new(2, 1, factory).unwrap();
        let item = pool.get();
        assert_eq!(pool.queue.len(), 1);

        pool.release(item);

        // Verify that releasing the item increases the queue length back to 2.
        assert_eq!(pool.queue.len(), 2);
    }

    /// Tests the buffer counter methods.
    #[test]
    fn test_buffer_counters() {
        let pool = RefillingPool::new(10, 5, || Box::new(TestObject(0))).unwrap();

        // 1. Check initial state
        assert_eq!(pool.n_buffers_available(), 10);
        assert_eq!(pool.n_buffers_created_since_last_checked(), 10, "Should count 10 buffers from pre-fill");
        assert_eq!(pool.n_buffers_created_since_last_checked(), 0, "Counter should be reset");

        // 2. Take some items, but not enough to trigger refill
        let _ = pool.get();
        let _ = pool.get();
        assert_eq!(pool.n_buffers_available(), 8);
        assert_eq!(pool.n_buffers_created_since_last_checked(), 0, "No new buffers should have been created");

        // 3. Take more items to trigger refill
        // pool size is 8, low_water_mark is 5.
        // get() -> size 7
        // get() -> size 6
        // get() -> size 5 (triggers refill)
        let _ = pool.get();
        let _ = pool.get();
        let _ = pool.get();
        assert_eq!(pool.n_buffers_available(), 5);
        assert_eq!(pool.n_buffers_created_since_last_checked(), 0, "Refill is async, counter shouldn't be updated yet");

        // 4. Wait for refill and check
        thread::sleep(Duration::from_millis(50)); // Give refiller time to run
        assert_eq!(pool.n_buffers_available(), 10, "Pool should be refilled to capacity");
        assert_eq!(pool.n_buffers_created_since_last_checked(), 5, "Refiller should have created 5 new buffers");
        assert_eq!(pool.n_buffers_created_since_last_checked(), 0, "Counter should be reset again");

        // 5. Drain the pool completely
        for _ in 0..10 {
            let _ = pool.get();
        }
        // At this point the refiller has likely kicked in again, wait for it to finish
        thread::sleep(Duration::from_millis(50));
        assert_eq!(pool.n_buffers_available(), 10);
        // We drain it again to ensure it is empty for the next step.
        for _ in 0..10 {
            let _ = pool.get();
        }
        assert_eq!(pool.n_buffers_available(), 0);
        // We don't care about the created count from the last refill for this test step. Reset it.
        pool.n_buffers_created_since_last_checked();


        // 6. Trigger on-demand fallback allocation
        let _ = pool.get();
        assert_eq!(pool.n_buffers_available(), 0, "Fallback allocation doesn't go into queue");
        assert_eq!(pool.n_buffers_created_since_last_checked(), 1, "Should count the one on-demand allocation");
    }
}

