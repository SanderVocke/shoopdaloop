#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod support;
use shoop_backend::*;
use shoop_rt_backend::{ImmediateCommand as Command, SnapshotDelivery};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use shoop_wasm_test_support::shoop_test;
use support::*;
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[shoop_test]
fn immediate_controls_and_graph_edits_share_the_application_session() {
    let mut b = stack(&description());
    let track = b
        .create_direct_track(DirectTrackRequest {
            port_name_base: "mixed-commands".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    b.wait_idle();
    b.immediate_group(&[
        Command::LoopGain(track.loops[0], 0.25),
        Command::OutputBalance(track.track_id, -0.5),
    ])
    .unwrap();
    let bus = b
        .create_bus(BackendBusRequest {
            name: "live edit".into(),
            channel_count: 2,
            fx: None,
        })
        .unwrap();
    b.immediate_group(&[Command::LoopGain(track.loops[0], 0.9)])
        .unwrap();
    b.wait_idle();
    for _ in 0..16 {
        b.clock_mut().render(16);
        b.pump();
    }
    let state = b.poll().unwrap();
    assert!(state.mixer.buses.contains_key(&bus.bus_id));
    assert_eq!(state.loops[&track.loops[0]].gain, 0.25);
    assert_eq!(state.tracks[&track.track_id].output_balance, -0.5);
    assert_eq!(
        b.immediate_diagnostics().rt_refused,
        1,
        "the queued old configuration is refused"
    );
    b.immediate_group(&[
        Command::LoopGain(track.loops[0], 0.5),
        Command::BusMute(bus.bus_id, true),
    ])
    .unwrap();
    b.set_loop_gain(track.loops[0], 0.75).unwrap();
    b.wait_idle();
    b.remove_bus(bus.bus_id).unwrap();
    b.wait_idle();
    for _ in 0..16 {
        b.clock_mut().render(16);
        b.pump();
    }
    let state = b.poll().unwrap();
    assert_eq!(state.loops[&track.loops[0]].gain, 0.75);
    assert!(!state.mixer.buses.contains_key(&bus.bus_id));
    b.immediate_group(&[Command::LoopGain(track.loops[0], 0.6)])
        .unwrap();
    b.clock_mut().render(16);
    b.pump();
    assert_eq!(b.poll().unwrap().loops[&track.loops[0]].gain, 0.6);
    for index in 0..40 {
        b.set_loop_gain(track.loops[0], index as f32 / 50.).unwrap();
    }
    let cursor = b.mutation_cursor();
    assert!(b
        .immediate_group(&[Command::LoopGain(track.loops[0], 0.95)])
        .is_err());
    assert_eq!(b.mutation_cursor(), cursor);
    b.wait_idle();
    assert_eq!(b.poll().unwrap().loops[&track.loops[0]].gain, 0.78);
    b.immediate_group(&[Command::LoopGain(track.loops[0], 0.65)])
        .unwrap();
    b.clock_mut().render(16);
    b.pump();
    assert_eq!(b.poll().unwrap().loops[&track.loops[0]].gain, 0.65);
    let cursor = b.mutation_cursor();
    for index in 0..shoop_rt_protocol::immediate::FIFO_SINGLETONS {
        b.immediate_group(&[Command::LoopGain(track.loops[0], index as f32 / 1000.)])
            .unwrap();
    }
    assert_eq!(b.mutation_cursor(), cursor);
    assert_no_heap(|| b.clock_mut().render(1));
    b.pump();
    assert_eq!(b.poll().unwrap().loops[&track.loops[0]].gain, 0.127);
    b.immediate_group(&[
        Command::LoopGain(track.loops[0], 0.25),
        Command::LoopGain(BackendLoopId::from_raw(u64::MAX), 0.9),
        Command::LoopGain(track.loops[0], 0.5),
    ])
    .unwrap();
    assert_no_heap(|| b.clock_mut().render(1));
    b.pump();
    assert_eq!(b.poll().unwrap().loops[&track.loops[0]].gain, 0.5);
    assert_eq!(b.immediate_diagnostics().rt_refused, 2);
}

#[shoop_test]
fn additional_bus_uses_the_snapshot_generation_for_immediate_controls() {
    let mut b = stack(&description());
    let track = b
        .create_direct_track(DirectTrackRequest {
            port_name_base: "bus-generation".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    b.wait_idle();
    b.input_edits(
        &[BackendInputEdit::Mode {
            loop_id: track.loops[0],
            mode: BackendLoopMode::Stopped,
            position: None,
        }],
        None,
    )
    .unwrap();
    b.wait_idle();
    let bus = b
        .create_bus(BackendBusRequest {
            name: "Additional".into(),
            channel_count: 2,
            fx: None,
        })
        .unwrap();
    for _ in 0..32 {
        b.clock_mut().render(16);
        b.pump();
    }
    assert!(!b.mutations_pending_through(b.mutation_cursor()));
    assert!(
        b.snapshot_frame().unwrap() >= b.clock().frame() - 16,
        "an active additional bus must not invalidate whole snapshots"
    );
    b.immediate_group(&[
        Command::BusGain(bus.bus_id, 0.25),
        Command::BusBalance(bus.bus_id, -0.5),
        Command::BusMute(bus.bus_id, true),
        Command::LoopGain(track.loops[0], 0.5),
    ])
    .unwrap();
    b.clock_mut().render(16);
    b.pump();
    let state = b.poll().unwrap();
    assert_eq!(b.immediate_diagnostics().rt_refused, 0);
    assert_eq!(state.loops[&track.loops[0]].gain, 0.5);
    let bus = &state.mixer.buses[&bus.bus_id];
    assert!((bus.gain_db + 12.0412).abs() < 0.0001);
    assert_eq!(bus.balance, -0.5);
    assert!(bus.muted);
}

#[shoop_test]
fn thirty_loaded_loops_start_together_with_ordered_results_and_snapshots() {
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
    let mut first_starts = starts.clone();
    first_starts.insert(15, Command::Start(BackendLoopId::from_raw(u64::MAX)));
    let responses = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let received = responses.clone();
    b.set_immediate_response_handler(move |response| received.lock().unwrap().push(*response));
    let credits = b.mutation_cursor();
    b.immediate_group(&first_starts).unwrap();
    assert_eq!(b.mutation_cursor(), credits);
    assert_no_heap(|| b.clock_mut().render(1));
    b.pump();
    {
        use shoop_rt_protocol::immediate::Code;
        let observed = responses.lock().unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].header.deadline(), None);
        assert_eq!(observed[0].records().len(), 31);
        for (index, record) in observed[0].records().iter().enumerate() {
            assert_eq!(
                record.code(),
                if index == 15 {
                    Code::TargetUnavailable
                } else {
                    Code::Applied
                }
            );
        }
    }
    let state = b.poll().unwrap();
    assert_eq!(
        b.immediate_diagnostics().rt_refused,
        1,
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

#[shoop_test]
fn immediate_recording_uses_rt_state_before_snapshots_and_preserves_exported_audio() {
    fn audio(b: &mut Stack, id: BackendLoopId) -> Vec<std::sync::Arc<[f32]>> {
        for _ in 0..100 {
            if let Some(audio) = b.loop_audio_data(id).unwrap() {
                return audio;
            }
            b.clock_mut().render(16);
            b.pump();
        }
        panic!("audio export did not arrive");
    }
    let mut b = stack(&description());
    b.clock_mut().capture = |_| 0.25;
    let created = b
        .create_direct_track(DirectTrackRequest {
            port_name_base: "recording".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    let id = created.loops[0];
    b.set_port_connected(created.ports[0].id, "audio_in_1", true)
        .unwrap();
    b.wait_idle();
    for _ in 0..4 {
        b.clock_mut().render(1);
        b.pump();
    }
    let responses = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let received = responses.clone();
    b.set_immediate_response_handler(move |r| received.lock().unwrap().push(*r));
    let cursor = b.mutation_cursor();
    b.immediate_group(&[Command::Record(id)]).unwrap();
    assert_no_heap(|| b.clock_mut().render(8));
    b.immediate_group(&[Command::Start(id)]).unwrap();
    assert_no_heap(|| b.clock_mut().render(1));
    for _ in 0..8 {
        b.pump();
    }
    let state = b.poll().unwrap();
    assert_eq!(
        state.loops[&id].mode,
        BackendLoopMode::Playing,
        "{:?}",
        responses.lock().unwrap()
    );
    assert_eq!(state.loops[&id].length, 8);
    assert_eq!(b.mutation_cursor(), cursor, "no legacy recording requests");
    assert_eq!(b.immediate_diagnostics().rt_refused, 0);
    let old = audio(&mut b, id);
    assert_eq!(&*old[0], &[0.25; 8]);
    b.wait_idle();
    b.clock_mut().capture = |_| -0.5;
    b.immediate_group(&[Command::Record(id)]).unwrap();
    for _ in 0..64 {
        assert_no_heap(|| b.clock_mut().render(1));
        b.pump();
        if responses.lock().unwrap().len() == 3 {
            break;
        }
    }
    assert_eq!(
        responses.lock().unwrap().len(),
        3,
        "bulk retirement must release later commands"
    );
    let start = responses.lock().unwrap()[2].header.frame();
    assert_no_heap(|| b.clock_mut().render(4));
    b.immediate_group(&[Command::Stop(id)]).unwrap();
    assert_no_heap(|| b.clock_mut().render(1));
    for _ in 0..8 {
        b.pump();
    }
    let state = b.poll().unwrap();
    assert_eq!(
        state.loops[&id].mode,
        BackendLoopMode::Stopped,
        "{:?}",
        responses
            .lock()
            .unwrap()
            .iter()
            .map(|r| (
                r.header.frame(),
                r.records()
                    .iter()
                    .map(|v| (v.command(), v.code()))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
    let frames = responses.lock().unwrap()[3].header.frame() - start;
    assert_eq!(u64::from(state.loops[&id].length), frames);
    let new = audio(&mut b, id);
    assert_eq!(&*new[0], vec![-0.5; frames as usize]);
    assert_eq!(&*old[0], &[0.25; 8]);
    assert_eq!(b.immediate_diagnostics().rt_refused, 0);
}
