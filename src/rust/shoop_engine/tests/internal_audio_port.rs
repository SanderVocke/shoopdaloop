//! Translation of `src/backend/test/unit/test_InternalAudioPort.cpp`.
//!
//! `InternalAudioPort<float>("dummy", 10, 0, 0, pool)` becomes a buffer of 10 frames
//! with no connectability either way; the pool argument becomes the ringbuffer's
//! buffer size, since the capture ring allocates its own buffers up front.
//!
//! `PROC_get_buffer` handed out a raw pointer that the caller wrote through, so the
//! C++ cases `memcpy` into it. `buffer()` returns a mutable slice, which is the same
//! thing with a length attached.

use assert2::check;
use shoop_engine::internal_audio_port::InternalAudioPort;
use shoop_engine::port::PortConnectability;

fn port(ringbuffer_buffer_size: usize) -> InternalAudioPort {
    InternalAudioPort::new(
        "dummy",
        10,
        PortConnectability::NONE,
        PortConnectability::NONE,
        ringbuffer_buffer_size,
    )
}

/// Catch2's `Approx`, which defaults to a relative epsilon around 1e-5.
fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-5 * a.abs().max(b.abs()).max(1.0)
}

#[test]
fn internal_audio_port_properties() {
    let p = port(4);

    check!(p.has_internal_read_access());
    check!(p.has_internal_write_access());
    check!(!p.has_implicit_input_source());
    check!(!p.has_implicit_output_sink());
}

#[test]
fn internal_audio_port_gain() {
    let mut p = port(4);
    let samples = [0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0];

    p.audio_mut().set_gain(0.5);

    p.prepare(3);
    p.buffer(3).copy_from_slice(&samples[..3]);
    p.process(3);

    let buf = p.buffer(3);
    check!(close(buf[0], 0.0));
    check!(close(buf[1], 0.5));
    check!(close(buf[2], 1.0));
}

#[test]
fn internal_audio_port_mute() {
    let mut p = port(4);
    let samples = [0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0];

    p.audio_mut().set_muted(true);

    p.prepare(3);
    p.buffer(3).copy_from_slice(&samples[..3]);
    p.process(3);

    let buf = p.buffer(3);
    check!(close(buf[0], 0.0));
    check!(close(buf[1], 0.0));
    check!(close(buf[2], 0.0));
}

#[test]
fn internal_audio_port_peak() {
    let mut p = port(4);
    let samples = [0.0f32, 0.5, 0.9, 0.5, 0.0];

    p.prepare(5);
    p.buffer(5).copy_from_slice(&samples);
    p.process(5);

    check!(close(p.audio().input_peak(), 0.9));
    check!(close(p.audio().output_peak(), 0.9));

    // Muting silences the output but not what arrived, so the input peak still
    // reflects the signal that was there.
    p.audio_mut().set_muted(true);
    p.audio_mut().reset_input_peak();
    p.audio_mut().reset_output_peak();
    p.prepare(5);
    p.buffer(5).copy_from_slice(&samples);
    p.process(5);

    check!(close(p.audio().input_peak(), 0.9));
    check!(close(p.audio().output_peak(), 0.0));
}

#[test]
fn internal_audio_port_noop_zero() {
    let mut p = port(4);
    let samples = [0.0f32, 0.5, 0.9, 0.5, 0.0];

    p.prepare(5);
    p.buffer(5).copy_from_slice(&samples);
    p.process(5);

    check!(close(p.audio().input_peak(), 0.9));
    check!(close(p.audio().output_peak(), 0.9));

    // A cycle nobody wrote to reads as silence: preparing clears the buffer, so
    // last cycle's samples cannot leak into this one.
    p.prepare(5);
    p.process(5);
    let buf = p.buffer(5);
    check!(buf[..5].iter().all(|&v| close(v, 0.0)));
}

#[test]
fn internal_audio_port_get_ringbuffer_data() {
    let mut p = port(4);

    p.prepare(4);
    p.buffer(4).copy_from_slice(&[0.0, 0.1, 0.2, 0.3]);
    p.process(4);

    let s = p.audio().ringbuffer_contents();
    check!(s.n_samples >= 4);
    let last = s.buffers.last().expect("a captured buffer");
    check!(last[..4] == [0.0, 0.1, 0.2, 0.3]);
}
