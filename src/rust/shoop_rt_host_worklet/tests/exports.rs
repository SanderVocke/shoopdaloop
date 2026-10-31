//! The exported functions, used as the processor script uses them.
#![cfg(feature = "exports")]

use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_rt_host_worklet::abi::*;
use shoop_rt_host_worklet::WorkletConfig;
use shoop_rt_protocol::codec::decode_engine;
use shoop_rt_protocol::{EngineMessage, PROTOCOL_VERSION};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn config() -> WorkletConfig {
    WorkletConfig::new(EngineDescription {
        sample_rate: 1000,
        max_frames: 16,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 1,
            loops: 2,
        }],
        resource_pages: 512,
        page_bytes: 256,
        midi_events: 16,
        midi_bytes: 64,
        pending_transitions: 8,
        reliable_credits: 4,
        bulk_credits: 1,
        ..EngineDescription::default()
    })
}

/// Copy `bytes` to a module address, as the script does through a memory view.
fn write(pointer: *mut u8, bytes: &[u8]) {
    assert!(!pointer.is_null());
    // SAFETY: The module returned `pointer` for a buffer of at least
    // `bytes.len()` bytes that it does not borrow between calls.
    unsafe { std::slice::from_raw_parts_mut(pointer, bytes.len()) }.copy_from_slice(bytes);
}

fn read(pointer: *const u8, len: usize) -> Vec<u8> {
    assert!(!pointer.is_null());
    // SAFETY: The module returned `pointer` and `len` for its last quantum's
    // bytes, which stay unchanged until the next call.
    unsafe { std::slice::from_raw_parts(pointer, len) }.to_vec()
}

fn prepare(record: &[u8]) -> u32 {
    write(shoop_rt_worklet_config_buffer(record.len()), record);
    shoop_rt_worklet_prepare()
}

#[shoop_test]
fn prepares_renders_and_releases_one_host() {
    shoop_rt_worklet_release();
    assert_eq!(shoop_rt_worklet_process(16, 0, 0., -1.), NOT_PREPARED);
    assert_eq!(shoop_rt_worklet_prepare(), MISSING_CONFIG);
    assert!(shoop_rt_worklet_config_buffer(0).is_null());
    assert_eq!(prepare(&[1, 0, 0, 0]), INVALID_CONFIG);
    let config = config();
    let record = config.encode().unwrap();
    let mut refused = config.clone();
    refused.engine.max_frames = 0;
    assert_eq!(prepare(&refused.encode().unwrap()), ENGINE_REFUSED);
    assert_eq!(prepare(&record), PREPARED);
    assert_eq!(shoop_rt_worklet_prepare(), ALREADY_PREPARED);
    assert!(shoop_rt_worklet_config_buffer(record.len()).is_null());

    assert_eq!(shoop_rt_worklet_max_frames(), 16);
    assert_eq!(shoop_rt_worklet_audio_inputs(), 2);
    assert_eq!(shoop_rt_worklet_audio_outputs(), 2);
    assert!(!shoop_rt_worklet_audio_input_ptr().is_null());
    assert!(!shoop_rt_worklet_audio_output_ptr().is_null());
    for (id, capacity) in [
        (RECORDS_IN, config.records_in_bytes),
        (RECORDS_OUT, config.records_out_bytes),
        (MIDI_IN, config.midi_in_bytes),
        (MIDI_OUT, config.midi_out_bytes),
    ] {
        assert!(!shoop_rt_worklet_buffer_ptr(id).is_null());
        assert_eq!(shoop_rt_worklet_buffer_capacity(id), capacity);
        assert_eq!(shoop_rt_worklet_buffer_len(id), 0);
    }
    assert!(shoop_rt_worklet_buffer_ptr(4).is_null());
    assert!(!shoop_rt_worklet_commit(RECORDS_OUT, 1));
    assert!(!shoop_rt_worklet_commit(
        RECORDS_IN,
        config.records_in_bytes + 1
    ));

    let hello = [5, 0, 0, PROTOCOL_VERSION as u8, 0];
    write(shoop_rt_worklet_buffer_ptr(RECORDS_IN), &hello[..3]);
    assert!(shoop_rt_worklet_commit(RECORDS_IN, 3));
    assert_eq!(shoop_rt_worklet_process(17, 2, 0., -1.), INVALID_QUANTUM);
    assert_eq!(shoop_rt_worklet_process(16, 2, 0., -1.), RENDERED);
    assert_eq!(shoop_rt_worklet_buffer_len(RECORDS_OUT), 0);
    assert_eq!(shoop_rt_worklet_buffer_len(RECORDS_IN), 3);
    let at = shoop_rt_worklet_buffer_len(RECORDS_IN);
    write(
        shoop_rt_worklet_buffer_ptr(RECORDS_IN).wrapping_add(at),
        &hello[3..],
    );
    assert!(shoop_rt_worklet_commit(RECORDS_IN, 2));
    assert_eq!(shoop_rt_worklet_process(16, 2, 16., 0.5), RENDERED);
    assert_eq!(shoop_rt_worklet_buffer_len(RECORDS_IN), 0);
    let len = shoop_rt_worklet_buffer_len(RECORDS_OUT);
    let first = read(shoop_rt_worklet_buffer_ptr(RECORDS_OUT), len);
    let first = &first[..usize::from(u16::from_le_bytes([first[0], first[1]]))];
    assert!(matches!(
        decode_engine(first),
        Ok(EngineMessage::Welcome { .. })
    ));

    assert_eq!(shoop_rt_worklet_process(16, 0, 100., 2.), RENDERED);
    assert_eq!(shoop_rt_worklet_counter(DISCONTINUITIES), 1);
    assert_eq!(shoop_rt_worklet_counter(OVERRUNS), 1);
    shoop_rt_worklet_release();
    assert_eq!(shoop_rt_worklet_process(16, 0, 116., -1.), NOT_PREPARED);
    assert_eq!(shoop_rt_worklet_counter(DISCONTINUITIES), 0);
}
