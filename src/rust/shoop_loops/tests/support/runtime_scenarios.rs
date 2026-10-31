use super::*;
use rt_runtime::{
    midi_state::{ChannelState, MidiState},
    resource::{self, Format},
};
use shoop_loops::media;
use shoop_loops_test_support::{LoopControl, LoopRuntime};

fn upload(
    store: &mut ResourceStore,
    format: Format,
    frames: u64,
    bytes: &[u8],
    initial: Option<&MidiState>,
) -> media::Source {
    let resource = store.create().unwrap();
    let description = resource::Description {
        format,
        frames,
        bytes: bytes.len(),
    };
    let token = if let Some(initial) = initial {
        store
            .begin_indexed_midi_upload(resource, Revision(0), description, initial)
            .unwrap()
    } else {
        store
            .begin_upload(resource, Revision(0), description)
            .unwrap()
    };
    for (page, chunk) in bytes.chunks(64).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 64, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    media::Source {
        snapshot,
        index: store.read_index(snapshot.id).unwrap(),
    }
}
pub(super) fn make_imports(store: &mut ResourceStore, one_sample: bool) -> Vec<media::Input> {
    let mut result = Vec::new();
    for (revision, mut samples, held, played, velocity) in [
        (0, vec![4f32, 8., 12., 16.], (55, 31), 61, 100),
        (7, vec![20f32, 40., 60.], (70, 50), 62, 90),
    ] {
        if one_sample && revision == 0 {
            samples.truncate(1);
        }
        let frames = samples.len() as u64;
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|sample| [*sample, *sample * 10.])
            .flat_map(f32::to_le_bytes)
            .collect();
        let audio = upload(store, Format::Audio { channels: 2 }, frames, &bytes, None);
        let mut initial = MidiState::default();
        initial.apply_packet(&[0x90, held.0, held.1], 3).unwrap();
        let midi = upload(
            store,
            Format::Midi,
            frames,
            &encode_midi(
                &[
                    (0, vec![0x90, played, velocity]),
                    (2, vec![0x80, played, 45]),
                ]
                .into_iter()
                .filter(|(frame, _)| *frame < frames)
                .collect::<Vec<_>>(),
            ),
            Some(&initial),
        );
        result.push(media::Input {
            description: media::Description {
                owner: id(),
                revision: Revision(revision),
                frames,
                channels: 2,
                midi: true,
            },
            audio: Some(audio),
            midi: Some(midi),
        });
    }
    result
}
fn initial(
    control: &mut LoopControl<Fx>,
    engine: &mut LoopRuntime<Fx>,
    host: &mut Host,
    export: media::ExportId,
) -> MidiState {
    for channel in 0..16 {
        submit(
            control,
            engine.next_frame().0,
            Operation::MidiInitial { export, channel },
        );
    }
    host.step(engine, 1);
    let mut replies = Vec::new();
    drain(control, &mut replies);
    assert_eq!(replies.len(), 16);
    let mut channels = [ChannelState::default(); 16];
    let mut seen = [false; 16];
    let mut reset = None;
    for (_, reply) in replies {
        let Reply::MidiInitial {
            channel,
            state,
            system_reset_seen,
        } = reply
        else {
            panic!("initial state {reply:?}")
        };
        assert!(!seen[channel as usize]);
        seen[channel as usize] = true;
        channels[channel as usize] = state;
        if let Some(old) = reset {
            assert_eq!(old, system_reset_seen);
        }
        reset = Some(system_reset_seen);
    }
    assert!(seen.into_iter().all(|s| s));
    MidiState::from_channels(channels, reset.unwrap())
}
fn finish(
    control: LoopControl<Fx>,
    engine: LoopRuntime<Fx>,
    imports: &[rt_runtime::ResourceId],
    pages: usize,
) {
    drop(control);
    let mut stopped = engine.into_stopped();
    assert_no_heap(|| {
        stopped.cleanup().unwrap();
        stopped.cleanup().unwrap();
        for resource in imports {
            stopped.resources().remove(*resource).unwrap();
        }
        for _ in 0..1024 {
            stopped.resources().begin_callback();
            stopped.resources().reclaim();
        }
        assert_eq!(stopped.media_usage().references, 0);
        assert_eq!(stopped.resource_usage().free_pages, pages);
        assert_eq!(stopped.resource_usage().free_versions, 32);
        assert_eq!(stopped.resource_usage().free_snapshots, 32);
    });
}
fn exported(
    replies: &[(Sequence, Reply)],
    sequence: Sequence,
) -> (media::ExportId, Cut, media::Export) {
    match replies.iter().find(|(s, _)| *s == sequence).unwrap().1 {
        Reply::Export { id, cut, media } => (id, cut, media),
        other => panic!("export {other:?}"),
    }
}
fn state(replies: &[(Sequence, Reply)], sequence: Sequence) -> Cut {
    match replies.iter().find(|(s, _)| *s == sequence).unwrap().1 {
        Reply::State(cut) => cut,
        other => panic!("state {other:?}"),
    }
}
fn notes_restore(frame: u64, notes: &[(u8, u8)], out: &mut Vec<(u64, Vec<u8>)>) {
    for channel in 0..16 {
        for packet in [
            [0xe0 | channel, 0, 64],
            [0xb0 | channel, 64, 0],
            [0xb0 | channel, 69, 0],
            [0xb0 | channel, 66, 0],
        ] {
            out.push((frame, packet.to_vec()));
        }
        if channel == 0 {
            for &(key, velocity) in notes {
                out.push((frame, vec![0x90, key, velocity]));
            }
        }
    }
}
fn offs(frame: u64, keys: &[u8], out: &mut Vec<(u64, Vec<u8>)>) {
    for &key in keys {
        out.push((frame, vec![0x80, key, 0]));
    }
}
fn replacement_midi() -> Vec<(u64, Vec<u8>)> {
    let mut out = Vec::new();
    notes_restore(0, &[(55, 31)], &mut out);
    out.push((0, vec![0x90, 61, 100]));
    out.push((2, vec![0x80, 61, 45]));
    offs(4, &[55], &mut out);
    notes_restore(4, &[(70, 50)], &mut out);
    out.push((4, vec![0x90, 62, 90]));
    offs(6, &[62, 70], &mut out);
    notes_restore(6, &[(62, 90), (70, 50)], &mut out);
    out.push((7, vec![0x80, 62, 45]));
    offs(8, &[70], &mut out);
    notes_restore(8, &[(70, 50)], &mut out);
    out.push((8, vec![0x90, 62, 90]));
    // Monitoring an already-playing revision continues its MIDI stream.
    out.push((10, vec![0x80, 62, 45]));
    offs(11, &[70], &mut out);
    notes_restore(11, &[(70, 50)], &mut out);
    out.push((11, vec![0x90, 62, 90]));
    offs(12, &[62, 70], &mut out);
    notes_restore(12, &[(62, 90), (70, 50)], &mut out);
    out.push((13, vec![0x80, 62, 45]));
    offs(14, &[70], &mut out);
    notes_restore(14, &[(70, 50)], &mut out);
    out.push((14, vec![0x90, 62, 90]));
    offs(15, &[62, 70], &mut out);
    out
}
#[shoop_test]
fn prepared_replacement_retirement_gain_capture_revisions_and_late_delivery_are_coherent() {
    let expected_midi = replacement_midi();
    for partition in [vec![16], vec![1; 16], vec![2, 5, 1, 4, 4]] {
        let (mut control, mut engine, imports) = setup_with(
            1024,
            true,
            Options {
                imports: true,
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.early = false;
        let replacement = control
            .media()
            .iter()
            .find(|m| m.revision == Revision(7))
            .unwrap()
            .media;
        transition(
            &mut control,
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        submit(
            &mut control,
            4,
            Operation::Select {
                target: id(),
                timing: Timing::At,
                media: replacement,
                preserve_position: false,
            },
        );
        submit(&mut control, 4, Operation::Retire { media: replacement });
        submit(
            &mut control,
            5,
            Operation::Gain {
                target: id(),
                value: -1.,
            },
        );
        transition(
            &mut control,
            6,
            Transition::Seek {
                position: 1,
                retrigger: false,
            },
        );
        submit(
            &mut control,
            8,
            Operation::Gain {
                target: id(),
                value: 0.25,
            },
        );
        transition(
            &mut control,
            9,
            Transition::Record {
                monitor_previous: true,
            },
        );
        let export = submit(
            &mut control,
            12,
            Operation::Export {
                target: id(),
                selection: Selection::Playback,
            },
        );
        transition(
            &mut control,
            12,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        let stale = submit(
            &mut control,
            13,
            Operation::Select {
                target: id(),
                timing: Timing::At,
                media: replacement,
                preserve_position: false,
            },
        );
        transition(&mut control, 15, Transition::Stop);
        let query = submit(&mut control, 12, Operation::State { target: id() });
        let mut packets = Vec::new();
        let expected = [
            2., 4., 6., 8., 10., -20., -20., -30., 2.5, 5., 7.5, 2.5, 0.625, 0.9375, 0.3125, 0.,
        ];
        for frames in partition {
            let at = engine.next_frame().0 as usize;
            host.step(&mut engine, frames);
            for frame in 0..frames {
                assert_eq!(host.output[0][frame], expected[at + frame]);
                assert_eq!(host.output[1][frame], expected[at + frame] * 10.);
            }
            for i in 0..host.midi_out[0].len() {
                let (offset, packet) = host.midi_out[0].get(i).unwrap();
                packets.push(((at + offset as usize) as u64, packet.to_vec()));
            }
        }
        assert_eq!(packets, expected_midi);
        let mut replies = Vec::new();
        drain(&mut control, &mut replies);
        assert_eq!(
            replies.iter().find(|(s, _)| *s == stale).unwrap().1,
            Reply::Failed(Failure::Runtime(Error::InvalidId))
        );
        let cut = state(&replies, query);
        assert_eq!(cut.frame, Frame(12));
        assert_eq!(cut.state.content_revision, Revision(8));
        assert_eq!(cut.state.content_length, 3);
        assert_eq!(cut.state.mode, Mode::Playing);
        assert_eq!(cut.gain, 0.25);
        assert_eq!(cut.balance, 0.);
        let (export, _, media) = exported(&replies, export);
        assert_eq!(media.view.description.revision, Revision(8));
        let late = transition(
            &mut control,
            14,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        host.step(&mut engine, 1);
        assert_eq!(host.output[0][0], 0.625);
        let admission = control.try_completion().unwrap();
        assert_eq!(admission.sequence, late);
        assert_eq!(
            admission.outcome,
            RuntimeOutcome::Scheduled {
                effective_frame: Frame(16),
                late_by: 2
            }
        );
        let execution = control.try_execution().unwrap();
        assert_eq!(execution.requested_frame, Frame(14));
        assert_eq!(execution.delivered_frame, Some(Frame(16)));
        assert_eq!(execution.actual_frame, Frame(16));
        assert_eq!(execution.late_by(), 2);
        let Some(Reply::Transition(completion)) = execution.operation_result else {
            panic!("late transition")
        };
        assert_eq!(completion.requested, Frame(14));
        assert_eq!(completion.effective, Frame(16));
        assert_eq!(completion.outcome, Outcome::Applied { frame: Frame(16) });
        control
            .submit(
                Lane::Recovery,
                Frame(17),
                Operation::Transition {
                    target: id(),
                    timing: Timing::At,
                    transition: Transition::Stop,
                },
            )
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        host.step(&mut engine, 1);
        drain(&mut control, &mut Vec::new());
        let bytes = read_export(&mut control, &mut engine, &mut host, export, false);
        assert_eq!(
            bytes,
            [5f32, 50., 7.5, 75., 2.5, 25.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()
        );
        let bytes = read_export(&mut control, &mut engine, &mut host, export, true);
        let source: Vec<_> = expected_midi
            .iter()
            .filter(|(frame, _)| (9..12).contains(frame))
            .map(|(frame, packet)| (frame - 9, packet.clone()))
            .collect();
        assert_eq!(bytes, encode_midi(&source));
        let initial = initial(&mut control, &mut engine, &mut host, export);
        assert!(initial.channel(0).unwrap().note(62).unwrap().held());
        assert_eq!(initial.channel(0).unwrap().note(62).unwrap().velocity(), 90);
        assert!(initial.channel(0).unwrap().note(70).unwrap().held());
        assert_eq!(initial.channel(0).unwrap().note(70).unwrap().velocity(), 50);
        finish(control, engine, &imports, 512);
    }
}

#[shoop_test]
fn stopped_input_initial_state_and_half_open_prefix_export_survive_immediate_play() {
    for partition in [vec![8], vec![1; 8], vec![1, 2, 1, 4]] {
        let (mut control, mut engine) = setup(0, false);
        let mut host = Host::new();
        transition(
            &mut control,
            1,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let prefix = submit(
            &mut control,
            2,
            Operation::Export {
                target: id(),
                selection: Selection::Recording,
            },
        );
        let full = submit(
            &mut control,
            3,
            Operation::Export {
                target: id(),
                selection: Selection::Playback,
            },
        );
        transition(
            &mut control,
            3,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        transition(&mut control, 7, Transition::Stop);
        let mut packets = Vec::new();
        let expected = [1., 2., 3., 5., 1.5, 1., 1.5, 0.];
        for frames in partition {
            let at = engine.next_frame().0 as usize;
            host.step(&mut engine, frames);
            assert_eq!(&host.output[0][..frames], &expected[at..at + frames]);
            for i in 0..host.midi_out[0].len() {
                let (offset, packet) = host.midi_out[0].get(i).unwrap();
                packets.push(((at + offset as usize) as u64, packet.to_vec()));
            }
        }
        let mut expected = vec![(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])];
        notes_restore(3, &[(60, 100)], &mut expected);
        expected.push((4, vec![0x80, 60, 45]));
        notes_restore(5, &[(60, 100)], &mut expected);
        expected.push((6, vec![0x80, 60, 45]));
        assert_eq!(packets, expected);
        let mut replies = Vec::new();
        drain(&mut control, &mut replies);
        for (sequence, length, events, samples) in [
            (prefix, 1, vec![], vec![2f32, 20.]),
            (
                full,
                2,
                vec![(1, vec![0x80, 60, 45])],
                vec![2f32, 20., 3., 30.],
            ),
        ] {
            let (export, cut, media) = exported(&replies, sequence);
            assert_eq!(media.view.description.frames, length);
            assert_eq!(cut.frame, if length == 1 { Frame(2) } else { Frame(3) });
            assert_eq!(
                read_export(&mut control, &mut engine, &mut host, export, false),
                samples
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                read_export(&mut control, &mut engine, &mut host, export, true),
                encode_midi(&events)
            );
            let initial = initial(&mut control, &mut engine, &mut host, export);
            let mut expected = MidiState::default();
            expected.apply_packet(&[0x90, 60, 100], 3).unwrap();
            assert_eq!(initial, expected);
        }
        finish(control, engine, &[], 512);
    }
}

#[shoop_test]
fn capture_preflight_and_optional_export_refusals_do_not_publish_partial_takes() {
    let (mut control, mut engine, _) = setup_with(
        1024,
        false,
        Options {
            checkpoints: 4,
            ..Options::default()
        },
    );
    let mut host = Host::new();
    let request = transition(
        &mut control,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let query = submit(&mut control, 0, Operation::State { target: id() });
    host.step(&mut engine, 4);
    assert_eq!(&host.output[0][..4], &[1., 2., 3., 4.]);
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert!(matches!(
        replies.iter().find(|(s, _)| *s == request).unwrap().1,
        Reply::Transition(Completion {
            outcome: Outcome::Failed {
                frame: Frame(0),
                reason: shoop_loops::Error::Capacity
            },
            ..
        })
    ));
    let cut = state(&replies, query);
    assert_eq!(cut.state.mode, Mode::Stopped);
    assert_eq!(cut.state.capture_revision, Revision(0));
    assert_eq!(cut.state.last_pulse, None);
    assert_eq!(cut.latest, None);
    assert_eq!(engine.resource_usage().reserved_pages, 0);
    finish(control, engine, &[], 512);

    let (mut control, mut engine, _) = setup_with(
        1024,
        true,
        Options {
            media: 2,
            ..Options::default()
        },
    );
    let mut host = Host::new();
    host.tail = 2.;
    transition(
        &mut control,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let failed = transition(
        &mut control,
        4,
        Transition::Record {
            monitor_previous: true,
        },
    );
    let query = submit(&mut control, 4, Operation::State { target: id() });
    let export = submit(
        &mut control,
        8,
        Operation::Export {
            target: id(),
            selection: Selection::Playback,
        },
    );
    transition(
        &mut control,
        8,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    );
    transition(&mut control, 16, Transition::Stop);
    host.step(&mut engine, 16);
    assert_eq!(
        &host.output[0][..16],
        &[1., 2., 3., 4., 1., 1., 1., 1., 1.5, 2., 2.5, 3., 1.5, 1.5, 1.5, 1.5]
    );
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert!(matches!(
        replies.iter().find(|(s, _)| *s == failed).unwrap().1,
        Reply::Transition(Completion {
            outcome: Outcome::Failed {
                frame: Frame(4),
                reason: shoop_loops::Error::Capacity
            },
            ..
        })
    ));
    let cut = state(&replies, query);
    assert_eq!(cut.state.capture_frames, 4);
    assert_eq!(cut.state.capture_revision, Revision(1));
    assert!(!cut.state.monitor_previous);
    assert!(!cut.state.capture_incomplete);
    assert_eq!(cut.state.last_pulse, Some(Frame(0)));
    host.step(&mut engine, 0);
    assert!(control.try_execution().is_none());
    host.step(&mut engine, 1);
    assert_eq!(host.output[0][0], 1.);
    drain(&mut control, &mut Vec::new());
    let (export, _, media) = exported(&replies, export);
    assert_eq!(media.view.description.frames, 8);
    assert_eq!(media.view.description.revision, Revision(1));
    let expected: Vec<_> = [1f32, 2., 3., 4., 1., 1., 1., 1.]
        .into_iter()
        .flat_map(|x| [x, x * 10.])
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(
        read_export(&mut control, &mut engine, &mut host, export, false),
        expected
    );
    finish(control, engine, &[], 512);

    let (mut control, mut engine) = setup(1024, false);
    let mut host = Host::new();
    transition(
        &mut control,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let failed = submit(
        &mut control,
        5,
        Operation::Export {
            target: id(),
            selection: Selection::Recording,
        },
    );
    let query = submit(&mut control, 5, Operation::State { target: id() });
    let export = submit(
        &mut control,
        8,
        Operation::Export {
            target: id(),
            selection: Selection::Playback,
        },
    );
    transition(
        &mut control,
        8,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    );
    transition(&mut control, 12, Transition::Stop);
    host.step(&mut engine, 13);
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert_eq!(
        replies.iter().find(|(s, _)| *s == failed).unwrap().1,
        Reply::Failed(Failure::Runtime(Error::Budget))
    );
    let cut = state(&replies, query);
    assert_eq!(cut.state.mode, Mode::Recording);
    assert_eq!(cut.state.capture_frames, 5);
    assert_eq!(cut.capture_failure, None);
    assert!(!cut.state.capture_incomplete);
    assert_eq!(engine.resource_usage().free_snapshots, 30);
    let (export, _, media) = exported(&replies, export);
    assert_eq!(media.view.description.frames, 8);
    let expected: Vec<_> = [1f32, 2., 3., 4., 0., 0., 0., 0.]
        .into_iter()
        .flat_map(|x| [x, x * 10.])
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(
        read_export(&mut control, &mut engine, &mut host, export, false),
        expected
    );
    assert_eq!(
        read_export(&mut control, &mut engine, &mut host, export, true),
        encode_midi(&[(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])])
    );
    finish(control, engine, &[], 512);
}

#[shoop_test]
fn running_capture_exhaustion_preserves_independently_predicted_common_prefix_and_live_audio() {
    for partition in [vec![32, 32], vec![1; 64], vec![7, 9, 5, 11, 7, 9, 5, 11]] {
        let (mut control, mut engine, _) = setup_with(
            1024,
            false,
            Options {
                pages: 136,
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.tail = 2.;
        transition(
            &mut control,
            0,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let mut prefix = 0usize;
        let mut failed = false;
        for frames in partition {
            if !failed {
                let used_audio = prefix.div_ceil(8);
                let new_audio = (prefix + frames).div_ceil(8) - used_audio;
                let used_midi = usize::from(prefix > 0);
                if used_audio + used_midi + new_audio + 128 > 136 {
                    failed = true;
                } else {
                    prefix += frames;
                }
            }
            let at = engine.next_frame().0;
            host.step(&mut engine, frames);
            for frame in 0..frames {
                let absolute = at + frame as u64;
                let expected = if absolute < 4 {
                    (absolute + 1) as f32
                } else {
                    1.
                };
                assert_eq!(host.output[0][frame], expected);
                assert_eq!(host.output[1][frame], 10. * expected);
            }
        }
        assert!(failed);
        assert!(prefix >= 32 && prefix < 64);
        let mut replies = Vec::new();
        drain(&mut control, &mut replies);
        let query = submit(&mut control, 64, Operation::State { target: id() });
        let export = submit(
            &mut control,
            64,
            Operation::Export {
                target: id(),
                selection: Selection::Latest,
            },
        );
        host.step(&mut engine, 1);
        drain(&mut control, &mut replies);
        let cut = state(&replies, query);
        assert_eq!(cut.state.mode, Mode::Stopped);
        assert!(cut.state.capture_incomplete);
        assert_eq!(cut.state.capture_frames, prefix as u64);
        assert_eq!(cut.capture_failure, Some(Error::Capacity));
        let (export, _, media) = exported(&replies, export);
        assert_eq!(media.view.description.frames, prefix as u64);
        for source in [media.view.audio, media.view.midi].into_iter().flatten() {
            assert_eq!(source.snapshot.description.frames, prefix as u64);
            assert_eq!(
                source.snapshot.capture.unwrap().incomplete,
                Some(Error::Capacity)
            );
        }
        let expected: Vec<u8> = (0..prefix)
            .flat_map(|frame| {
                let value = if frame < 4 { (frame + 1) as f32 } else { 1. };
                [value, value * 10.]
            })
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(
            read_export(&mut control, &mut engine, &mut host, export, false),
            expected
        );
        assert_eq!(
            read_export(&mut control, &mut engine, &mut host, export, true),
            encode_midi(&[(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])])
        );
        finish(control, engine, &[], 136);
    }
}

#[shoop_test]
fn recovery_and_cancellation_do_not_require_normal_domain_or_runtime_result_credits() {
    let (mut control, mut engine, _) = setup_with(
        0,
        false,
        Options {
            requests: 2,
            self_sync: true,
            ..Options::default()
        },
    );
    let mut host = Host::new();
    let mut sequences = Vec::new();
    for _ in 0..32 {
        sequences.push(submit(
            &mut control,
            0,
            Operation::Transition {
                target: id(),
                timing: Timing::AfterTicks {
                    source: id(),
                    skip: u64::MAX,
                },
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            },
        ));
    }
    host.step(&mut engine, 1);
    let recovery = assert_no_heap(|| {
        control.submit(
            Lane::Recovery,
            Frame(1),
            Operation::Transition {
                target: id(),
                timing: Timing::At,
                transition: Transition::Stop,
            },
        )
    })
    .unwrap_or_else(|e| panic!("{:?}", e.error))
    .sequence();
    host.step(&mut engine, 1);
    assert_eq!(engine.next_frame(), Frame(2));
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert_eq!(replies.len(), 33);
    for sequence in &sequences[..2] {
        assert!(matches!(
            replies.iter().find(|(s, _)| s == sequence).unwrap().1,
            Reply::Transition(Completion {
                outcome: Outcome::Cancelled { frame: Frame(1) },
                ..
            })
        ));
    }
    for sequence in &sequences[2..] {
        assert_eq!(
            replies.iter().find(|(s, _)| s == sequence).unwrap().1,
            Reply::Failed(Failure::Runtime(Error::Capacity))
        );
    }
    assert!(matches!(
        replies.iter().find(|(s, _)| *s == recovery).unwrap().1,
        Reply::Transition(Completion {
            outcome: Outcome::Applied { frame: Frame(1) },
            ..
        })
    ));
    let pending = submit(
        &mut control,
        2,
        Operation::Transition {
            target: id(),
            timing: Timing::AfterTicks {
                source: id(),
                skip: 1,
            },
            transition: Transition::Record {
                monitor_previous: false,
            },
        },
    );
    host.step(&mut engine, 1);
    drain(&mut control, &mut Vec::new());
    control
        .submit(
            Lane::Recovery,
            Frame(3),
            Operation::Cancel { sequence: pending },
        )
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    host.step(&mut engine, 1);
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert!(replies.iter().any(|(s, r)| *s == pending
        && matches!(
            r,
            Reply::Transition(Completion {
                outcome: Outcome::Cancelled { frame: Frame(3) },
                ..
            })
        )));
    assert!(replies.iter().any(|(_, r)| *r
        == Reply::Cancelled {
            sequence: pending,
            delivered: true
        }));
    let queued = transition(
        &mut control,
        20,
        Transition::Record {
            monitor_previous: false,
        },
    );
    host.step(&mut engine, 1);
    drain(&mut control, &mut Vec::new());
    control
        .submit(
            Lane::Recovery,
            Frame(5),
            Operation::Cancel { sequence: queued },
        )
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    host.step(&mut engine, 1);
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert_eq!(
        replies
            .iter()
            .filter(|(_, r)| *r
                == Reply::Cancelled {
                    sequence: queued,
                    delivered: false
                })
            .count(),
        2
    );
    finish(control, engine, &[], 512);
}

#[shoop_test]
fn stopped_extraction_recovers_active_and_partially_staged_faulted_capture_owners() {
    for fault in [false, true] {
        let (mut control, mut engine, _) = setup_with(
            1024,
            false,
            Options {
                input_events: if fault { 1 } else { 512 },
                ..Options::default()
            },
        );
        let mut host = Host::new();
        transition(
            &mut control,
            0,
            Transition::Record {
                monitor_previous: false,
            },
        );
        if fault {
            host.input[0][..3].copy_from_slice(&[2., 4., 6.]);
            host.input[1][..3].copy_from_slice(&[20., 40., 60.]);
            host.midi_in[0].push(0, 3, &[0x90, 60, 100]).unwrap();
            host.midi_in[0].push(2, 3, &[0x80, 60, 45]).unwrap();
            let [left, right] = &mut host.output;
            assert_eq!(
                assert_no_heap(|| engine.process(
                    AudioBlock {
                        frame: Frame(0),
                        frames: 3,
                        inputs: &[&host.input[0][..3], &host.input[1][..3]],
                        outputs: &mut [&mut left[..3], &mut right[..3]]
                    },
                    MidiBlock {
                        inputs: &host.midi_in,
                        outputs: &mut host.midi_out
                    }
                )),
                Err(Error::Capacity)
            );
            assert_eq!(engine.next_frame(), Frame(3));
            assert!(engine.resource_usage().reserved_pages > 0);
        } else {
            host.step(&mut engine, 3);
        }
        finish(control, engine, &[], 512);
    }
}

#[shoop_test]
fn dense_one_sample_media_keeps_all_128_boundaries_with_unread_external_results() {
    for (partition, journal, reversed) in [
        (vec![32; 4], 0, false),
        (vec![1; 128], 1, true),
        ([7, 9, 5, 11].repeat(4), 1024, true),
    ] {
        let (mut control, mut engine, imports) = setup_with(
            journal,
            reversed,
            Options {
                imports: true,
                one_sample: true,
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.early = false;
        transition(
            &mut control,
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        for _ in 0..31 {
            submit(&mut control, 0, Operation::State { target: id() });
        }
        let mut packets = Vec::new();
        for frames in partition {
            let at = engine.next_frame().0;
            host.step(&mut engine, frames);
            assert!(host.output[0][..frames].iter().all(|v| *v == 2.));
            assert!(host.output[1][..frames].iter().all(|v| *v == 20.));
            for i in 0..host.midi_out[0].len() {
                let (offset, packet) = host.midi_out[0].get(i).unwrap();
                packets.push((at + u64::from(offset), packet.to_vec()));
            }
        }
        assert_eq!(engine.next_frame(), Frame(128));
        let mut expected = Vec::new();
        for frame in 0..128 {
            if frame > 0 {
                offs(frame, &[55, 61], &mut expected);
            }
            notes_restore(frame, &[(55, 31)], &mut expected);
            expected.push((frame, vec![0x90, 61, 100]));
        }
        assert_eq!(packets, expected);
        control
            .submit(
                Lane::Recovery,
                Frame(128),
                Operation::Transition {
                    target: id(),
                    timing: Timing::At,
                    transition: Transition::Stop,
                },
            )
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        host.step(&mut engine, 1);
        assert_eq!(host.output[0][0], 0.);
        assert_eq!(host.midi_out[0].len(), 2);
        assert_eq!(host.midi_out[0].get(0).unwrap(), (0, &[0x80, 55, 0][..]));
        assert_eq!(host.midi_out[0].get(1).unwrap(), (0, &[0x80, 61, 0][..]));
        let mut replies = Vec::new();
        drain(&mut control, &mut replies);
        assert_eq!(replies.len(), 33);
        drop(control);
        let mut stopped = engine.into_stopped();
        let log = stopped.observer().as_any().downcast_ref::<Log>().unwrap();
        use shoop_loops::decision::EventKind;
        let mut expected = Vec::new();
        for frame in 0..128 {
            expected.push(Event {
                frame: Frame(frame),
                target: id(),
                kind: if frame == 0 {
                    EventKind::Transition(Transition::Play {
                        position: 0,
                        retrigger: false,
                    })
                } else {
                    EventKind::Wrap
                },
            });
            expected.push(Event {
                frame: Frame(frame),
                target: id(),
                kind: EventKind::Pulse,
            });
        }
        expected.push(Event {
            frame: Frame(128),
            target: id(),
            kind: EventKind::Transition(Transition::Stop),
        });
        let count = expected.len();
        if journal == 0 {
            expected.clear();
        } else if journal == 1 {
            expected.retain(|event| event.kind != EventKind::Pulse);
        }
        assert_eq!(log.events, expected);
        assert_eq!(log.dropped, (count - expected.len()) as u64);
        assert_no_heap(|| {
            stopped.cleanup().unwrap();
            for resource in &imports {
                stopped.resources().remove(*resource).unwrap();
            }
            for _ in 0..1024 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
            assert_eq!(stopped.media_usage().references, 0);
            assert_eq!(stopped.resource_usage().free_pages, 512);
            assert_eq!(stopped.resource_usage().free_snapshots, 32);
            assert_eq!(stopped.resource_usage().free_versions, 32);
        });
    }
}

fn lightweight(
    change: impl FnOnce(&mut Vec<Binding>, &mut Settings, &mut Vec<media::Input>, &mut GraphDescription),
) -> Result<(LoopControl<Fx>, LoopRuntime<Fx>), Failure> {
    let mut cfg = config().limits();
    cfg.audio_inputs = 0;
    cfg.audio_outputs = 0;
    cfg.midi_inputs = 0;
    cfg.midi_outputs = 0;
    cfg.resource_pages = 16;
    cfg.resource_slots = 4;
    let cfg = cfg.prepare().unwrap();
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 5,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: id(),
                kind: LoopKind::Primitive,
                length: 4,
                sync_source: None,
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let mut bindings = vec![Binding {
        target: id(),
        playback: node(0),
        capture: node(1),
        channels: 0,
        midi: false,
        gain: 1.,
        smoothing: Smoothing::None,
        midi_events: 0,
        midi_bytes: 0,
        packet_bytes: 0,
    }];
    let mut settings = Settings {
        planner: PlannerId::from_parts(0, 1).unwrap(),
        normal_requests: 4,
        media_slots: 8,
        export_slots: 2,
    };
    let mut imports = Vec::new();
    let mut graph = GraphDescription {
        nodes: vec![node(0), node(1)],
        connections: Vec::new(),
    };
    change(&mut bindings, &mut settings, &mut imports, &mut graph);
    LoopRuntime::prepare(
        cfg,
        topology,
        ResourceStore::new(cfg).unwrap(),
        settings,
        bindings,
        imports,
        vec![],
        graph,
    )
}
#[shoop_test]
fn facade_node_pair_validation_invalid_operations_and_imported_revision_exhaustion_are_checked() {
    let input = |revision| media::Input {
        description: media::Description {
            owner: id(),
            revision,
            frames: 4,
            channels: 0,
            midi: false,
        },
        audio: None,
        midi: None,
    };
    for (case, error) in [
        (0, Error::InvalidId),
        (1, Error::InvalidId),
        (2, Error::InvalidShape),
        (3, Error::InvalidShape),
        (4, Error::InvalidId),
        (5, Error::InvalidConfig),
        (6, Error::InvalidConfig),
        (7, Error::InvalidShape),
        (8, Error::InvalidShape),
        (9, Error::InvalidShape),
    ] {
        let result = lightweight(|bindings, settings, imports, graph| match case {
            0 => bindings[0].capture = node(2),
            1 => bindings[0].capture = bindings[0].playback,
            2 => bindings.push(bindings[0]),
            3 => bindings[0].gain = f32::NAN,
            4 => bindings.clear(),
            5 => settings.normal_requests = 5,
            6 => bindings[0].midi = true,
            7 => {
                imports.push(input(Revision(0)));
                imports.push(input(Revision(0)));
            }
            8 => {
                let mut value = input(Revision(0));
                value.description.frames = 3;
                imports.push(value);
            }
            9 => graph.nodes.push(node(2)),
            _ => unreachable!(),
        });
        assert!(
            matches!(result,Err(Failure::Runtime(found)) if found==error),
            "case {case}"
        );
    }
    let (mut control, mut engine) =
        lightweight(|_, _, imports, _| imports.push(input(Revision(u64::MAX)))).unwrap();
    let record = transition(
        &mut control,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let bad = [
        (
            Operation::Gain {
                target: id(),
                value: f32::INFINITY,
            },
            Failure::Runtime(Error::InvalidParameter),
        ),
        (
            Operation::Balance {
                target: id(),
                value: 1.5,
            },
            Failure::Runtime(Error::InvalidParameter),
        ),
        (
            Operation::State {
                target: LoopId::from_parts(0, 2).unwrap(),
            },
            Failure::Timeline(shoop_loops::Error::InvalidId),
        ),
        (
            Operation::Transition {
                target: id(),
                timing: Timing::At,
                transition: Transition::Replace {
                    length: 1,
                    revision: Revision(8),
                    preserve_position: false,
                },
            },
            Failure::Runtime(Error::InvalidEvent),
        ),
    ];
    let mut sequences = Vec::new();
    for (operation, error) in bad {
        sequences.push((submit(&mut control, 0, operation), error));
    }
    assert_no_heap(|| {
        engine.process(
            AudioBlock {
                frame: Frame(0),
                frames: 1,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert!(matches!(
        replies.iter().find(|(s, _)| *s == record).unwrap().1,
        Reply::Transition(Completion {
            outcome: Outcome::Failed {
                frame: Frame(0),
                reason: shoop_loops::Error::Exhausted
            },
            ..
        })
    ));
    for (sequence, error) in sequences {
        assert_eq!(
            replies.iter().find(|(s, _)| *s == sequence).unwrap().1,
            Reply::Failed(error)
        );
    }
    let query = submit(&mut control, 1, Operation::State { target: id() });
    assert_no_heap(|| {
        engine.process(
            AudioBlock {
                frame: Frame(1),
                frames: 1,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    drain(&mut control, &mut replies);
    let cut = state(&replies, query);
    assert_eq!(cut.state.mode, Mode::Stopped);
    assert_eq!(cut.state.content_revision, Revision(0));
    assert_eq!(cut.state.capture_revision, Revision(0));
    assert_eq!(cut.state.content_length, 4);
    drop(control);
    let mut stopped = engine.into_stopped();
    assert_no_heap(|| {
        stopped.cleanup().unwrap();
        assert_eq!(stopped.media_usage().references, 0);
        assert_eq!(stopped.resource_usage().free_pages, 16);
        assert_eq!(stopped.resource_usage().free_versions, 4);
        assert_eq!(stopped.resource_usage().free_snapshots, 4);
    });
}

#[shoop_test]
fn integrated_restart_smoothing_and_gain_follow_decisions_not_host_cuts() {
    for partition in [vec![10], vec![1; 10], vec![2, 1, 4, 3]] {
        let (mut control, mut engine, imports) = setup_with(
            0,
            false,
            Options {
                imports: true,
                smoothing: Smoothing::LinearRestart { frames: 3 },
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.early = false;
        transition(
            &mut control,
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        transition(
            &mut control,
            3,
            Transition::Seek {
                position: 0,
                retrigger: false,
            },
        );
        submit(
            &mut control,
            4,
            Operation::Gain {
                target: id(),
                value: -0.5,
            },
        );
        transition(&mut control, 9, Transition::Stop);
        let mut expected = [[0.; 10]; 2];
        for channel in 0..2 {
            let mut position = 0;
            let mut anchor = 0.;
            let mut last = 0.;
            let mut elapsed = 3;
            for frame in 0..10 {
                if frame == 9 {
                    break;
                }
                if frame == 0 || frame == 3 || position == 4 {
                    position = 0;
                    anchor = last;
                    elapsed = 0;
                }
                let raw = (position + 1) as f32
                    * 4.
                    * if channel == 0 { 1. } else { 10. }
                    * if frame >= 4 { -0.5 } else { 1. };
                elapsed = (elapsed + 1).min(3);
                let weight = elapsed as f32 / 3.;
                last = anchor * (1. - weight) + raw * weight;
                expected[channel][frame] = last * 0.5;
                position += 1;
            }
        }
        for frames in partition {
            let at = engine.next_frame().0 as usize;
            host.step(&mut engine, frames);
            for channel in 0..2 {
                for frame in 0..frames {
                    assert!(
                        (host.output[channel][frame] - expected[channel][at + frame]).abs() < 1e-5
                    );
                }
            }
        }
        finish(control, engine, &imports, 512);
    }
}

#[shoop_test]
fn stereo_balance_follows_decisions_and_is_reported_in_the_cut() {
    for partition in [vec![10], vec![1; 10], vec![2, 1, 4, 3]] {
        let (mut control, mut engine, imports) = setup_with(
            0,
            false,
            Options {
                imports: true,
                smoothing: Smoothing::LinearRestart { frames: 3 },
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.early = false;
        transition(
            &mut control,
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        transition(
            &mut control,
            3,
            Transition::Seek {
                position: 0,
                retrigger: false,
            },
        );
        submit(
            &mut control,
            4,
            Operation::Balance {
                target: id(),
                value: -0.5,
            },
        );
        transition(&mut control, 9, Transition::Stop);
        let mut expected = [[0.; 10]; 2];
        for channel in 0..2 {
            let mut position = 0;
            let mut anchor = 0.;
            let mut last = 0.;
            let mut elapsed = 3;
            for frame in 0..10 {
                if frame == 9 {
                    break;
                }
                if frame == 0 || frame == 3 || position == 4 {
                    position = 0;
                    anchor = last;
                    elapsed = 0;
                }
                let raw = (position + 1) as f32
                    * 4.
                    * if channel == 0 { 1. } else { 10. }
                    * if channel == 1 && frame >= 4 { 0.5 } else { 1. };
                elapsed = (elapsed + 1).min(3);
                let weight = elapsed as f32 / 3.;
                last = anchor * (1. - weight) + raw * weight;
                expected[channel][frame] = last * 0.5;
                position += 1;
            }
        }
        for frames in partition {
            let at = engine.next_frame().0 as usize;
            host.step(&mut engine, frames);
            for channel in 0..2 {
                for frame in 0..frames {
                    assert!(
                        (host.output[channel][frame] - expected[channel][at + frame]).abs() < 1e-5
                    );
                }
            }
        }
        let query = submit(
            &mut control,
            engine.next_frame().0,
            Operation::State { target: id() },
        );
        host.step(&mut engine, 1);
        let mut replies = Vec::new();
        drain(&mut control, &mut replies);
        assert_eq!(state(&replies, query).balance, -0.5);
        finish(control, engine, &imports, 512);
    }
}

#[shoop_test]
fn clear_stops_playback_selects_empty_content_and_releases_references() {
    for partition in [vec![14], vec![1; 14], vec![3, 2, 6, 3]] {
        let (mut control, mut engine, imports) = setup_with(
            0,
            false,
            Options {
                imports: true,
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.early = false;
        transition(
            &mut control,
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        let export = submit(
            &mut control,
            1,
            Operation::Export {
                target: id(),
                selection: Selection::Playback,
            },
        );
        let playing = submit(&mut control, 3, Operation::Clear { target: id() });
        let cleared = submit(&mut control, 3, Operation::State { target: id() });
        let empty = transition(
            &mut control,
            5,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        transition(
            &mut control,
            6,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let recording = submit(&mut control, 8, Operation::Clear { target: id() });
        transition(&mut control, 9, Transition::Stop);
        let recorded = submit(&mut control, 9, Operation::State { target: id() });
        let stopped = submit(&mut control, 10, Operation::Clear { target: id() });
        let after = submit(&mut control, 10, Operation::State { target: id() });
        let expected = [2., 4., 6., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.];
        for frames in partition {
            let at = engine.next_frame().0 as usize;
            host.step(&mut engine, frames);
            for frame in 0..frames {
                assert_eq!(host.output[0][frame], expected[at + frame]);
                assert_eq!(host.output[1][frame], expected[at + frame] * 10.);
            }
        }
        let mut replies = Vec::new();
        drain(&mut control, &mut replies);
        let outcome = |sequence| match replies.iter().find(|(s, _)| *s == sequence).unwrap().1 {
            Reply::Transition(completion) => completion.outcome,
            other => panic!("transition {other:?}"),
        };
        assert_eq!(outcome(playing), Outcome::Applied { frame: Frame(3) });
        assert_eq!(
            outcome(empty),
            Outcome::Failed {
                frame: Frame(5),
                reason: shoop_loops::Error::EmptyContent
            }
        );
        assert_eq!(
            outcome(recording),
            Outcome::Failed {
                frame: Frame(8),
                reason: shoop_loops::Error::InvalidState
            }
        );
        assert_eq!(outcome(stopped), Outcome::Applied { frame: Frame(10) });
        let cut = state(&replies, cleared);
        assert_eq!(cut.state.mode, Mode::Stopped);
        assert_eq!(
            (cut.state.content_length, cut.state.content_revision),
            (0, Revision(8))
        );
        assert!(cut.playback.is_some());
        assert_eq!(cut.latest, None);
        let cut = state(&replies, recorded);
        assert_eq!(
            (cut.state.content_length, cut.state.content_revision),
            (3, Revision(9))
        );
        assert!(cut.latest.is_some());
        let cut = state(&replies, after);
        assert_eq!(
            (cut.state.mode, cut.state.content_length),
            (Mode::Stopped, 0)
        );
        assert_eq!(cut.state.content_revision, Revision(10));
        assert_eq!(cut.latest, None);
        let (export, _, media) = exported(&replies, export);
        assert_eq!(media.view.description.frames, 4);
        let audio = read_export(&mut control, &mut engine, &mut host, export, false);
        let samples: Vec<f32> = audio
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(samples, [4., 40., 8., 80., 12., 120., 16., 160.]);
        submit(
            &mut control,
            engine.next_frame().0,
            Operation::Release { export },
        );
        host.step(&mut engine, 1);
        drain(&mut control, &mut replies);
        finish(control, engine, &imports, 512);
    }
}

#[shoop_test]
fn smoothing_operations_apply_from_the_next_restart_and_are_reported() {
    for partition in [vec![10], vec![1; 10], vec![2, 1, 4, 3]] {
        let (mut control, mut engine, imports) = setup_with(
            0,
            false,
            Options {
                imports: true,
                smoothing: Smoothing::LinearRestart { frames: 3 },
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.early = false;
        transition(
            &mut control,
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        submit(
            &mut control,
            2,
            Operation::Smoothing {
                target: id(),
                smoothing: Smoothing::LinearRestart { frames: 2 },
            },
        );
        let stale = submit(
            &mut control,
            2,
            Operation::Smoothing {
                target: LoopId::from_parts(0, 2).unwrap(),
                smoothing: Smoothing::None,
            },
        );
        transition(
            &mut control,
            5,
            Transition::Seek {
                position: 0,
                retrigger: false,
            },
        );
        let query = submit(&mut control, 5, Operation::State { target: id() });
        transition(&mut control, 9, Transition::Stop);
        let mut expected = [[0.; 10]; 2];
        for channel in 0..2 {
            let (mut position, mut anchor, mut last) = (0, 0., 0.);
            let (mut elapsed, mut length) = (3, 3);
            for frame in 0..9 {
                if frame == 0 || frame == 5 || position == 4 {
                    position = 0;
                    anchor = last;
                    elapsed = 0;
                    length = if frame == 0 { 3 } else { 2 };
                }
                let raw = (position + 1) as f32 * 4. * if channel == 0 { 1. } else { 10. };
                elapsed = (elapsed + 1).min(length);
                let weight = elapsed as f32 / length as f32;
                last = anchor * (1. - weight) + raw * weight;
                expected[channel][frame] = last * 0.5;
                position += 1;
            }
        }
        for frames in partition {
            let at = engine.next_frame().0 as usize;
            host.step(&mut engine, frames);
            for channel in 0..2 {
                for frame in 0..frames {
                    assert!(
                        (host.output[channel][frame] - expected[channel][at + frame]).abs() < 1e-5
                    );
                }
            }
        }
        let mut replies = Vec::new();
        drain(&mut control, &mut replies);
        assert_eq!(
            state(&replies, query).smoothing,
            Smoothing::LinearRestart { frames: 2 }
        );
        assert_eq!(
            replies.iter().find(|(s, _)| *s == stale).unwrap().1,
            Reply::Failed(Failure::Runtime(Error::InvalidId))
        );
        finish(control, engine, &imports, 512);
    }
}
