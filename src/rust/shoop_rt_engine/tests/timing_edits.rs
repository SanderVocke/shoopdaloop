mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn done(h: &mut Harness, request: Request, quantum: usize) {
    assert_eq!(h.apply_in(request, quantum), Outcome::Done);
}
fn edit(
    h: &mut Harness,
    target: LoopSlotId,
    previous: TimingState,
    change: TimingEdit,
    quantum: usize,
) -> Outcome {
    h.apply_in(
        Request::LoopTiming {
            loop_id: target,
            expected_revision: previous.revision,
            expected_content: previous.content_revision,
            edit: change,
        },
        quantum,
    )
}
fn applied(outcome: Outcome) -> TimingState {
    let Outcome::TimingApplied(state) = outcome else {
        panic!("timing refused: {outcome:?}")
    };
    state
}

#[shoop_test]
fn window_edits_keep_raw_recordings_atomic_results_and_recoverable_state_at_every_quantum() {
    for quantum in [1, 16, 32, 64, 128] {
        let mut settings = description();
        settings.quantum = quantum;
        settings.max_frames = quantum.max(16);
        let mut h = Harness::new(&settings);
        h.input = Box::new(|channel, _| (channel + 1) as f32);
        let strip = strip(0, 1);
        let target = loop_id(0, 1);
        done(
            &mut h,
            Request::CreateStrip {
                strip,
                shape: shape(2, 2, false),
            },
            quantum,
        );
        for channel in 0..2 {
            done(
                &mut h,
                Request::Connect {
                    port: port(strip, PortKind::AudioIn, channel),
                    host: audio_in(channel as u32),
                    connected: true,
                },
                quantum,
            );
            done(&mut h, route(strip, channel, channel, true), quantum);
        }
        done(
            &mut h,
            Request::CreateLoop {
                loop_id: target,
                strip,
            },
            quantum,
        );
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Recording,
                delay: None,
            },
            quantum,
        );
        h.run(8 * quantum, quantum);
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Stopped,
                delay: None,
            },
            quantum,
        );
        let (
            _,
            Telemetry::Loop {
                content_revision, ..
            },
        ) = h.last_loop(target).unwrap()
        else {
            panic!("missing loop")
        };
        let state = TimingState {
            content_revision,
            ..TimingState::default()
        };
        let state = applied(edit(
            &mut h,
            target,
            state,
            TimingEdit {
                length: Some((2 * quantum) as u64),
                ..TimingEdit::default()
            },
            quantum,
        ));
        assert!(state.raw_audio_frames >= (8 * quantum) as u64);
        assert_eq!(state.length, (2 * quantum) as u64);
        let mut channels = [ChannelTimingUpdate::default(); MAX_TIMING_UPDATES];
        channels[0] = ChannelTimingUpdate {
            channel: 1,
            start_offset: Some(state.raw_audio_frames as i32),
            ..Default::default()
        };
        let state = applied(edit(
            &mut h,
            target,
            state,
            TimingEdit {
                count: 1,
                channels,
                ..Default::default()
            },
            quantum,
        ));
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Playing,
                delay: None,
            },
            quantum,
        );
        h.run(8 * quantum, quantum);
        assert!(h.outputs[0]
            .iter()
            .rev()
            .take(quantum)
            .all(|sample| *sample == 1.));
        assert!(h.outputs[1]
            .iter()
            .rev()
            .take(quantum)
            .all(|sample| *sample == 0.));
        let (total, samples, _) = h.read(target, BulkSelector::Audio { channel: 1 });
        assert_eq!(total, state.raw_audio_frames);
        assert_eq!(samples, vec![2.; total as usize]);
        let foreign = ChannelTimingUpdate {
            channel: 7,
            start_offset: Some(0),
            ..Default::default()
        };
        let mut invalid = TimingEdit {
            count: 1,
            ..Default::default()
        };
        invalid.channels[0] = foreign;
        assert_eq!(
            edit(&mut h, target, state, invalid, quantum),
            Outcome::Refused(Refusal::InvalidId)
        );
        channels[0].start_offset = Some(0);
        channels[1] = channels[0];
        assert!(matches!(
            edit(
                &mut h,
                target,
                state,
                TimingEdit {
                    count: 2,
                    channels,
                    ..Default::default()
                },
                quantum
            ),
            Outcome::Refused(_)
        ));
        assert!(matches!(
            edit(
                &mut h,
                target,
                TimingState {
                    revision: state.revision - 1,
                    ..state
                },
                TimingEdit {
                    start_offset: Some(0),
                    ..Default::default()
                },
                quantum
            ),
            Outcome::Refused(_)
        ));
        if quantum > 1 {
            assert_eq!(
                edit(
                    &mut h,
                    target,
                    state,
                    TimingEdit {
                        length: Some(quantum as u64 + 1),
                        ..Default::default()
                    },
                    quantum
                ),
                Outcome::Refused(Refusal::InvalidValue)
            );
        }
        assert_no_heap(|| h.host.client.resync()).unwrap();
        h.pump();
        assert!(h.states.contains(&StateRecord::LoopTiming {
            loop_id: target,
            timing: state
        }));
        let empty = applied(edit(
            &mut h,
            target,
            state,
            TimingEdit {
                length: Some(0),
                ..Default::default()
            },
            quantum,
        ));
        let (total, samples, _) = h.read(target, BulkSelector::Audio { channel: 0 });
        assert_eq!(total, empty.raw_audio_frames);
        assert_eq!(samples, vec![1.; total as usize]);
        done(
            &mut h,
            Request::LoopTransition {
                loop_id: target,
                mode: LoopMode::Recording,
                delay: None,
            },
            quantum,
        );
        assert!(matches!(
            edit(
                &mut h,
                target,
                empty,
                TimingEdit {
                    start_offset: Some(2),
                    length: Some(quantum as u64),
                    ..Default::default()
                },
                quantum
            ),
            Outcome::Refused(_)
        ));
        let moved = applied(edit(
            &mut h,
            target,
            empty,
            TimingEdit {
                start_offset: Some(2),
                preplay: Some(quantum as u32),
                ..Default::default()
            },
            quantum,
        ));
        assert_eq!(moved.windows.audio[0].start_offset, 2);
        let (_, Telemetry::Loop { mode, .. }) = h.last_loop(target).unwrap() else {
            panic!("missing capture")
        };
        assert_eq!(mode, LoopMode::Recording);
    }
}
