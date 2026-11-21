use rt_fpga_compiler::{
    CompileContext, LogicalStateProgram, MidiOperation as M, StateProgramTarget, shoop_graph::*,
};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::JobProcessor,
    midi::{BufferLimits, Message, MidiMemory, OutputEvent, Record, SequenceId, StoreLimits},
    pipeline::Pipeline,
    state::PlannedTransition,
    storage::MediaStore,
    target::RustTarget,
    transitions::{LoopId, Operation},
};

pub const QUANTUM: usize = 32;
pub const SAMPLE_RATE: usize = 48_000;
pub const DESTINATIONS: usize = 8;
const EVENT_PERIOD: usize = SAMPLE_RATE / 100;

pub fn id(slot: usize) -> LoopId {
    LoopId {
        slot: slot as u32,
        generation: 1,
    }
}
pub fn sample(slot: usize, channel: usize, frame: usize) -> f32 {
    let value = ((frame + channel * 17) % 64) as i32 - 32;
    (value * (slot as i32 + 1)) as f32 / 1_048_576.0
}
pub fn records(slot: usize, frames: usize) -> Vec<Record> {
    (slot % 15 * QUANTUM..frames)
        .step_by(EVENT_PERIOD)
        .enumerate()
        .map(|(order, frame)| Record {
            frame: frame as u64,
            order: order as u32,
            message: Message::parse(&[
                0xb0 | (slot % 8) as u8,
                1,
                (1 + (order + slot) % 127) as u8,
            ])
            .unwrap(),
        })
        .collect()
}
pub fn limits() -> StoreLimits {
    StoreLimits {
        block_events: 256,
        blocks: 2432,
        max_events: 8192,
        views: 256,
        retirements: 128,
    }
}
pub fn logical(audio: bool, midi: bool, loops: usize, frames: usize) -> LogicalStateProgram {
    let mut graph = Graph::default();
    let port = |owner, role, channel| PortId {
        owner,
        signal: SignalType::Audio,
        role,
        index: channel,
    };
    if audio {
        for channel in 0..2 {
            for role in [PortRole::Input, PortRole::Output] {
                graph.ports.push(PortView {
                    id: port(Owner::Host, role, channel),
                    channel,
                    name: "audio".into(),
                });
            }
        }
    }
    for slot in 0..loops {
        let key = Id {
            slot: slot as u32,
            generation: 1,
        };
        graph.loops.push(LoopView {
            id: key,
            strip: None,
            layout: ChannelLayout::Direct {
                audio: if audio { 2 } else { 0 },
                midi: false,
            },
            gain: 1.0,
            balance: 0.0,
            boundary_smoothing_frames: 0,
            sync_source: None,
            repeat_sync: false,
            composite: None,
        });
        if audio {
            for channel in 0..2 {
                for role in [PortRole::Input, PortRole::Output] {
                    graph.ports.push(PortView {
                        id: port(Owner::Loop(key), role, channel),
                        channel,
                        name: "audio".into(),
                    });
                }
                graph.connections.push(Connection {
                    source: port(Owner::Host, PortRole::Input, channel),
                    destination: port(Owner::Loop(key), PortRole::Input, channel),
                });
                graph.connections.push(Connection {
                    source: port(Owner::Loop(key), PortRole::Output, channel),
                    destination: port(Owner::Host, PortRole::Output, channel),
                });
            }
        }
    }
    let mut logical = rt_fpga_compiler::compile_with_chunk_frames(
        &graph,
        CompileContext {
            quantum: QUANTUM as u32,
            limits: Default::default(),
        },
        QUANTUM as u32,
    )
    .unwrap();
    for entry in &mut logical.loops {
        entry.length = frames as u64;
    }
    if !midi {
        return logical;
    }
    let mut operations = vec![M::Input {
        port: 0,
        destination: 0,
    }];
    for slot in 0..loops {
        let key = Id {
            slot: slot as u32,
            generation: 1,
        };
        operations.push(M::Capture {
            loop_id: key,
            sequence: slot as u16,
            source: 0,
        });
        operations.push(M::Playback {
            loop_id: key,
            sequence: slot as u16,
            cursor: slot as u16,
            channels: 1 << (slot % 8),
            destination: slot as u16 + 1,
        });
    }
    let mut next = loops as u16 + 1;
    for start in (0..loops).step_by(8) {
        let mut left = start as u16 + 1;
        for slot in start + 1..(start + 8).min(loops) {
            operations.push(M::Merge {
                left,
                right: slot as u16 + 1,
                destination: next,
            });
            left = next;
            next += 1;
        }
        operations.push(M::Output {
            source: left,
            port: (start / 8) as u16,
        });
    }
    logical.with_midi_data(&operations).unwrap()
}

