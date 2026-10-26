use shoop_engine::audio_preview::AudioPreview;
use shoop_engine::internal_audio_port::InternalAudioPort;
use shoop_engine::session::{Port, Session};
use shoop_engine::PortConnectability;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

fn fixture() -> (Session, usize, usize, Arc<AtomicU64>) {
    let mut session = Session::default();
    session.set_buffer_size(4);
    let mut port = |name| {
        session.add_port(Port::Internal(InternalAudioPort::new(
            name,
            4,
            PortConnectability::INTERNAL,
            PortConnectability::INTERNAL,
            0,
        )))
    };
    let left = port("master_left");
    let right = port("master_right");
    let finished = Arc::new(AtomicU64::new(0));
    session.configure_audio_preview(&mut Some(AudioPreview::new(
        vec![left, right],
        Arc::clone(&finished),
    )));
    session.apply_graph_changes().unwrap();
    (session, left, right, finished)
}

fn buffer(session: &mut Session, port: usize) -> &[f32] {
    let Port::Internal(port) = session.port_mut(port).unwrap() else {
        panic!("internal port")
    };
    port.buffer(4)
}

#[shoop_wasm_test_support::shoop_test]
fn preview_streams_exact_blocks_and_finishes_without_loop_transport_or_realtime_allocation() {
    let (mut session, left, right, finished) = fixture();
    let mut samples: Arc<[f32]> = Arc::from([0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    session.replace_audio_preview(7, &mut samples);
    assert_no_alloc::assert_no_alloc(|| session.process(4));
    for port in [left, right] {
        assert_eq!(buffer(&mut session, port), &[0.1, 0.2, 0.3, 0.4]);
    }
    assert_eq!(finished.load(Ordering::Acquire), 0);
    assert_no_alloc::assert_no_alloc(|| session.process(4));
    for port in [left, right] {
        assert_eq!(buffer(&mut session, port), &[0.5, 0.6, 0.0, 0.0]);
    }
    assert_eq!(finished.load(Ordering::Acquire), 7);
    assert_no_alloc::assert_no_alloc(|| session.process(4));
    assert_eq!(buffer(&mut session, left), &[0.0; 4]);
}

#[shoop_wasm_test_support::shoop_test]
fn preview_replacement_stop_and_output_controls_keep_the_graph_and_reclaim_off_callback() {
    let (mut session, left, right, finished) = fixture();
    let graph = session.graph_applied_id();
    let original: Arc<[f32]> = Arc::from([0.25; 16]);
    let mut incoming = Arc::clone(&original);
    session.replace_audio_preview(1, &mut incoming);
    session.process(4);
    let mut replacement: Arc<[f32]> = Arc::from([0.5; 5]);
    assert_no_alloc::assert_no_alloc(|| session.replace_audio_preview(2, &mut replacement));
    assert!(Arc::ptr_eq(&original, &replacement));
    session
        .port_mut(left)
        .unwrap()
        .audio_mut()
        .unwrap()
        .set_gain(0.5);
    session
        .port_mut(right)
        .unwrap()
        .audio_mut()
        .unwrap()
        .set_muted(true);
    assert_no_alloc::assert_no_alloc(|| session.process(4));
    assert_eq!(buffer(&mut session, left), &[0.25; 4]);
    assert_eq!(buffer(&mut session, right), &[0.0; 4]);
    let mut silence: Arc<[f32]> = Arc::from([]);
    assert_no_alloc::assert_no_alloc(|| session.replace_audio_preview(0, &mut silence));
    assert_no_alloc::assert_no_alloc(|| session.process(4));
    assert_eq!(buffer(&mut session, left), &[0.0; 4]);
    assert_eq!(finished.load(Ordering::Acquire), 0);
    assert_eq!(session.graph_applied_id(), graph);
    assert_eq!(silence.len(), 5);
}

#[shoop_wasm_test_support::shoop_test]
fn preview_mixes_with_running_loop_and_keeps_source_order_when_processors_change() {
    use shoop_engine::{ChannelMode, LoopMode};
    let (mut session, left, right, _) = fixture();
    let loop_id = session.create_loop();
    let channel = session
        .add_audio_channel(loop_id, 4, ChannelMode::Direct)
        .unwrap();
    session.connect_channel_output(channel, left).unwrap();
    session
        .loop_mut(loop_id)
        .unwrap()
        .audio_channel_mut(0)
        .unwrap()
        .load_data(&[0.25; 4]);
    session.loop_mut(loop_id).unwrap().set_length(4);
    session.set_loop_mode(loop_id, LoopMode::Playing).unwrap();
    session.apply_graph_changes().unwrap();
    let mut samples: Arc<[f32]> = Arc::from([0.5; 8]);
    session.replace_audio_preview(3, &mut samples);
    session.set_external_processor("unrelated");
    // Until the new topology is applied, the old preview node must still run as a source.
    session.process(4);
    assert_eq!(buffer(&mut session, left), &[0.75; 4]);
    assert_eq!(buffer(&mut session, right), &[0.5; 4]);
    session.apply_graph_changes().unwrap();
    session.process(4);
    assert_eq!(buffer(&mut session, left), &[0.75; 4]);
    session.process(4);
    assert_eq!(buffer(&mut session, left), &[0.25; 4]);
    assert_eq!(buffer(&mut session, right), &[0.0; 4]);
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
