#[path = "../../rt_fpga_compiler/tests/support/programs.rs"]
mod programs;
use rt_fpga_compiler::{MidiOperation as M, StateProgramTarget};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{Job, JobProcessor},
    midi::{
        self, BufferLimits, InputEvent, Message, MidiMemory, OutputEvent, SequenceId, StoreLimits,
        TakeStatus,
    },
    pipeline::Pipeline,
    state::ModePlan,
    storage::MediaStore,
    target::RustTarget,
    transitions::{LoopId, Mode},
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 1,
    }
}
fn message(bytes: &[u8]) -> Message {
    Message::parse(bytes).unwrap()
}
fn event(input: u16, offset: u32, bytes: &[u8]) -> InputEvent {
    InputEvent {
        input,
        offset,
        message: message(bytes),
    }
}
fn limits() -> StoreLimits {
    StoreLimits {
        block_events: 8,
        blocks: 512,
        max_events: 4096,
        views: 32,
        retirements: 32,
    }
}

struct Fixture {
    pipeline: Pipeline,
    jobs: JobProcessor,
    audio: MediaStore,
    midi: MidiMemory,
    emitted: Vec<OutputEvent>,
    mixed: bool,
    q: usize,
    input: Vec<Fixed>,
    out0: Vec<Fixed>,
    out1: Vec<Fixed>,
}
impl Fixture {
    fn new(q: u32, mixed: bool) -> Self {
        let mut logical = programs::mono_loops(q, 2);
        if !mixed {
            logical.work_pass.clear();
            logical.jobs_per_quantum = 0;
        }
        logical.chunk_frames = q;
        logical.resources.buffers = logical.resources.buffers.max(8);
        logical.resources.input_ports = 2;
        let operations = [
            M::Playback {
                loop_id: logical.loops[0].id,
                sequence: 0,
                cursor: 0,
                channels: u16::MAX,
                destination: 2,
            },
            M::Playback {
                loop_id: logical.loops[1].id,
                sequence: 1,
                cursor: 1,
                channels: u16::MAX,
                destination: 3,
            },
            M::Input {
                port: 0,
                destination: 0,
            },
            M::Input {
                port: 1,
                destination: 1,
            },
            M::Capture {
                loop_id: logical.loops[0].id,
                sequence: 0,
                source: 0,
            },
            M::Capture {
                loop_id: logical.loops[1].id,
                sequence: 1,
                source: 1,
            },
            M::Merge {
                left: 3,
                right: 2,
                destination: 4,
            },
            M::Output { source: 4, port: 0 },
        ];
        let logical = logical.with_midi_data(&operations).unwrap();
        let mut audio = MediaStore::new(q as usize).unwrap();
        let views: Vec<_> = (0..2)
            .map(|_| audio.create_recording().unwrap().0 as u64)
            .collect();
        audio.reserve_chunk_capacity(64).unwrap();
        for view in &views {
            audio
                .reserve_view_capacity(
                    rt_fpga_reference::storage::ViewId(*view as usize),
                    q as usize * 128,
                )
                .unwrap();
        }
        let compiled = RustTarget::new(&views).build(&logical).unwrap();
        let pipeline = Pipeline::prepare(
            compiled.plan,
            compiled.program,
            compiled.memory,
            logical.jobs_per_quantum as usize,
        )
        .unwrap();
        let mut jobs = JobProcessor::new(q as usize, logical.resources.buffers as usize);
        jobs.prepare_midi(BufferLimits {
            buffers: 8,
            cursors: 2,
            scratch_events: 16384,
            input_ports: 2,
            output_ports: 2,
        })
        .unwrap();
        Self {
            pipeline,
            jobs,
            audio,
            midi: MidiMemory::prepare(limits(), 2).unwrap(),
            emitted: Vec::with_capacity(8192),
            mixed,
            q: q as usize,
            input: vec![Fixed::from_f32(0.25); q as usize],
            out0: vec![Fixed::ZERO; q as usize],
            out1: vec![Fixed::ZERO; q as usize],
        }
    }
    fn mode(&mut self, slot: u32, mode: Mode) {
        self.pipeline
            .write_mode_plan(
                id(slot),
                ModePlan {
                    now: Some(mode),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    fn tick(&mut self, events: &[InputEvent]) -> midi::Work {
        self.emitted.clear();
        let Self {
            pipeline,
            jobs,
            audio,
            midi,
            emitted,
            input,
            out0,
            out1,
            q,
            mixed,
        } = self;
        let inputs: &[&[Fixed]] = if *mixed { &[input] } else { &[] };
        pipeline
            .process_with_midi(
                jobs,
                audio,
                inputs,
                &mut [out0, out1],
                *q,
                midi,
                events,
                &mut |event| {
                    emitted.push(event);
                    Ok(())
                },
            )
            .unwrap()
    }
    fn install(&mut self, slot: u16, records: &[(u64, Message)], frames: u64) -> midi::ViewId {
        let view = self.midi.store.writer().unwrap();
        for &(frame, message) in records {
            self.midi.store.append(view, frame, message).unwrap();
        }
        self.midi.store.finish(view, frames).unwrap();
        self.pipeline
            .replace_midi_view(
                &mut self.midi,
                id(u32::from(slot)),
                SequenceId(slot),
                Some(view),
            )
            .unwrap();
        view
    }
}

#[shoop_test]
fn compiled_sp_and_common_jp_record_merge_and_repeat_more_than_256_events() {
    for mixed in [false, true] {
        let mut f = Fixture::new(32, mixed);
        let mut events = Vec::new();
        for i in 0..600 {
            events.push(event(1, 2, &[0xb1, 1, (i % 128) as u8]));
            events.push(event(0, 2, &[0xb0, 1, (i % 128) as u8]));
        }
        f.pipeline.trace_jobs(true);
        f.mode(0, Mode::Recording);
        f.mode(1, Mode::Recording);
        assert_no_heap(|| {
            let work = f.tick(&events);
            assert_eq!(work.captured_events, 1200);
        });
        assert!(f.emitted.is_empty());
        for slot in 0..2 {
            assert_eq!(
                f.midi
                    .store
                    .describe(f.midi.view(SequenceId(slot)).unwrap().unwrap())
                    .unwrap()
                    .events,
                600
            );
        }
        f.mode(0, Mode::Stopped);
        f.mode(1, Mode::Stopped);
        f.tick(&[]);
        assert_eq!(
            f.midi
                .store
                .describe(f.midi.view(SequenceId(0)).unwrap().unwrap())
                .unwrap()
                .status,
            TakeStatus::Complete
        );
        assert_eq!(
            f.pipeline
                .emitted_jobs()
                .iter()
                .filter(|j| matches!(j, Job::MidiFinishCapture { .. }))
                .count(),
            2
        );
        f.tick(&[]);
        assert!(!f
            .pipeline
            .emitted_jobs()
            .iter()
            .any(|j| matches!(j, Job::MidiFinishCapture { .. })));
        f.mode(0, Mode::Playing);
        f.mode(1, Mode::Playing);
        assert_no_heap(|| {
            let work = f.tick(&[]);
            assert_eq!(work.output_events, 1200);
            assert_eq!(work.merged_events, 1200);
        });
        assert_eq!(f.emitted.len(), 1200);
        assert!(f.emitted.iter().all(|e| e.offset == 2));
        assert!(f.emitted[..600]
            .iter()
            .all(|e| e.message.channel() == Some(0)));
        assert!(f.emitted[600..]
            .iter()
            .all(|e| e.message.channel() == Some(1)));
        if mixed {
            assert_eq!(f.out0, f.input);
            assert_eq!(f.out1, f.input);
            assert!(f
                .pipeline
                .emitted_jobs()
                .iter()
                .any(|j| matches!(j, Job::ReadView { .. })));
        }
        let expected = f.emitted.clone();
        f.tick(&[]);
        assert_eq!(f.emitted, expected);
        f.mode(0, Mode::Stopped);
        f.mode(1, Mode::Stopped);
        f.tick(&[]);
        assert!(f.emitted.is_empty());
    }
}

#[shoop_test]
fn future_event_lookahead_avoids_reloading_quiet_sequences() {
    let mut f = Fixture::new(32, false);
    f.install(0, &[(64, message(&[0x90, 60, 80]))], 96);
    f.mode(0, Mode::Playing);
    assert_eq!(f.tick(&[]).record_reads, 1);
    assert!(f.emitted.is_empty());
    assert_eq!(f.tick(&[]).record_reads, 0);
    assert!(f.emitted.is_empty());
    assert_eq!(f.tick(&[]).record_reads, 0);
    assert_eq!(f.emitted.len(), 1);
    assert_eq!(f.emitted[0].offset, 0);
    assert_eq!(f.tick(&[]).record_reads, 1);
    assert!(f.emitted.is_empty());
}

#[shoop_test]
fn pedal_and_note_packets_repeat_verbatim_and_stop_emits_no_corrections() {
    for q in [1, 16, 32, 64, 128] {
        let mut f = Fixture::new(q, false);
        let a = message(&[0x90, 60, 80]);
        let b = message(&[0xb0, 64, 127]);
        f.install(0, &[(0, a), (u64::from(q) - 1, b)], u64::from(q));
        f.mode(0, Mode::Playing);
        f.tick(&[]);
        assert_eq!(
            f.emitted.iter().map(|e| e.message).collect::<Vec<_>>(),
            vec![a, b]
        );
        f.tick(&[]);
        assert_eq!(f.emitted.len(), 2);
        f.mode(0, Mode::Stopped);
        f.tick(&[]);
        assert!(f.emitted.is_empty());
    }
}

#[shoop_test]
fn shared_range_views_cross_blocks_and_survive_retakes() {
    let mut f = Fixture::new(32, false);
    let records: Vec<_> = (0..40).map(|n| (n, message(&[0xb0, 1, n as u8]))).collect();
    let original = f.install(0, &records, 64);
    let range = f.midi.store.range(original, 5, 37).unwrap();
    f.pipeline
        .replace_midi_view(&mut f.midi, id(1), SequenceId(1), Some(range))
        .unwrap();
    assert_eq!(
        f.midi
            .store
            .read(range, 0)
            .unwrap()
            .unwrap()
            .message
            .as_bytes(),
        &[0xb0, 1, 5]
    );
    assert_eq!(f.midi.store.read(range, 31).unwrap().unwrap().frame, 31);
    f.mode(0, Mode::Recording);
    f.tick(&[event(0, 0, &[0x90, 70, 100])]);
    f.mode(0, Mode::Stopped);
    f.tick(&[]);
    f.mode(1, Mode::Playing);
    f.tick(&[]);
    assert_eq!(f.emitted.len(), 32);
    assert_eq!(
        f.midi
            .store
            .read(original, 39)
            .unwrap()
            .unwrap()
            .message
            .as_bytes(),
        &[0xb0, 1, 39]
    );
}

#[shoop_test]
fn all_short_controller_messages_pass_without_state_interpretation() {
    for cc in [6, 38, 64, 66, 69, 96, 97, 98, 99, 100, 101, 120, 121, 123] {
        assert!(Message::parse(&[0xb0, cc, 0]).is_ok());
    }
    for data in [&[0xf8][..], &[0xfa], &[0xf1, 1], &[0xf2, 0, 1], &[0xff]] {
        assert!(Message::parse(data).is_ok());
    }
    assert_eq!(
        Message::parse(&[0x90, 128, 0]),
        Err(midi::Error::InvalidEvent)
    );
    assert_eq!(Message::parse(&[0xf0]), Err(midi::Error::Unsupported));
}

#[shoop_test]
fn capture_to_play_in_one_pass_preserves_positions_across_partial_blocks() {
    let mut f = Fixture::new(32, false);
    f.mode(0, Mode::Recording);
    let batches = [
        vec![event(0, 2, &[0x90, 60, 80]); 5],
        (0..10).map(|n| event(0, n, &[0xb0, 1, n as u8])).collect(),
        vec![event(0, 31, &[0x80, 60, 0]); 12],
    ];
    for batch in &batches {
        f.tick(batch);
    }
    f.mode(0, Mode::Playing);
    for batch in &batches {
        f.tick(&[]);
        assert_eq!(
            f.emitted
                .iter()
                .map(|e| (e.offset, e.message))
                .collect::<Vec<_>>(),
            batch
                .iter()
                .map(|e| (e.offset, e.message))
                .collect::<Vec<_>>()
        );
    }
    let view = f.midi.view(SequenceId(0)).unwrap().unwrap();
    let description = f.midi.store.describe(view).unwrap();
    assert_eq!(
        (description.events, description.frames, description.status),
        (27, 96, TakeStatus::Complete)
    );
    assert_eq!(f.midi.store.read(view, 5).unwrap().unwrap().frame, 32);
    assert_eq!(f.midi.store.read(view, 15).unwrap().unwrap().frame, 95);
}

#[shoop_test]
fn malformed_input_is_rejected_before_sp_advances_and_can_be_corrected() {
    let mut f = Fixture::new(32, false);
    let bad = [event(0, 31, &[0x90, 60, 80]), event(0, 0, &[0x80, 60, 0])];
    let error = f.pipeline.process_with_midi(
        &mut f.jobs,
        &mut f.audio,
        &[],
        &mut [&mut f.out0, &mut f.out1],
        32,
        &mut f.midi,
        &bad,
        &mut |_| Ok(()),
    );
    assert_eq!(
        error,
        Err(rt_fpga_reference::pipeline::PipelineError::Jobs(
            rt_fpga_reference::Error::Midi(midi::Error::InvalidEvent)
        ))
    );
    assert_eq!(f.pipeline.state().frame(), 0);
    assert_eq!(f.pipeline.fault(), None);
    f.tick(&[]);
    assert_eq!(f.pipeline.state().frame(), 32);
}

#[shoop_test]
fn output_refusal_is_a_pipeline_fault_without_automatic_midi_packets() {
    let mut f = Fixture::new(32, false);
    let note = message(&[0x90, 60, 80]);
    f.install(0, &[(0, note), (1, note)], 32);
    f.mode(0, Mode::Playing);
    let mut accepted = Vec::new();
    let error = f.pipeline.process_with_midi(
        &mut f.jobs,
        &mut f.audio,
        &[],
        &mut [&mut f.out0, &mut f.out1],
        32,
        &mut f.midi,
        &[],
        &mut |e| {
            if accepted.is_empty() {
                accepted.push(e);
                Ok(())
            } else {
                Err(midi::Error::OutputRefused)
            }
        },
    );
    assert_eq!(
        error,
        Err(rt_fpga_reference::pipeline::PipelineError::Jobs(
            rt_fpga_reference::Error::Midi(midi::Error::OutputRefused)
        ))
    );
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].message, note);
    assert!(f.pipeline.fault().is_some());
    let before = f.pipeline.state().frame();
    let again = f.pipeline.process_with_midi(
        &mut f.jobs,
        &mut f.audio,
        &[],
        &mut [&mut f.out0, &mut f.out1],
        32,
        &mut f.midi,
        &[],
        &mut |_| panic!("output after fault"),
    );
    assert_eq!(again, error);
    assert_eq!(f.pipeline.state().frame(), before);
}

#[shoop_test]
fn multi_quantum_callbacks_preserve_offsets_without_heap_work_or_message_quotas() {
    let mut f = Fixture::new(32, false);
    let events: Vec<_> = [0, 31, 32, 63, 64, 95]
        .into_iter()
        .flat_map(|offset| (0..300).map(move |n| event(0, offset, &[0xb0, 1, (n % 128) as u8])))
        .collect();
    let mut out0 = vec![Fixed::ZERO; 96];
    let mut out1 = out0.clone();
    f.mode(0, Mode::Recording);
    assert_no_heap(|| {
        let work = f
            .pipeline
            .process_with_midi(
                &mut f.jobs,
                &mut f.audio,
                &[],
                &mut [&mut out0, &mut out1],
                96,
                &mut f.midi,
                &events,
                &mut |_| Ok(()),
            )
            .unwrap();
        assert_eq!(work.captured_events, 1800);
    });
    f.mode(0, Mode::Playing);
    let mut output = Vec::with_capacity(1800);
    assert_no_heap(|| {
        let work = f
            .pipeline
            .process_with_midi(
                &mut f.jobs,
                &mut f.audio,
                &[],
                &mut [&mut out0, &mut out1],
                96,
                &mut f.midi,
                &[],
                &mut |e| {
                    output.push(e);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(work.output_events, 1800);
    });
    assert_eq!(
        output
            .iter()
            .map(|e| (e.offset, e.message))
            .collect::<Vec<_>>(),
        events
            .iter()
            .map(|e| (e.offset, e.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(f.pipeline.state().frame(), 192);
}

#[shoop_test]
fn seeks_without_retrigger_are_positioned_by_sp_and_rewind_the_data_cursor() {
    use rt_fpga_reference::{state::PlannedTransition, transitions::Operation};
    let mut f = Fixture::new(32, false);
    let records: Vec<_> = (0..4)
        .map(|i| (i * 32 + 2, message(&[0xb0, 1, i as u8])))
        .collect();
    f.install(0, &records, 128);
    f.mode(0, Mode::Playing);
    for _ in 0..3 {
        f.tick(&[]);
    }
    f.pipeline.trace_jobs(true);
    f.pipeline
        .write_plans(
            id(0),
            [
                Some(PlannedTransition {
                    sequence: 1,
                    effective: f.pipeline.state().frame(),
                    on_pulse: false,
                    remaining_ticks: 0,
                    operation: Operation::Seek {
                        position: 32,
                        retrigger: false,
                    },
                }),
                None,
            ],
        )
        .unwrap();
    f.tick(&[]);
    assert_eq!(
        f.emitted
            .iter()
            .map(|e| (e.offset, e.message))
            .collect::<Vec<_>>(),
        vec![(2, records[1].1)]
    );
    assert!(f.pipeline.emitted_jobs().iter().any(|job| matches!(
        job,
        Job::MidiReadView {
            position: 32,
            restart: true,
            ..
        }
    )));
    f.tick(&[]);
    assert_eq!(f.emitted[0].message, records[2].1);
}

#[shoop_test]
fn prepared_scratch_exhaustion_faults_explicitly_without_heap_growth() {
    let mut f = Fixture::new(32, false);
    f.jobs
        .prepare_midi(BufferLimits {
            buffers: 8,
            cursors: 2,
            scratch_events: 4,
            input_ports: 2,
            output_ports: 2,
        })
        .unwrap();
    let input = vec![event(0, 0, &[0xb0, 1, 20]); 5];
    f.mode(0, Mode::Recording);
    assert_no_heap(|| {
        let result = f.pipeline.process_with_midi(
            &mut f.jobs,
            &mut f.audio,
            &[],
            &mut [&mut f.out0, &mut f.out1],
            32,
            &mut f.midi,
            &input,
            &mut |_| panic!("output on failed capture"),
        );
        assert_eq!(
            result,
            Err(rt_fpga_reference::pipeline::PipelineError::Jobs(
                rt_fpga_reference::Error::Midi(midi::Error::Capacity)
            ))
        );
    });
    assert!(f.pipeline.fault().is_some());
    let view = f.midi.view(SequenceId(0)).unwrap().unwrap();
    assert_eq!(f.midi.store.describe(view).unwrap().events, 0);
}

#[shoop_test]
fn content_publication_checks_loop_binding_and_is_atomic_on_rejection() {
    let mut f = Fixture::new(32, false);
    let a = f.install(0, &[(2, message(&[0x90, 60, 80]))], 32);
    let b = f.midi.store.writer().unwrap();
    f.midi.store.finish(b, 64).unwrap();
    assert_eq!(
        f.pipeline
            .replace_midi_view(&mut f.midi, id(0), SequenceId(1), Some(b)),
        Err(midi::Error::InvalidState)
    );
    assert_eq!(f.midi.view(SequenceId(1)).unwrap(), None);
    assert_eq!(
        f.pipeline
            .state()
            .loop_state(id(0))
            .unwrap()
            .state
            .content_length,
        32
    );
    f.mode(0, Mode::Playing);
    assert_eq!(
        f.pipeline
            .replace_midi_view(&mut f.midi, id(0), SequenceId(0), Some(b)),
        Err(midi::Error::Busy)
    );
    assert_eq!(f.midi.view(SequenceId(0)).unwrap(), Some(a));
    f.tick(&[]);
    assert_eq!(f.emitted.len(), 1);
}
