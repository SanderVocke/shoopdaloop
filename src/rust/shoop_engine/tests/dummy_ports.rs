//! Translation of `legacy C++ backend unit test test_DummyPorts.cpp`.
//!
//! `DummyAudioPort("dummy", dir, pool)` takes an id here as well, since ports are
//! identified by index rather than by pointer, and the pool argument becomes the
//! capture ring's buffer size.
//!
//! `PROC_get_buffer` returned a raw pointer the caller wrote through; `buffer()`
//! returns a mutable slice, so the C++ `memcpy` into it becomes `copy_from_slice`.
//! The C++ `CHECK(buf != nullptr)` and `CHECK(buf == buf2)` assert that asking twice
//! within a cycle hands back the same storage, which becomes a check that the slice
//! has the requested length and keeps what was written to it.

use assert2::{check, let_assert};
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::port::PortDirection;

fn in_port(ringbuffer_buffer_size: usize) -> DummyAudioPort {
    DummyAudioPort::new(
        PortId(1),
        "dummy",
        PortDirection::Input,
        ringbuffer_buffer_size,
    )
}

fn out_port() -> DummyAudioPort {
    DummyAudioPort::new(PortId(2), "dummy", PortDirection::Output, 4)
}

/// Catch2's `Approx`, which defaults to a relative epsilon around 1e-5.
fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-5 * a.abs().max(b.abs()).max(1.0)
}

fn all_close(got: &[f32], want: &[f32]) -> bool {
    got.len() == want.len() && got.iter().zip(want).all(|(a, b)| close(*a, *b))
}

#[test]
fn dummy_audio_in_properties() {
    let p = in_port(4);

    check!(p.has_internal_read_access());
    check!(!p.has_internal_write_access());
    check!(p.has_implicit_input_source());
    check!(!p.has_implicit_output_sink());
}

#[test]
fn dummy_audio_in_buffers() {
    let mut p = in_port(4);

    p.prepare(10);
    p.process(10);
    check!(p.buffer(10).len() == 10);
    // Asking again within the cycle gives the same storage back, so a write through
    // the first view is visible through the second.
    p.buffer(10)[3] = 0.25;
    check!(p.buffer(10)[3] == 0.25);
}

#[test]
fn dummy_audio_in_queue() {
    let mut p = in_port(4);
    let samples = [0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0];

    p.queue_data(&samples);

    p.prepare(3);
    p.process(3);
    check!(all_close(p.buffer(3), &samples[..3]));

    p.prepare(3);
    p.process(3);
    check!(all_close(p.buffer(3), &samples[3..]));

    // The queue is drained, so further cycles read as silence.
    p.prepare(3);
    p.process(3);
    check!(all_close(p.buffer(3), &[0.0, 0.0, 0.0]));
}

