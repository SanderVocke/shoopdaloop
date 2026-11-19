pub use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{BufferId, JobProcessor},
    midi::*,
    pipeline::Pipeline,
    records::Identity,
    state::{Instruction, PlannedTransition, Program},
    storage::MediaStore,
    transitions::{
        LoopKind, LoopSpec, Operation, PlanDescription, TransitionLimits, TransitionPlan,
    },
};
use std::{vec, vec::Vec};
pub fn msg(bytes: &[u8]) -> Message {
    Message::managed(bytes).unwrap()
}
pub fn event(offset: u32, bytes: &[u8]) -> InputEvent {
    InputEvent {
        input: 0,
        offset,
        message: msg(bytes),
    }
}
pub fn id(slot: usize) -> Identity {
    Identity {
        slot: slot as u32,
        generation: 1,
    }
}
pub fn limits(loops: u16) -> SessionLimits {
    SessionLimits {
        store: StoreLimits {
            block_events: 8,
            blocks: 128,
            max_events: 128,
            views: 64,
            snapshots: 64,
            retirements: 32,
        },
        loops,
        inputs: 1,
        destinations: 2,
        input_events_per_quantum: 16,
        history_frames: 128,
        maintenance_steps: 8,
    }
}
pub fn logical(q: u32, limits: SessionLimits) -> rt_fpga_compiler::LogicalStateProgram {
    use rt_fpga_compiler::{LogicalStateProgram, Operation as Op, Resources};
    LogicalStateProgram {
        quantum: q,
        chunk_frames: 1,
        sync_pass: (0..limits.loops)
            .map(|slot| rt_fpga_compiler::Identity {
                slot: u32::from(slot),
                generation: 1,
            })
            .collect(),
        work_pass: vec![
            Op::Input {
                port: 0,
                destination: 0,
            },
            Op::Output { source: 0, port: 0 },
        ],
        initial_state: vec![],
        state_bindings: vec![],
        loops: (0..limits.loops)
            .map(|slot| rt_fpga_compiler::Loop {
                id: rt_fpga_compiler::Identity {
                    slot: u32::from(slot),
                    generation: 1,
                },
                length: 0,
                sync_source: None,
                repeat_sync: false,
            })
            .collect(),
        bindings: (0..limits.loops)
            .map(|slot| rt_fpga_compiler::LoopBinding {
                id: rt_fpga_compiler::Identity {
                    slot: u32::from(slot),
                    generation: 1,
                },
                gain: None,
                view: None,
                meter: None,
            })
            .collect(),
        jobs_per_quantum: 2,
        resources: Resources {
            queue_capacity: 2,
            buffers: 1,
            history_slots: 0,
            input_ports: 1,
            output_ports: 1,
            events: 512,
        },
    }
    .with_midi(
        &(0..limits.loops)
            .map(|context| rt_fpga_compiler::MidiBinding {
                loop_id: rt_fpga_compiler::Identity {
                    slot: u32::from(context),
                    generation: 1,
                },
                context,
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
pub struct Fixture {
    pub pipeline: Pipeline,
    pub jobs: JobProcessor,
    pub audio: MediaStore,
    pub midi: MidiSession,
    pub q: u32,
    pub emitted: Vec<OutputEvent>,
    pub input: Vec<Fixed>,
    pub output: Vec<Fixed>,
    pub sequence: u64,
    pub capacity: usize,
}
impl Fixture {
    pub fn new(q: u32, loops: u16) -> Self {
        Self::with_limits(q, limits(loops))
    }
    pub fn with_limits(q: u32, limits: SessionLimits) -> Self {
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: q,
                loops: (0..usize::from(limits.loops))
                    .map(|i| LoopSpec {
                        id: id(i),
                        kind: LoopKind::Primitive,
                        length: 0,
                        sync_source: None,
                        repeat_sync: false,
                        timeline: None,
                    })
                    .collect(),
            },
            TransitionLimits {
                loop_slots: u32::from(limits.loops),
                events: 512,
            },
        )
        .unwrap();
        use rt_fpga_compiler::StateProgramTarget;
        let logical = logical(q, limits);
        let compiled = rt_fpga_reference::target::RustTarget::new(&[])
            .build(&logical)
            .unwrap();
        let program = compiled.program;
        let capacity = program.jobs_per_quantum;
        let mut midi = MidiSession::prepare(limits).unwrap();
        for i in 0..limits.loops {
            midi.bind(
                i,
                Binding {
                    loop_id: id(usize::from(i)),
                    input: if limits.inputs == limits.loops { i } else { 0 },
                    destination: i / 16,
                    channels: 1 << (i % 16),
                },
            )
            .unwrap();
        }
        Self {
            pipeline: Pipeline::prepare(plan, program, vec![], capacity).unwrap(),
            jobs: JobProcessor::new(q as usize, 1),
            audio: MediaStore::new(1).unwrap(),
            midi,
            q,
            emitted: Vec::with_capacity(65536),
            input: vec![Fixed(17); q as usize * 2],
            output: vec![Fixed::ZERO; q as usize * 2],
            sequence: 0,
            capacity: usize::MAX,
        }
    }
    pub fn command(&mut self, slot: usize, operation: Operation) {
        self.sequence += 1;
        self.pipeline
            .write_plans(
                id(slot),
                [
                    Some(PlannedTransition {
                        sequence: self.sequence,
                        operation,
                        on_pulse: false,
                        effective: self.pipeline.state().frame(),
                        remaining_ticks: 0,
                    }),
                    None,
                ],
            )
            .unwrap();
    }
    pub fn try_run_frames(
        &mut self,
        events: &[InputEvent],
        frames: usize,
    ) -> Result<SessionReport, ProcessError> {
        let cap = self.capacity;
        let emitted = &mut self.emitted;
        let mut accepted = 0;
        let result = self.pipeline.process_midi_session(
            &mut self.jobs,
            &mut self.audio,
            &[&self.input[..frames]],
            &mut [&mut self.output[..frames]],
            frames,
            &mut self.midi,
            events,
            &mut |e| {
                if accepted == cap {
                    return Err(Error::Capacity);
                }
                accepted += 1;
                emitted.push(e);
                Ok(())
            },
        );
        result
    }
    pub fn run_frames(&mut self, events: &[InputEvent], frames: usize) -> SessionReport {
        let result = self.try_run_frames(events, frames).unwrap();
        assert_eq!(&self.output[..frames], &self.input[..frames]);
        result
    }
    pub fn run(&mut self, events: &[InputEvent]) -> SessionReport {
        self.run_frames(events, self.q as usize)
    }
}

#[derive(Debug)]
pub struct SessionProfile {
    pub events_per_loop: u32,
    pub prepared_bytes: usize,
    pub capture_ns: u128,
    pub playback_ns: u128,
    pub quantum_p50_ns: u128,
    pub quantum_p99_ns: u128,
    pub quantum_max_ns: u128,
    pub grab_ns: u128,
    pub grab_replay_steps: u64,
    pub grab_comparison_work: u64,
    pub grab_storage_work: u64,
    pub work: SessionWork,
    pub final_usage: StoreUsage,
}

pub fn dense_session_workload(
    events_per_loop: u32,
    mut hot: impl FnMut(&mut dyn FnMut()),
) -> SessionProfile {
    use std::time::Instant;
    assert!((1..=8192).contains(&events_per_loop));
    let mut l = SessionLimits::session();
    l.inputs = 128;
    l.input_events_per_quantum = 1;
    let mut f = Fixture::with_limits(32, l);
    for input in 1..128 {
        f.midi.set_history_retention(input, 0).unwrap();
    }
    let prepared_bytes = f.midi.prepared_bytes();
    let duration = 30 * 48_000u64;
    let quanta = duration / u64::from(f.q);
    let mut input = [InputEvent {
        input: 0,
        offset: 0,
        message: msg(&[0xb0, 2, 1]),
    }; 128];
    let mut times = Vec::with_capacity((quanta * 2) as usize);
    let mut next = 0u32;
    for slot in 0..128 {
        f.command(slot, Operation::Record);
    }
    let capture = Instant::now();
    for _ in 0..quanta {
        let base = f.pipeline.state().frame();
        let end = base + u64::from(f.q);
        let count = if next < events_per_loop
            && u64::from(next) * duration / u64::from(events_per_loop) < end
        {
            let offset = (u64::from(next) * duration / u64::from(events_per_loop) - base) as u32;
            for (i, event) in input.iter_mut().enumerate() {
                *event = InputEvent {
                    input: i as u16,
                    offset,
                    message: msg(&[0xb0 | (i % 16) as u8, 2, 1 + (next % 127) as u8]),
                };
            }
            next += 1;
            128
        } else {
            0
        };
        hot(&mut || {
            let now = Instant::now();
            assert_eq!(f.run(&input[..count]).error, None);
            times.push(now.elapsed().as_nanos());
        });
    }
    let capture_ns = capture.elapsed().as_nanos();
    assert_eq!(next, events_per_loop);
    for slot in 0..128 {
        f.command(slot, Operation::Stop);
    }
    hot(&mut || {
        assert_eq!(f.run(&[]).error, None);
    });
    for slot in 0..128 {
        let d = f
            .midi
            .store()
            .describe(f.midi.content(slot).unwrap().unwrap())
            .unwrap();
        assert_eq!((d.frames, d.events), (duration, events_per_loop));
    }
    let original = f.midi.content(0).unwrap().unwrap();
    let export = f.midi.export(original).unwrap();
    let initial = *f
        .midi
        .store()
        .state(f.midi.store().describe(export).unwrap().initial)
        .unwrap();
    let imported = f.midi.begin_import(&initial).unwrap();
    let mut transfer = [Record {
        frame: 0,
        order: 0,
        message: msg(&[0xb0, 2, 1]),
    }; 32];
    let mut exported = 0u32;
    let mut imported_ready = false;
    for slot in 0..128 {
        f.command(
            slot,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    let mut played = [0u32; 128];
    let playback = Instant::now();
    for _ in 0..quanta {
        let base = f.pipeline.state().frame();
        // Continue one history source while playing all captured sequences.
        let relative = base - (duration + u64::from(f.q));
        let next_history = relative.div_ceil(480) * 480;
        let e = InputEvent {
            input: 0,
            offset: (next_history - relative) as u32,
            message: msg(&[0xb0, 3, (next_history / 480 % 128) as u8]),
        };
        let events = if e.offset < f.q {
            core::slice::from_ref(&e)
        } else {
            &[]
        };
        hot(&mut || {
            f.emitted.clear();
            let now = Instant::now();
            assert_eq!(f.run(events).error, None);
            times.push(now.elapsed().as_nanos());
            for output in &f.emitted {
                let bytes = output.message.as_bytes();
                if bytes[0] & 0xf0 == 0xb0 && bytes[1] == 2 {
                    let slot = usize::from(output.destination) * 16 + usize::from(bytes[0] & 15);
                    let expected = u64::from(played[slot]) * duration / u64::from(events_per_loop);
                    assert_eq!(
                        base + u64::from(output.offset),
                        duration + u64::from(f.q) + expected
                    );
                    assert_eq!(bytes[2], 1 + (played[slot] % 127) as u8);
                    played[slot] += 1;
                }
            }
            if exported < events_per_loop {
                let n = f
                    .midi
                    .export_events(export, exported, &mut transfer)
                    .unwrap();
                for (i, r) in transfer[..n].iter().enumerate() {
                    assert_eq!(
                        r.frame,
                        u64::from(exported + i as u32) * duration / u64::from(events_per_loop)
                    );
                }
                f.midi.import_events(imported, &transfer[..n]).unwrap();
                exported += n as u32;
            } else if !imported_ready {
                imported_ready = f.midi.finish_import(imported, duration, 256).unwrap();
            }
        });
    }
    let playback_ns = playback.elapsed().as_nanos();
    assert!(played.iter().all(|&n| n == events_per_loop));
    assert!(imported_ready);
    assert!(!f.midi.history_bounds(0).unwrap().2);
    let now = f.pipeline.state().frame();
    let mut first = 2900u32;
    first -= (events_per_loop + first + 1) % 32;
    let start = duration + u64::from(f.q) + u64::from(first) * 480 - 17;
    let end = start + 48_000;
    let slots = core::array::from_fn::<u16, 128, _>(|i| i as u16);
    let before = f.midi.work();
    let old_usage = f.midi.store().usage();
    let mut grab_ns = 0;
    hot(&mut || {
        let timer = Instant::now();
        f.pipeline
            .grab_midi(&mut f.midi, 0, start, end, &slots)
            .unwrap();
        grab_ns = timer.elapsed().as_nanos();
    });
    assert_eq!(f.pipeline.state().frame(), now);
    let after = f.midi.work();
    let grabbed = f.midi.content(0).unwrap().unwrap();
    assert!(
        slots
            .iter()
            .all(|&slot| f.midi.content(slot).unwrap() == Some(grabbed))
    );
    assert!(f.midi.store().usage().snapshots <= old_usage.snapshots + 1);
    let d = f.midi.store().describe(grabbed).unwrap();
    assert_eq!(d.frames, 48_000);
    let relative = start - (duration + u64::from(f.q));
    let expected = (relative.div_ceil(480) - 1) % 128;
    assert_eq!(
        f.midi
            .store()
            .state(d.initial)
            .unwrap()
            .channel(0)
            .unwrap()
            .controller(3),
        Some(expected as u8)
    );
    assert_eq!(after.replay_steps - before.replay_steps, 31);
    hot(&mut || {
        f.emitted.clear();
        assert_eq!(f.run(&[]).error, None);
        assert!(f.emitted.is_empty());
    });
    assert_eq!(
        f.midi.export_events(export, 0, &mut transfer).unwrap(),
        32.min(events_per_loop as usize)
    );
    f.pipeline
        .replace_midi(&mut f.midi, &[0], imported)
        .unwrap();
    f.pipeline.duplicate_midi(&mut f.midi, 0, 1).unwrap();
    hot(&mut || {
        assert_eq!(f.run(&[]).error, None);
    });
    f.pipeline.clear_midi(&mut f.midi, &slots).unwrap();
    f.midi.release(imported).unwrap();
    f.midi.release(export).unwrap();
    hot(&mut || {
        assert_eq!(f.run(&[]).error, None);
        while f.midi.maintenance(64) != 0 {}
    });
    assert_eq!(f.midi.store().usage().views, 128);
    assert!(
        slots
            .iter()
            .all(|&slot| f.midi.content(slot).unwrap().is_none())
    );
    times.sort_unstable();
    SessionProfile {
        events_per_loop,
        prepared_bytes,
        capture_ns,
        playback_ns,
        quantum_p50_ns: times[times.len() / 2],
        quantum_p99_ns: times[times.len() * 99 / 100],
        quantum_max_ns: *times.last().unwrap(),
        grab_ns,
        grab_replay_steps: after.replay_steps - before.replay_steps,
        grab_comparison_work: after.comparison_work - before.comparison_work,
        grab_storage_work: after.storage_work - before.storage_work,
        work: f.midi.work(),
        final_usage: f.midi.store().usage(),
    }
}

#[derive(Debug)]
pub struct GrabProfile {
    pub samples: usize,
    pub min_ns: u128,
    pub p50_ns: u128,
    pub p99_ns: u128,
    pub max_ns: u128,
    pub max_replay_steps: u64,
    pub max_comparison_work: u64,
    pub max_storage_work: u64,
}

pub fn repeated_grab_workload(
    samples: usize,
    mut hot: impl FnMut(&mut dyn FnMut()),
) -> GrabProfile {
    use std::time::Instant;
    assert!(samples > 0);
    let mut f = Fixture::with_limits(32, SessionLimits::session());
    let input = core::array::from_fn::<InputEvent, 256, _>(|i| {
        event((i / 8) as u32, &[0xb0, 1, 1 + (i % 127) as u8])
    });
    for _ in 0..32 {
        assert_eq!(f.run(&input).error, None);
    }
    let extra = core::array::from_fn::<InputEvent, 31, _>(|i| {
        event(
            i as u32,
            &[
                0xb0,
                if i < 5 { i as u8 + 1 } else { i as u8 + 2 },
                70 + i as u8,
            ],
        )
    });
    f.run(&extra);
    f.run(&[]);
    let slots = core::array::from_fn::<u16, 128, _>(|i| i as u16);
    let mut times = Vec::with_capacity(samples);
    let mut max_replay_steps = 0;
    let mut max_comparison_work = 0;
    let mut max_storage_work = 0;
    for _ in 0..samples {
        while f.midi.maintenance(64) != 0 {}
        for slot in 0..128 {
            if slot % 32 == 0 {
                f.run(&[]);
            }
            let view = f.midi.history_range(0, 0, 1024).unwrap();
            assert_eq!(f.midi.store().describe(view).unwrap().events, 8192);
            f.pipeline.replace_midi(&mut f.midi, &[slot], view).unwrap();
            f.midi.release(view).unwrap();
        }
        f.run(&[]);
        let frame = f.pipeline.state().frame();
        let before = f.midi.work();
        hot(&mut || {
            let timer = Instant::now();
            f.pipeline
                .grab_midi(&mut f.midi, 0, 1055, 1087, &slots)
                .unwrap();
            times.push(timer.elapsed().as_nanos());
        });
        let after = f.midi.work();
        assert_eq!(f.pipeline.state().frame(), frame);
        assert_eq!(after.replay_steps - before.replay_steps, 31);
        max_replay_steps = max_replay_steps.max(after.replay_steps - before.replay_steps);
        max_comparison_work =
            max_comparison_work.max(after.comparison_work - before.comparison_work);
        max_storage_work = max_storage_work.max(after.storage_work - before.storage_work);
        let view = f.midi.content(0).unwrap().unwrap();
        assert!(
            slots
                .iter()
                .all(|&slot| f.midi.content(slot).unwrap() == Some(view))
        );
    }
    times.sort_unstable();
    GrabProfile {
        samples,
        min_ns: times[0],
        p50_ns: times[samples / 2],
        p99_ns: times[samples * 99 / 100],
        max_ns: times[samples - 1],
        max_replay_steps,
        max_comparison_work,
        max_storage_work,
    }
}