pub struct Reference {
    pub pipeline: Pipeline,
    pub jobs: JobProcessor,
    pub audio: MediaStore,
    pub midi: MidiMemory,
    pub emitted: Vec<OutputEvent>,
    pub outputs: Vec<Vec<Fixed>>,
    pub logical: LogicalStateProgram,
    pub loops: usize,
    pub frames: usize,
    pub has_midi: bool,
    pub state_enabled: bool,
    inputs: Vec<Vec<Fixed>>,
}
impl Reference {
    pub fn prepare(audio: bool, midi: bool, loops: usize, frames: usize, state: bool) -> Self {
        assert!((1..=64).contains(&loops) && (audio || midi));
        assert!(frames >= EVENT_PERIOD && frames % QUANTUM == 0);
        let mut logical = logical(audio, midi, loops, frames);
        if state && midi {
            logical = logical
                .with_midi_data(
                    &(0..loops)
                        .map(|slot| M::State {
                            loop_id: Id {
                                slot: slot as u32,
                                generation: 1,
                            },
                            sequence: slot as u16,
                            input: 0,
                            outputs: 1 << (slot / 8),
                            channels: 1 << (slot % 8),
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap();
        }
        let mut media = MediaStore::with_block_chunks(QUANTUM, 32).unwrap();
        let mut views = Vec::new();
        if audio {
            for slot in 0..loops {
                for channel in 0..2 {
                    let samples: Vec<_> = (0..frames)
                        .map(|frame| Fixed::from_f32(sample(slot, channel, frame)))
                        .collect();
                    views.push(media.insert(&samples).unwrap().0 as u64);
                }
            }
        }
        let compiled = RustTarget::new(&views).build(&logical).unwrap();
        let resources = compiled.resources;
        let mut pipeline = Pipeline::prepare(
            compiled.plan,
            compiled.program,
            compiled.memory,
            resources.queue_capacity as usize,
        )
        .unwrap();
        pipeline.trace_jobs(true);
        let mut jobs = JobProcessor::with_history_slots(
            QUANTUM,
            resources.buffers as usize,
            resources.history_slots as usize,
        );
        jobs.prepare_midi(BufferLimits {
            buffers: 256,
            cursors: 64,
            scratch_events: 131072,
            input_ports: 1,
            output_ports: DESTINATIONS as u16,
        })
        .unwrap();
        let mut memory = MidiMemory::prepare(limits(), 64).unwrap();
        if state && midi {
            memory.prepare_state(1, 8).unwrap();
            for output in 0..8 {
                let mut initial = rt_fpga_reference::midi::performance::State::new();
                for ch in 0..8 {
                    initial.set_word(ch * 512 + 129, 0x80000000 | 20).unwrap();
                }
                memory
                    .expression
                    .as_mut()
                    .unwrap()
                    .seed_output(output, &initial, &memory.store)
                    .unwrap();
            }
            pipeline.prepare_midi_state(&mut memory).unwrap();
        }
        if midi {
            for slot in 0..loops {
                let view = memory.store.writer().unwrap();
                if state {
                    let mut initial = rt_fpga_reference::midi::performance::State::new();
                    initial
                        .set_word((slot % 8) * 512 + 129, 0x80000000 | 20)
                        .unwrap();
                    memory
                        .store
                        .seed_snapshot(view, &initial, 1 << (slot % 8))
                        .unwrap();
                }
                for r in records(slot, frames) {
                    memory.store.append(view, r.frame, r.message).unwrap();
                }
                memory.store.finish(view, frames as u64).unwrap();
                pipeline
                    .replace_midi_view(&mut memory, id(slot), SequenceId(slot as u16), Some(view))
                    .unwrap();
                memory.store.release(view).unwrap();
            }
        }
        Self {
            pipeline,
            jobs,
            audio: media,
            midi: memory,
            logical,
            loops,
            frames,
            has_midi: midi,
            state_enabled: state && midi,
            emitted: Vec::with_capacity(8192),
            outputs: vec![vec![Fixed::ZERO; QUANTUM]; if audio { 2 } else { 0 }],
            inputs: vec![vec![Fixed::ZERO; QUANTUM]; if audio { 2 } else { 0 }],
        }
    }
    pub fn musical_fixture(&mut self) {
        use rt_fpga_reference::midi::performance::State;
        let mut target = State::new();
        target.known_reset(1);
        for bytes in [
            &[0xb0, 1, 20][..],
            &[0xe0, 16, 78],
            &[0xd0, 15],
            &[0x90, 60, 80],
            &[0xb0, 66, 127],
            &[0xa0, 60, 40],
        ] {
            target.apply(Message::parse(bytes).unwrap());
        }
        let view = self.midi.store.writer().unwrap();
        self.midi.store.seed_snapshot(view, &target, 1).unwrap();
        for record in records(0, self.frames) {
            self.midi
                .store
                .append(view, record.frame, record.message)
                .unwrap();
        }
        self.midi.store.finish(view, self.frames as u64).unwrap();
        self.pipeline
            .replace_midi_view(&mut self.midi, id(0), SequenceId(0), Some(view))
            .unwrap();
        self.midi.store.release(view).unwrap();
        self.midi.store.maintenance(64);
        let mut current = State::new();
        current.known_reset(1);
        for ch in 0..8 {
            current.set_word(ch * 512 + 129, 0x80000000 | 20).unwrap();
        }
        for bytes in [&[0x90, 67, 70][..], &[0xb0, 64, 127], &[0x80, 67, 0]] {
            current.apply(Message::parse(bytes).unwrap());
        }
        self.midi
            .expression
            .as_mut()
            .unwrap()
            .seed_output(0, &current, &self.midi.store)
            .unwrap();
    }
    pub fn command(&mut self, operation: Operation) {
        self.pipeline
            .write_plans(
                id(0),
                [
                    Some(PlannedTransition {
                        sequence: self.pipeline.state().frame() + 100,
                        effective: self.pipeline.state().frame(),
                        on_pulse: false,
                        remaining_ticks: 0,
                        operation,
                    }),
                    None,
                ],
            )
            .unwrap();
    }
    pub fn play(&mut self) {
        for slot in 0..self.loops {
            self.pipeline
                .write_plans(
                    id(slot),
                    [
                        Some(PlannedTransition {
                            sequence: 1,
                            operation: Operation::Play {
                                position: 0,
                                retrigger: true,
                            },
                            on_pulse: false,
                            effective: self.pipeline.state().frame(),
                            remaining_ticks: 0,
                        }),
                        None,
                    ],
                )
                .unwrap();
        }
    }
    pub fn callback(&mut self) {
        self.emitted.clear();
        let input_refs = match self.inputs.as_slice() {
            [l, r] => [l.as_slice(), r.as_slice()],
            [] => [&[][..], &[][..]],
            _ => unreachable!(),
        };
        let audio_ports = if self.outputs.is_empty() { 0 } else { 2 };
        let mut output_refs = match self.outputs.as_mut_slice() {
            [l, r] => [l.as_mut_slice(), r.as_mut_slice()],
            [] => [&mut [][..], &mut [][..]],
            _ => unreachable!(),
        };
        self.pipeline
            .process_with_midi(
                &mut self.jobs,
                &mut self.audio,
                &input_refs[..audio_ports],
                &mut output_refs[..audio_ports],
                QUANTUM,
                &mut self.midi,
                &[],
                &mut |event| {
                    self.emitted.push(event);
                    Ok(())
                },
            )
            .unwrap();
    }
    pub fn position(&self) -> usize {
        self.pipeline
            .state()
            .loop_state(id(0))
            .unwrap()
            .render
            .position as usize
    }
    pub fn validate(&self) {
        let position = self.position();
        let mut events = Vec::new();
        if self.has_midi {
            for slot in 0..self.loops {
                let phase = slot % 15 * QUANTUM;
                let order = position.saturating_sub(phase).div_ceil(EVENT_PERIOD);
                let frame = phase + order * EVENT_PERIOD;
                if frame >= position && frame < position + QUANTUM && frame < self.frames {
                    events.push(OutputEvent {
                        destination: (slot / 8) as u16,
                        offset: (frame - position) as u32,
                        message: Message::parse(&[
                            0xb0 | (slot % 8) as u8,
                            1,
                            (1 + (order + slot) % 127) as u8,
                        ])
                        .unwrap(),
                    });
                }
            }
        }
        assert_eq!(self.emitted, events);
        for (channel, samples) in self.outputs.iter().enumerate() {
            for (offset, value) in samples.iter().enumerate() {
                let expected: f32 = (0..self.loops)
                    .map(|slot| sample(slot, channel, position + offset))
                    .sum();
                assert_eq!(*value, Fixed::from_f32(expected));
            }
        }
    }
}
