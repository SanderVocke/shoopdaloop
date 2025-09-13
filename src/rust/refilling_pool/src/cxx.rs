use crate::refilling_pool::RefillingPool;

#[cxx::bridge(namespace = "refilling_pool")]
mod ffi {

    /// A struct passed to C++ containing the raw pointer and length of a buffer.
    #[derive(Debug, Default)]
    struct Buffer {
        ptr: *mut u8,
        len: usize,
    }

    // Opaque types are Rust structs that can be moved and passed around in C++
    // as `UniquePtr<T>` or raw pointers, but their fields are not accessible.
    extern "Rust" {
        /// Represents the entire buffer pool. C++ will hold a `UniquePtr<BufferPool>`.
        type BufferPool;

        /// Represents a single buffer (`Vec<u8>`). C++ will pass this around as an
        /// opaque `BufferHandle*`. This avoids exposing `std::vector` layout details.
        type BufferHandle;

        /// Creates a new pool.
        /// `buffer_size` is the size in bytes for each allocated buffer.
        fn new_buffer_pool(
            capacity: usize,
            low_water_mark: usize,
            buffer_size: usize,
        ) -> Box<BufferPool>;

        /// Retrieves a buffer handle from the pool.
        /// Returns a raw pointer to a `BufferHandle`, which must be returned
        /// to the pool via `release_buffer`.
        fn get_buffer(self: &mut BufferPool) -> *mut BufferHandle;

        /// Returns a buffer handle to the pool for reuse.
        /// The handle becomes invalid after this call.
        unsafe fn release_buffer(self: &mut BufferPool, handle: *mut BufferHandle);

        /// A free function to safely get the data pointer and length from a handle.
        /// This is safer than trying to dereference the opaque pointer in C++.
        unsafe fn get_buffer_data(handle: *mut BufferHandle) -> Buffer;

        /// Number of available buffers in the pool.
        fn n_buffers_available(self: &BufferPool) -> usize;

        /// Number of buffers created since the last call to this function.
        fn n_buffers_created_since_last_checked(self: &BufferPool) -> usize;
    }
}

use std::fmt::Debug;

/// The concrete Rust type for the opaque `BufferPool`.
/// It wraps the generic `RefillingPool` with a concrete type: `BufferHandle`.
pub struct BufferPool {
    pool: RefillingPool<BufferHandle>,
}

/// The concrete Rust type for the opaque `BufferHandle`.
/// This must be a distinct struct, not a type alias, for cxx.
pub struct BufferHandle(Vec<u8>);

impl Debug for BufferHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferHandle")
            .field("len", &self.0.len())
            .finish()
    }
}

/// Implementation of the C++-callable functions.

fn new_buffer_pool(capacity: usize, low_water_mark: usize, buffer_size: usize) -> Box<BufferPool> {
    // The factory closure captures the desired buffer size.
    let factory = move || Box::new(BufferHandle(vec![0u8; buffer_size]));

    let pool =
        RefillingPool::new(capacity, low_water_mark, factory).expect("Failed to create pool");

    Box::new(BufferPool { pool })
}

impl BufferPool {
    /// Acquires a `Box<BufferHandle>`, consumes it, and returns a raw pointer,
    /// transferring ownership to the C++ side until `release_buffer` is called.
    fn get_buffer(&mut self) -> *mut BufferHandle {
        let buffer_box = self.pool.get();
        Box::into_raw(buffer_box)
    }

    /// Takes a raw pointer from C++, reconstructs the `Box<BufferHandle>`,
    /// and returns it to the pool for reuse.
    fn release_buffer(&mut self, handle: *mut BufferHandle) {
        // This is safe under the assumption that the pointer was obtained
        // from `get_buffer` and has not been released yet.
        let buffer_box = unsafe { Box::from_raw(handle) };
        self.pool.release(buffer_box);
    }

    /// Number of buffers waiting in the queue.
    fn n_buffers_available(self: &BufferPool) -> usize {
        self.pool.n_buffers_available()
    }

    /// Number of buffers allocated since the last call to this method.
    fn n_buffers_created_since_last_checked(self: &BufferPool) -> usize {
        self.pool.n_buffers_created_since_last_checked()
    }
}

/// Free function to access buffer data from C++.
fn get_buffer_data(handle: *mut BufferHandle) -> ffi::Buffer {
    // This is safe as long as C++ provides a valid handle.
    // We get a mutable reference to the BufferHandle behind the raw pointer.
    let buffer_handle = unsafe { &mut *handle };
    // Then we get a mutable reference to the inner Vec<u8>.
    let buffer_vec = &mut buffer_handle.0;
    ffi::Buffer {
        ptr: buffer_vec.as_mut_ptr(),
        len: buffer_vec.len(),
    }
}
