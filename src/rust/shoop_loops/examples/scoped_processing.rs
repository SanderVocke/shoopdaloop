#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    planner::ConsumerAccess,
    processor::{Cadence, ProcessBlock, Processor},
    resource::*,
    schedule::Action,
    transport::Lane,
    Config, Error, Frame, NodeId, PlannerId, Revision,
};
use shoop_loops::{
    audio::Smoothing,
    media,
    runtime::{
        callback::{self, CallbackNode, CallbackPlanner},
        Binding, Settings,
    },
    *,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[path = "../tests/support/scoped_runtime.rs"]
mod fixture;
use fixture::*;

fn main() {
    let (mut control, mut runtime, calls) = setup_bus();
    assert!(control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(0),
                action: Action::Planner {
                    planner: planner(),
                    operation: shoop_loops::runtime::Operation::Transition {
                        target: id(),
                        timing: Timing::At,
                        transition: Transition::Play {
                            position: 0,
                            retrigger: false
                        }
                    }
                },
                payload: Box::new([])
            }
        )
        .is_ok());
    let mut left = [0.0; 17];
    let mut right = [0.0; 17];
    let mut midi = [MidiBuffer::new(4096, 16384).unwrap()];
    support::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 17,
                inputs: &[],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(
        left.to_vec(),
        (0..17)
            .map(|frame| 3.0 * (frame % 4 + 1) as f32)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        right.to_vec(),
        (0..17)
            .map(|frame| 30.0 * (frame % 4 + 1) as f32)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        (0..midi[0].len())
            .map(|i| {
                let (at, data) = midi[0].get(i).unwrap();
                (at, data.to_vec())
            })
            .collect::<Vec<_>>(),
        [
            (1, vec![0xf8]),
            (2, vec![0x90, 72, 91]),
            (5, vec![0xf8]),
            (9, vec![0xf8]),
            (9, vec![0x80, 72, 64]),
            (13, vec![0xf8])
        ]
    );
    assert_eq!(calls.snapshot(), [(0, 17)]);
    println!("bus: frames=17 local_intervals={} fx_calls={:?} verified_audio_samples=34 midi_events={} heap_operations=0", runtime.diagnostics().subranges, calls.snapshot(), midi[0].len());
    for (origin, partitions, length) in [
        (0, vec![17], 4),
        (0, vec![3, 4, 1, 9], 4),
        ((1u64 << 32) + 37, vec![17], 4),
        (0, vec![32], 1),
    ] {
        let (mut control, mut runtime, calls) = setup_details(32, origin, length);
        play(&mut control);
        command(
            &mut control,
            callback::Operation::Capture {
                target: id(),
                start: Frame(origin + 5),
                end: Frame(origin + 15),
            },
        );
        let mut samples = 0;
        let mut midi_events = 0;
        for frames in partitions {
            let at = runtime.next_frame();
            let (left, right, midi) = render(&mut runtime, frames).unwrap();
            assert_eq!(
                left,
                (at.0 - origin..at.0 - origin + frames as u64)
                    .map(|frame| 3.0 * (frame % length + 1) as f32)
                    .collect::<Vec<_>>()
            );
            assert_eq!(right, left.iter().map(|x| x * 10.0).collect::<Vec<_>>());
            let expected_midi: Vec<_> = (at.0 - origin..at.0 - origin + frames as u64)
                .flat_map(|frame| {
                    let mut events = Vec::new();
                    if frame % length == u64::from(length > 1) {
                        events.push((origin + frame, vec![0xf8]));
                    }
                    if frame == 2 {
                        events.push((origin + frame, vec![0x90, 72, 91]));
                    }
                    if frame == 9 {
                        events.push((origin + frame, vec![0x80, 72, 64]));
                    }
                    events
                })
                .collect();
            assert_eq!(midi, expected_midi);
            samples += frames * 2;
            midi_events += midi.len();
        }
        let captured = std::iter::from_fn(|| control.try_execution())
            .find_map(|reply| match reply.operation_result {
                Some(callback::Reply::Captured { media, .. }) => Some(media),
                _ => None,
            })
            .unwrap();
        let (mut planner, mut resources) = runtime.into_planner_and_resources();
        let media = planner.ledger().view(captured).unwrap();
        let source = media.audio.unwrap();
        let mut cursor = resources.render_io().cursor_at(source.index, 0).unwrap();
        let mut bytes = [0; 80];
        assert_eq!(
            resources.render_io().read(&mut cursor, &mut bytes).unwrap(),
            bytes.len()
        );
        let actual: Vec<_> = bytes
            .chunks_exact(4)
            .map(|x| f32::from_le_bytes(x.try_into().unwrap()))
            .collect();
        let expected: Vec<_> = (5..15)
            .flat_map(|frame| {
                let x = 3.0 * (frame % length + 1) as f32;
                [x, x * 10.0]
            })
            .collect();
        assert_eq!(actual, expected);
        let checkpoint = resources
            .render_io()
            .midi_seek(media.midi.unwrap().index, 0)
            .unwrap();
        assert_eq!(
            checkpoint
                .state
                .channel(0)
                .unwrap()
                .note(72)
                .unwrap()
                .velocity(),
            91
        );
        let mut cursor = checkpoint.cursor;
        let mut captured_events = Vec::new();
        while let Some(event) = resources.render_io().midi_next(&mut cursor).unwrap() {
            let mut bytes = vec![0; event.byte_len()];
            resources.render_io().midi_read(event, &mut bytes).unwrap();
            captured_events.push((event.frame(), bytes));
        }
        let expected_events: Vec<_> = (5..15)
            .flat_map(|frame| {
                let mut events = Vec::new();
                if frame % length == u64::from(length > 1) {
                    events.push((frame - 5, vec![0xf8]));
                }
                if frame == 9 {
                    events.push((frame - 5, vec![0x80, 72, 64]));
                }
                events
            })
            .collect();
        assert_eq!(captured_events, expected_events);
        println!("dry_wet: origin={origin} loop_frames={length} callback_work={:?} fx_calls={:?} plan_bytes={} verified_output_samples={samples} verified_capture_samples=20 captured_frames=10 midi_events={midi_events} verified_capture_midi_events={} checkpoint_note=72:91 heap_operations=0", planner.work_counts(), calls.snapshot(), planner.plan_storage_bytes(), captured_events.len());
        planner.release_all(&mut resources).unwrap();
    }
    let (mut control, mut runtime, _) = setup_two_owners();
    play(&mut control);
    let other = LoopId::from_parts(1, 1).unwrap();
    command(
        &mut control,
        callback::Operation::Transition {
            target: other,
            at: Frame(1),
            timing: Timing::AfterTicks {
                source: id(),
                skip: 0,
            },
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
    );
    let (audio, _, _) = render(&mut runtime, 17).unwrap();
    assert_eq!(
        audio,
        (0..17)
            .map(|frame| 3.0 * (frame % 4 + 1) as f32 * if frame < 4 { 1.0 } else { 1.5 })
            .collect::<Vec<_>>()
    );
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    assert_eq!(planner.state(other).unwrap().position, 1);
    println!(
        "shared_sync: owners=2 callback_work={:?} verified_output_samples=34",
        planner.work_counts()
    );
    planner.release_all(&mut resources).unwrap();
    let (mut control, mut runtime, _) = setup(32);
    play(&mut control);
    schedule(
        &mut control,
        Frame(3),
        Action::Parameter {
            node: node(1),
            parameter: 0,
            value: 1.0,
        },
        &[],
    );
    render(&mut runtime, 17).unwrap();
    assert!(std::iter::from_fn(|| control.try_execution())
        .any(|reply| reply.result == Err(Error::InvalidFrame) && reply.actual_frame == Frame(3)));
    let (_, resources, _, _) = parts_length(1);
    assert_eq!(resources.config().limits().max_frames, 32);
    println!("scope_actions: interior_callback_parameter=InvalidFrame effective_frame=3");
}