#[test]
fn dummy_audio_in_gain() {
    let mut p = in_port(4);
    p.queue_data(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    p.audio_mut().set_gain(0.5);

    p.prepare(3);
    p.process(3);
    check!(all_close(p.buffer(3), &[0.0, 0.5, 1.0]));
}

#[test]
fn dummy_audio_in_mute() {
    let mut p = in_port(4);
    p.queue_data(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    p.audio_mut().set_muted(true);

    p.prepare(3);
    p.process(3);
    check!(all_close(p.buffer(3), &[0.0, 0.0, 0.0]));
}

#[test]
fn dummy_audio_in_peak() {
    let mut p = in_port(4);
    p.queue_data(&[5.0, 4.0, 3.0, 2.0, 1.0, 0.0]);

    p.prepare(2);
    p.process(2);
    check!(close(p.audio().input_peak(), 5.0));
    check!(close(p.audio().output_peak(), 5.0));

    // The peak is a running maximum, so a quieter cycle does not lower it.
    p.prepare(1);
    p.process(1);
    check!(close(p.audio().input_peak(), 5.0));
    check!(close(p.audio().output_peak(), 5.0));

    p.audio_mut().reset_input_peak();
    p.audio_mut().reset_output_peak();

    p.prepare(3);
    p.process(3);
    check!(close(p.audio().input_peak(), 2.0));
    check!(close(p.audio().output_peak(), 2.0));
}

#[test]
fn dummy_audio_in_get_ringbuffer_data() {
    let mut p = in_port(4);

    p.prepare(4);
    p.buffer(4).copy_from_slice(&[0.0, 0.1, 0.2, 0.3]);
    p.process(4);

    let s = p.audio().ringbuffer_contents();
    check!(s.n_samples >= 4);
    let last = s.buffers.last().expect("a captured buffer");
    check!(last[..4] == [0.0, 0.1, 0.2, 0.3]);
}

#[test]
fn dummy_audio_out_properties() {
    let p = out_port();

    check!(!p.has_internal_read_access());
    check!(p.has_internal_write_access());
    check!(!p.has_implicit_input_source());
    check!(p.has_implicit_output_sink());
}

#[test]
fn dummy_audio_out_buffers() {
    let mut p = out_port();

    p.prepare(10);
    p.process(10);
    check!(p.buffer(10).len() == 10);
    p.buffer(10)[3] = 0.25;
    check!(p.buffer(10)[3] == 0.25);
}

#[test]
fn dummy_audio_out_queue() {
    let mut p = out_port();
    let samples = [0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0];

    p.request_data(6);

    p.prepare(6);
    p.buffer(6).copy_from_slice(&samples);
    p.process(6);

    let_assert!(Ok(dequeued) = p.dequeue_data(6));
    check!(dequeued == samples);
}

#[test]
fn dummy_audio_out_gain() {
    let mut p = out_port();
    p.request_data(3);
    p.audio_mut().set_gain(0.5);

    p.prepare(3);
    p.buffer(3).copy_from_slice(&[0.0, 1.0, 2.0]);
    p.process(3);

    let_assert!(Ok(dequeued) = p.dequeue_data(3));
    check!(all_close(&dequeued, &[0.0, 0.5, 1.0]));
}

#[test]
fn dummy_audio_out_mute() {
    let mut p = out_port();
    p.request_data(3);
    p.audio_mut().set_muted(true);

    p.prepare(3);
    p.buffer(3).copy_from_slice(&[0.0, 1.0, 2.0]);
    p.process(3);

    let_assert!(Ok(dequeued) = p.dequeue_data(3));
    check!(all_close(&dequeued, &[0.0, 0.0, 0.0]));
}

#[test]
fn dummy_audio_out_peak() {
    let mut p = out_port();

    p.prepare(3);
    p.buffer(3).copy_from_slice(&[0.0, 1.0, 2.0]);
    p.process(3);

    check!(close(p.audio().input_peak(), 2.0));
    check!(close(p.audio().output_peak(), 2.0));

    p.audio_mut().reset_output_peak();
    p.audio_mut().reset_input_peak();
    p.audio_mut().set_muted(true);

    p.prepare(3);
    p.buffer(3).copy_from_slice(&[0.0, 1.0, 2.0]);
    p.process(3);

    // Muting is applied on the way out, so what was written still counts as input.
    check!(close(p.audio().input_peak(), 2.0));
    check!(close(p.audio().output_peak(), 0.0));
}

#[test]
fn dummy_audio_out_noop_zero() {
    let mut p = out_port();
    p.request_data(6);

    p.prepare(3);
    p.buffer(3).copy_from_slice(&[0.0, 1.0, 2.0]);
    p.process(3);

    let_assert!(Ok(dequeued) = p.dequeue_data(3));
    check!(all_close(&dequeued, &[0.0, 1.0, 2.0]));

    // A cycle nobody wrote to captures silence rather than repeating the last one.
    p.prepare(3);
    p.process(3);

    let_assert!(Ok(dequeued) = p.dequeue_data(3));
    check!(all_close(&dequeued, &[0.0, 0.0, 0.0]));
}
