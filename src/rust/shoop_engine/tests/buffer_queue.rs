//! Translation of `unit test test_BufferQueue.cpp`.
//!
//! `BufferQueue<int>(pool, n)` takes its buffer size from the pool's third
//! constructor argument and `n` as the buffer limit. There is no pool here: every
//! buffer is allocated up front, so `PROC_get()` becomes `snapshot()` and
//! `data->at(i)->at(j)` becomes `snapshot.buffers[i][j]`.

use assert2::check;
use shoop_engine::buffer_queue::BufferQueue;

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_starting_state() {
    let q = BufferQueue::new(10, 10);

    check!(q.n_samples() == 0);
    check!(q.snapshot().n_samples == 0);
    check!(q.snapshot().buffers.is_empty());
}

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_single_buf_full() {
    let mut q = BufferQueue::new(10, 10);
    let data: Vec<f32> = (1..=10).map(|v| v as f32).collect();

    q.put(&data);

    check!(q.n_samples() == 10);
    let s = q.snapshot();
    check!(s.n_samples == 10);
    check!(s.buffers.len() == 1);
    check!(s.buffers[0] == data);
}

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_single_buf_partial() {
    let mut q = BufferQueue::new(10, 10);
    let data: Vec<f32> = (1..=10).map(|v| v as f32).collect();

    q.put(&data[..3]);

    check!(q.n_samples() == 3);
    let s = q.snapshot();
    check!(s.n_samples == 3);
    check!(s.buffers.len() == 1);
    // The last buffer is only partly filled, so only `n_samples` of it counts.
    check!(s.buffers[0][..3] == data[..3]);
    check!(s.contiguous() == data[..3]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_two_bufs_full() {
    let mut q = BufferQueue::new(4, 4);
    let data: Vec<f32> = (1..=8).map(|v| v as f32).collect();

    q.put(&data);

    check!(q.n_samples() == 8);
    let s = q.snapshot();
    check!(s.n_samples == 8);
    check!(s.buffers.len() == 2);
    check!(s.buffers[0] == data[..4]);
    check!(s.buffers[1] == data[4..]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_two_bufs_partial() {
    let mut q = BufferQueue::new(4, 4);
    let data: Vec<f32> = (1..=6).map(|v| v as f32).collect();

    q.put(&data);

    check!(q.n_samples() == 6);
    let s = q.snapshot();
    check!(s.n_samples == 6);
    check!(s.buffers.len() == 2);
    check!(s.buffers[0] == data[..4]);
    check!(s.buffers[1][..2] == data[4..]);
    check!(s.contiguous() == data);
}

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_drop_buffer() {
    // Room for two buffers of two samples, so a third buffer pushes the first out.
    let mut q = BufferQueue::new(2, 2);

    q.put(&[1.0, 2.0, 3.0, 4.0]);

    check!(q.n_samples() == 4);
    let s = q.snapshot();
    check!(s.n_samples == 4);
    check!(s.buffers.len() == 2);
    check!(s.buffers[0] == vec![1.0, 2.0]);
    check!(s.buffers[1] == vec![3.0, 4.0]);

    q.put(&[5.0, 6.0]);

    check!(q.n_samples() == 4);
    let s = q.snapshot();
    check!(s.n_samples == 4);
    check!(s.buffers.len() == 2);
    // The window moved: {1,2} was dropped.
    check!(s.buffers[0] == vec![3.0, 4.0]);
    check!(s.buffers[1] == vec![5.0, 6.0]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_drop_buffer_then_lower_the_limit() {
    let mut q = BufferQueue::new(2, 2);

    q.put(&[1.0, 2.0, 3.0, 4.0]);
    q.put(&[5.0, 6.0]);
    check!(q.snapshot().n_samples == 4);

    // Lowering the limit shrinks the window kept from here on.
    q.set_max_buffers(1);
    q.put(&[7.0, 8.0, 9.0, 10.0]);

    let s = q.snapshot();
    check!(s.n_samples == 2);
    check!(s.buffers.len() == 1);
    check!(s.buffers[0] == vec![9.0, 10.0]);
}

#[tracy_nextest_capture::tracy_capture_test]
fn buffer_queue_snapshot_then_drop() {
    let mut q = BufferQueue::new(2, 2);

    q.put(&[1.0, 2.0, 3.0, 4.0]);
    // buffers being shared and refcounted for the same guarantee.
    let snapshot = q.snapshot();
    q.put(&[5.0, 6.0]);

    let s = q.snapshot();
    check!(s.n_samples == 4);
    check!(s.buffers[0] == vec![3.0, 4.0]);
    check!(s.buffers[1] == vec![5.0, 6.0]);

    check!(snapshot.n_samples == 4);
    check!(snapshot.buffers[0] == vec![1.0, 2.0]);
    check!(snapshot.buffers[1] == vec![3.0, 4.0]);
}
