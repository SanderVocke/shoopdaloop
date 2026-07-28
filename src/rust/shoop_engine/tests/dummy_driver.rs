//! Translation of `legacy C++ backend unit test test_DummyAudioMidiDriver.cpp`.
//!
//! Its first two cases drive a real thread: they start the driver, `wait_process()`
//! for it to run, and inspect what a tracker recorded. This driver owns no thread --
//! the caller runs the cycle and the driver only decides how many frames each cycle
//! gets -- so those two become assertions about the chunk sizes handed out, which is
//! the part carrying the behaviour. `wait_process` plus a tracker becomes calling
//! `next_chunk` and looking at what came back; `pause`/`resume` survive as they are.
//!
//! The four port cases translate directly.

use assert2::check;
use shoop_engine::dummy_driver::{DriverMode, DriverSettings, DummyDriver};
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::port::PortDirection;

fn driver(mode: DriverMode) -> DummyDriver {
    let mut d = DummyDriver::default();
    d.enter_mode(mode);
    d.start(DriverSettings {
        sample_rate: 48000,
        buffer_size: 256,
        client_name: "test".to_string(),
    });
    d
}

fn in_port() -> DummyAudioPort {
    DummyAudioPort::new(PortId(1), "test_in", PortDirection::Input, 4)
}

/// Chunk sizes a run of `cycles` cycles would process, dropping idle ones.
fn processed(d: &mut DummyDriver, cycles: usize) -> Vec<u32> {
    (0..cycles).map(|_| d.next_chunk()).collect()
}

#[test]
fn dummy_driver_automatic() {
    let mut d = driver(DriverMode::Automatic);

    let chunks = processed(&mut d, 4);
    d.close();

    check!(chunks.iter().sum::<u32>() > 0);
    // Always a whole buffer, never a partial one.
    check!(chunks.iter().all(|&n| n == 256));
}

#[test]
fn dummy_driver_controlled() {
    let mut d = driver(DriverMode::Controlled);

    // Nothing requested, so nothing is processed.
    check!(processed(&mut d, 4).iter().all(|&n| n == 0));

    d.request_samples(64);
    check!(d.samples_to_process() == 64);

    let chunks = processed(&mut d, 4);
    // Exactly the request, in one chunk since it is under a buffer, then idle.
    check!(chunks.iter().sum::<u32>() == 64);
    check!(chunks[0] == 64);
    check!(chunks[1..].iter().all(|&n| n == 0));
    check!(d.samples_to_process() == 0);

    d.close();
}

#[test]
fn dummy_driver_input_port_default() {
    let mut p = in_port();

    // Nothing queued and nothing processed, so the buffer reads as silence.
    check!(p.buffer(8) == [0.0; 8]);
}

#[test]
fn dummy_driver_input_port_queue() {
    let mut p = in_port();
    let data = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    p.queue_data(&data);

    p.prepare(8);
    p.process(8);
    check!(p.buffer(8) == data);
}

#[test]
fn dummy_driver_input_port_queue_consume_multiple() {
    let mut p = in_port();
    p.queue_data(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);

    p.prepare(4);
    p.process(4);
    check!(p.buffer(4) == [1.0, 2.0, 3.0, 4.0]);

    // More frames than the queue has left, so the remainder is silence.
    p.prepare(8);
    p.process(8);
    check!(p.buffer(8) == [5.0, 6.0, 7.0, 8.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn dummy_driver_input_port_queue_consume_combine() {
    let mut p = in_port();
    // Two separate queued blocks, which run together rather than each taking a cycle.
    p.queue_data(&[1.0, 2.0, 3.0, 4.0]);
    p.queue_data(&[1.0, 2.0, 3.0, 4.0]);

    p.prepare(10);
    p.process(10);
    check!(p.buffer(10) == [1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0]);
}
