use super::*;
use shoop_loops::windows::{ChannelUpdate, Edit, Windows};

fn edit(control: &mut LoopControl<Fx>, at: u64, revision: u64, change: Edit) -> Sequence {
    submit(
        control,
        at,
        Operation::EditWindows {
            target: id(),
            expected_revision: revision,
            expected_content: Revision(0),
            edit: change,
        },
    )
}

#[shoop_test]
fn manual_timing_keeps_absolute_position_raw_media_and_independent_channel_windows() {
    let (mut control, mut engine, _) = setup_with(
        32,
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
    host.step(&mut engine, 3);
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    let edited = edit(
        &mut control,
        3,
        0,
        Edit {
            length: Some(8),
            ..Edit::default()
        },
    );
    host.step(&mut engine, 2);
    assert_eq!(&host.output[0][..2], &[8., 0.]);
    drain(&mut control, &mut replies);
    let Reply::Timing(cut) = replies
        .iter()
        .find(|(sequence, _)| *sequence == edited)
        .unwrap()
        .1
    else {
        panic!("missing timing result")
    };
    assert_eq!((cut.state.position, cut.state.content_length), (3, 8));
    let exported = submit(
        &mut control,
        5,
        Operation::Export {
            target: id(),
            selection: Selection::Playback,
        },
    );
    let changed = Edit {
        count: 2,
        channels: core::array::from_fn(|index| match index {
            0 => ChannelUpdate {
                channel: 0,
                start_offset: Some(1),
                ..Default::default()
            },
            1 => ChannelUpdate {
                channel: 1,
                start_offset: Some(-1),
                ..Default::default()
            },
            _ => ChannelUpdate::default(),
        }),
        ..Edit::default()
    };
    edit(&mut control, 5, 1, changed);
    edit(
        &mut control,
        5,
        2,
        Edit {
            length: Some(2),
            ..Edit::default()
        },
    );
    host.step(&mut engine, 4);
    assert_eq!(&host.output[0][..4], &[6., 4., 6., 4.]);
    assert_eq!(&host.output[1][..4], &[20., 0., 20., 0.]);
    drain(&mut control, &mut replies);
    let Reply::Export {
        id: export,
        media,
        cut,
    } = replies
        .iter()
        .find(|(sequence, _)| *sequence == exported)
        .unwrap()
        .1
    else {
        panic!("missing export")
    };
    assert_eq!(media.view.description.frames, 4);
    assert_eq!(cut.windows.audio[0].start_offset, 1);
    assert_eq!(cut.state.content_length, 2);
    submit(&mut control, 9, Operation::Release { export });
    host.step(&mut engine, 1);
}

#[shoop_test]
fn channel_batch_refusal_and_armed_recording_length_refusal_do_not_partially_apply() {
    let (mut control, mut engine, _) = setup_with(
        32,
        false,
        Options {
            imports: true,
            source: true,
            ..Options::default()
        },
    );
    let mut host = Host::new();
    host.early = false;
    let source = LoopId::from_parts(1, 1).unwrap();
    submit(
        &mut control,
        0,
        Operation::Transition {
            target: source,
            timing: Timing::At,
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
    );
    submit(
        &mut control,
        0,
        Operation::Transition {
            target: id(),
            timing: Timing::AfterTicks { source, skip: 2 },
            transition: Transition::Record {
                monitor_previous: false,
            },
        },
    );
    let invalid = edit(
        &mut control,
        0,
        0,
        Edit {
            length: Some(8),
            start_offset: Some(2),
            preplay: Some(3),
            ..Edit::default()
        },
    );
    let valid = edit(
        &mut control,
        0,
        0,
        Edit {
            start_offset: Some(1),
            preplay: Some(1),
            ..Edit::default()
        },
    );
    host.step(&mut engine, 1);
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert!(matches!(
        replies.iter().find(|(s, _)| *s == invalid).unwrap().1,
        Reply::Failed(_)
    ));
    let Reply::Timing(cut) = replies.iter().find(|(s, _)| *s == valid).unwrap().1 else {
        panic!("missing edit")
    };
    assert_eq!(cut.state.content_length, 4);
    assert_eq!(cut.timing_revision, 1);
    assert_eq!(cut.windows.audio[0].start_offset, 1);
    let mut channels = [ChannelUpdate::default(); shoop_loops::windows::MAX_CHANNEL_UPDATES];
    channels[0] = ChannelUpdate {
        channel: 0,
        start_offset: Some(2),
        ..ChannelUpdate::default()
    };
    channels[1] = channels[0];
    let duplicate = edit(
        &mut control,
        1,
        1,
        Edit {
            count: 2,
            channels,
            ..Edit::default()
        },
    );
    let stale = edit(
        &mut control,
        1,
        0,
        Edit {
            start_offset: Some(9),
            ..Edit::default()
        },
    );
    let query = submit(&mut control, 1, Operation::State { target: id() });
    host.step(&mut engine, 1);
    drain(&mut control, &mut replies);
    for sequence in [duplicate, stale] {
        assert!(matches!(
            replies.iter().find(|(s, _)| *s == sequence).unwrap().1,
            Reply::Failed(_)
        ));
    }
    let Reply::State(after) = replies.iter().find(|(s, _)| *s == query).unwrap().1 else {
        panic!("missing state")
    };
    assert_eq!(
        (after.windows, after.timing_revision),
        (cut.windows, cut.timing_revision)
    );
}

#[shoop_test]
fn preplay_is_audible_while_stopped_and_pickup_notes_continue_across_start() {
    let (mut control, mut engine, _) = setup_with(
        32,
        false,
        Options {
            imports: true,
            source: true,
            ..Options::default()
        },
    );
    let mut host = Host::new();
    host.early = false;
    let source = LoopId::from_parts(1, 1).unwrap();
    edit(
        &mut control,
        0,
        0,
        Edit {
            start_offset: Some(2),
            preplay: Some(2),
            ..Edit::default()
        },
    );
    submit(
        &mut control,
        0,
        Operation::Transition {
            target: source,
            timing: Timing::At,
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
    );
    host.step(&mut engine, 1);
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    submit(
        &mut control,
        1,
        Operation::Transition {
            target: id(),
            timing: Timing::AfterTicks { source, skip: 0 },
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
    );
    host.step(&mut engine, 5);
    assert_eq!(&host.output[0][..5], &[0.; 5]);
    let stopped = submit(&mut control, 6, Operation::State { target: id() });
    host.step(&mut engine, 2);
    assert_eq!(&host.output[0][..2], &[2., 4.]);
    assert!((0..host.midi_out[0].len())
        .filter_map(|index| host.midi_out[0].get(index))
        .any(|(_, packet)| packet == [0x90, 61, 100]));
    drain(&mut control, &mut replies);
    let Reply::State(cut) = replies.iter().find(|(s, _)| *s == stopped).unwrap().1 else {
        panic!("missing state")
    };
    assert_eq!(cut.state.mode, Mode::Stopped);
    host.step(&mut engine, 2);
    assert_eq!(&host.output[0][..2], &[6., 8.]);
    assert!(!(0..host.midi_out[0].len())
        .filter_map(|index| host.midi_out[0].get(index))
        .any(|(_, packet)| packet.first() == Some(&0x90)));
    assert!((0..host.midi_out[0].len())
        .filter_map(|index| host.midi_out[0].get(index))
        .any(|(frame, packet)| frame == 0 && packet == [0x80, 61, 45]));
    let played = submit(&mut control, 10, Operation::State { target: id() });
    host.step(&mut engine, 1);
    drain(&mut control, &mut replies);
    let Reply::State(cut) = replies.iter().find(|(s, _)| *s == played).unwrap().1 else {
        panic!("missing state")
    };
    assert_eq!(cut.state.mode, Mode::Playing);
    assert_ne!(cut.windows, Windows::default());
}

#[shoop_test]
fn cancelled_superseded_and_invalidated_pickups_release_notes_without_starting() {
    for action in 0..3 {
        let (mut control, mut engine, _) = setup_with(
            32,
            false,
            Options {
                imports: true,
                source: true,
                ..Options::default()
            },
        );
        let mut host = Host::new();
        host.early = false;
        let source = LoopId::from_parts(1, 1).unwrap();
        edit(
            &mut control,
            0,
            0,
            Edit {
                start_offset: Some(2),
                preplay: Some(2),
                ..Default::default()
            },
        );
        submit(
            &mut control,
            0,
            Operation::Transition {
                target: source,
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            },
        );
        host.step(&mut engine, 1);
        let planned = submit(
            &mut control,
            1,
            Operation::Transition {
                target: id(),
                timing: Timing::AfterTicks { source, skip: 0 },
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            },
        );
        host.step(&mut engine, 6);
        assert!(host.output[0][5] != 0.);
        assert!((0..host.midi_out[0].len())
            .filter_map(|index| host.midi_out[0].get(index))
            .any(|(_, packet)| packet == [0x90, 61, 100]));
        match action {
            0 => {
                submit(&mut control, 7, Operation::Cancel { sequence: planned });
            }
            1 => {
                submit(
                    &mut control,
                    7,
                    Operation::Transition {
                        target: source,
                        timing: Timing::At,
                        transition: Transition::Stop,
                    },
                );
            }
            _ => {
                submit(&mut control, 7, Operation::Cancel { sequence: planned });
                submit(
                    &mut control,
                    7,
                    Operation::Transition {
                        target: id(),
                        timing: Timing::AfterTicks { source, skip: 1 },
                        transition: Transition::Play {
                            position: 0,
                            retrigger: false,
                        },
                    },
                );
            }
        }
        host.step(&mut engine, 1);
        assert_eq!(host.output[0][0], 0.);
        assert!(
            (0..host.midi_out[0].len())
                .filter_map(|index| host.midi_out[0].get(index))
                .any(|(_, packet)| packet == [0x80, 61, 0]),
            "pickup note leaked after action {action}"
        );
        host.step(&mut engine, 4);
        assert_eq!(&host.output[0][..4], &[0.; 4]);
    }
}

#[shoop_test(no_wasm = "native callback stack regression")]
fn indexed_window_validation_fits_a_native_callback_stack() {
    let (mut control, engine, _) = setup_with(
        32,
        false,
        Options {
            imports: true,
            ..Default::default()
        },
    );
    let request = edit(
        &mut control,
        0,
        0,
        Edit {
            start_offset: Some(1),
            preplay: Some(1),
            ..Default::default()
        },
    );
    let engine = Box::new(engine);
    let engine = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || {
            let mut engine = engine;
            let mut host = Host::new();
            host.early = false;
            host.step(&mut engine, 1);
            engine
        })
        .unwrap()
        .join()
        .unwrap();
    let mut replies = Vec::new();
    drain(&mut control, &mut replies);
    assert!(replies
        .iter()
        .any(|(sequence, reply)| *sequence == request && matches!(reply, Reply::Timing(_))));
    drop(engine);
}
