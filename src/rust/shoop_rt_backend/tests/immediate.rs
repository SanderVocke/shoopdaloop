mod support;
use shoop_backend::*;
use shoop_rt_backend::{ImmediateCommand as Command, SnapshotDelivery};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use shoop_wasm_test_support::shoop_test;
use support::*;
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[shoop_test]
fn thirty_loaded_loops_start_together_and_snapshots_observe_without_receipts() {
    let mut d = description();
    d.master_connections = vec![(0, 0), (1, 1)];
    d.ramp_frames = 0;
    d.smoothing_frames = 0;
    let mut b = stack(&d);
    let track = b
        .create_direct_track(DirectTrackRequest {
            port_name_base: "immediate".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 30,
        })
        .unwrap();
    for (port, channel) in track
        .ports
        .iter()
        .filter(|p| p.role == BackendPortRole::AudioOutput)
        .zip(MASTER_BUS_CHANNEL_IDS)
    {
        b.set_mixer_route(port.id, channel, true).unwrap();
    }
    b.wait_idle();
    let content = BackendLoopContent {
        recorded_processor_state: None,
        source_id: 0,
        length: 64,
        gain: 1.,
        balance: 0.,
        audio: (0..2)
            .map(|_| BackendAudioContent {
                mode: BackendChannelMode::Direct,
                samples: std::sync::Arc::new(
                    (0..64).map(|i| if i == 0 { 0.01 } else { 0. }).collect(),
                ),
                gain: 1.,
                start_offset: 0,
                capture_alignment_frames: 0,
                preplay: 0,
            })
            .collect(),
        midi: vec![],
    };
    for &id in &track.loops {
        let mut loaded = false;
        for _ in 0..4000 {
            if matches!(
                b.load_loop_content_async(id, &content).unwrap(),
                BackendAsyncResult::Ready(())
            ) {
                loaded = true;
                break;
            }
            b.clock_mut().render(16);
        }
        assert!(loaded);
    }
    b.wait_idle();
    b.clock_mut().render(16);
    b.pump();
    let starts: Vec<_> = track.loops.iter().copied().map(Command::Start).collect();
    let credits = b.mutation_cursor();
    b.immediate_group(&starts).unwrap();
    assert_eq!(b.mutation_cursor(), credits);
    assert_no_heap(|| b.clock_mut().render(1));
    b.pump();
    let state = b.poll().unwrap();
    assert_eq!(
        b.immediate_diagnostics().rt_refused,
        0,
        "{:?}",
        b.immediate_diagnostics()
    );
    for id in &track.loops {
        assert_eq!(state.loops[id].mode, BackendLoopMode::Playing);
        assert_eq!(state.loops[id].position, 0);
    }
    assert!(
        (b.clock().offline.output(0)[0] - 0.30).abs() < 0.0001,
        "{}",
        b.clock().offline.output(0)[0]
    );
    b.clock_mut().render(16);
    b.pump();
    b.immediate_group(&starts).unwrap();
    assert_no_heap(|| b.clock_mut().render(1));
    b.pump();
    assert!((b.clock().offline.output(0)[0] - 0.30).abs() < 0.0001);
    b.set_snapshot_delivery(SnapshotDelivery::push(120).unwrap())
        .unwrap();
    b.immediate_group(&[
        Command::LoopGain(track.loops[0], 0.25),
        Command::OutputBalance(track.track_id, 0.5),
    ])
    .unwrap();
    b.clock_mut().render(16);
    b.pump();
    b.clock_mut().render(16);
    b.pump();
    let state = b.poll().unwrap();
    assert_eq!(state.loops[&track.loops[0]].gain, 0.25);
    assert_eq!(state.tracks[&track.track_id].output_balance, 0.5);
    let stops: Vec<_> = track.loops.iter().copied().map(Command::Stop).collect();
    b.immediate_group(&stops).unwrap();
    b.clock_mut().render(16);
    b.pump();
    b.clock_mut().render(16);
    b.pump();
    assert!(b
        .poll()
        .unwrap()
        .loops
        .values()
        .all(|l| l.mode == BackendLoopMode::Stopped));
    let mut read = None;
    for _ in 0..100 {
        read = b.loop_audio_data(track.loops[0]).unwrap();
        if read.is_some() {
            break;
        }
        b.clock_mut().render(16);
        b.pump();
    }
    let read = read.expect("legacy bulk reads remain available");
    assert_eq!(read[0].len(), 64);
    assert_eq!(read[0][0], 0.01);
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::{Arc, Barrier};
        b.immediate_group(&[
            Command::LoopGain(track.loops[0], 1.),
            Command::OutputBalance(track.track_id, 0.),
        ])
        .unwrap();
        b.clock_mut().render(16);
        b.pump();
        b.set_snapshot_delivery(SnapshotDelivery::Pull).unwrap();
        let (placeholder, _) = parts(&description());
        let mut clock = std::mem::replace(b.clock_mut(), placeholder);
        let mut ui = b;
        let ids = track.loops.clone();
        let group: Vec<_> = ids.iter().copied().map(Command::Start).collect();
        let begin = Arc::new(Barrier::new(2));
        let done = Arc::new(Barrier::new(2));
        let (render_begin, render_done) = (begin.clone(), done.clone());
        let render = std::thread::spawn(move || {
            let mut output = [0.; 2];
            for value in &mut output {
                render_begin.wait();
                assert_no_heap(|| clock.render(1));
                *value = clock.offline.output(0)[0];
                render_done.wait();
            }
            output
        });
        begin.wait();
        ui.immediate_group(&group).unwrap();
        done.wait();
        ui.pump();
        let first = ui.poll().unwrap();
        let mode = first.loops[&ids[0]].mode;
        assert!(ids.iter().all(|id| first.loops[id].mode == mode));
        begin.wait();
        done.wait();
        ui.pump();
        let second = ui.poll().unwrap();
        let position = second.loops[&ids[0]].position;
        assert!(ids
            .iter()
            .all(|id| second.loops[id].mode == BackendLoopMode::Playing
                && second.loops[id].position == position));
        let output = render.join().unwrap();
        assert!((output[0] + output[1] - 0.30).abs() < 0.0001, "{output:?}");
        assert!(
            output
                .iter()
                .all(|v| v.abs() < 0.0001 || (*v - 0.30).abs() < 0.0001),
            "{output:?}"
        );
    }
}
