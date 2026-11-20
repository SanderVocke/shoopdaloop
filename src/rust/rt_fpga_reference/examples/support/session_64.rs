#![allow(dead_code)]

use rt_fpga_compiler::shoop_graph::*;
use rt_fpga_compiler::{CompileContext, LogicalStateProgram, MidiBinding, StateProgramTarget};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::JobProcessor,
    midi::{Binding, Message, MidiSession, OutputEvent, Record, SessionLimits, SessionReport},
    pipeline::Pipeline,
    records::Identity,
    state::PlannedTransition,
    storage::MediaStore,
    target::RustTarget,
    transitions::Operation,
};

pub const QUANTUM: usize = 32;
pub const SAMPLE_RATE: usize = 48_000;
pub const FRAMES: usize = SAMPLE_RATE * 30;
pub const LOOPS: usize = 64;
pub const DESTINATIONS: usize = 8;
pub const EVENT_PERIOD: usize = SAMPLE_RATE / 100;

pub fn id(slot: usize) -> Identity {
    Identity {
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
            message: Message::managed(&[
                0xb0 | (slot % 8) as u8,
                1,
                (1 + (order + slot) % 127) as u8,
            ])
            .unwrap(),
        })
        .collect()
}

pub fn logical(audio: bool, loops: usize, frames: usize) -> LogicalStateProgram {
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
    logical
        .with_midi(
            &(0..loops)
                .map(|slot| MidiBinding {
                    loop_id: Id {
                        slot: slot as u32,
                        generation: 1,
                    },
                    context: slot as u16,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
}

pub fn limits(loops: usize) -> SessionLimits {
    let mut limits = SessionLimits::session();
    limits.loops = loops as u16;
    limits.store.blocks = 2432;
    limits.store.views = 256;
    limits.store.snapshots = 384;
    limits.store.retirements = 128;
    limits.input_events_per_quantum = 16;
    limits.history_frames = 128;
    limits
}

pub struct Reference {
    pub pipeline: Pipeline,
    pub jobs: JobProcessor,
    pub audio: MediaStore,
    pub midi: MidiSession,
    pub emitted: Vec<OutputEvent>,
    pub outputs: Vec<Vec<Fixed>>,
    pub logical: LogicalStateProgram,
    pub loops: usize,
    pub frames: usize,
    inputs: Vec<Vec<Fixed>>,
}
impl Reference {
    pub fn new(audio: bool, loops: usize, frames: usize) -> Self {
        assert!((1..=LOOPS).contains(&loops) && frames >= EVENT_PERIOD && frames % QUANTUM == 0);
        let logical = logical(audio, loops, frames);
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
        let jobs = JobProcessor::with_history_slots(
            QUANTUM,
            resources.buffers as usize,
            resources.history_slots as usize,
        );
        let mut midi = MidiSession::prepare(limits(loops)).unwrap();
        midi.set_history_retention(0, 0).unwrap();
        for slot in 0..loops {
            midi.bind(
                slot as u16,
                Binding {
                    loop_id: id(slot),
                    input: 0,
                    destination: (slot / 8) as u16,
                    channels: 1 << (slot % 8),
                },
            )
            .unwrap();
            let view = midi.begin_import(&Default::default()).unwrap();
            for events in records(slot, frames).chunks(64) {
                assert_eq!(midi.import_events(view, events).unwrap(), events.len());
            }
            while !midi.finish_import(view, frames as u64, 256).unwrap() {}
            pipeline
                .replace_midi(&mut midi, &[slot as u16], view)
                .unwrap();
            midi.release(view).unwrap();
        }
        Self {
            pipeline,
            jobs,
            audio: media,
            midi,
            emitted: Vec::with_capacity(8192),
            outputs: vec![vec![Fixed::ZERO; QUANTUM]; if audio { 2 } else { 0 }],
            inputs: vec![vec![Fixed::ZERO; QUANTUM]; if audio { 2 } else { 0 }],
            logical,
            loops,
            frames,
        }
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
    pub fn callback(&mut self) -> SessionReport {
        self.emitted.clear();
        let input_count = self.inputs.len();
        let output_count = self.outputs.len();
        let inputs: [&[Fixed]; 2] =
            core::array::from_fn(|i| self.inputs.get(i).map_or(&[][..], Vec::as_slice));
        let mut outputs: [&mut [Fixed]; 2] = [&mut [], &mut []];
        for (target, source) in outputs.iter_mut().zip(self.outputs.iter_mut()) {
            *target = source;
        }
        self.pipeline
            .process_midi_session(
                &mut self.jobs,
                &mut self.audio,
                &inputs[..input_count],
                &mut outputs[..output_count],
                QUANTUM,
                &mut self.midi,
                &[],
                &mut |event| {
                    self.emitted.push(event);
                    Ok(())
                },
            )
            .unwrap()
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
        assert_eq!(
            self.emitted,
            expected_events(self.loops, self.frames, position)
        );
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

pub fn expected_events(loops: usize, frames: usize, position: usize) -> Vec<OutputEvent> {
    let mut events = Vec::new();
    for slot in 0..loops {
        let phase = slot % 15 * QUANTUM;
        let order = position.saturating_sub(phase).div_ceil(EVENT_PERIOD);
        let frame = phase + order * EVENT_PERIOD;
        if frame >= position && frame < position + QUANTUM && frame < frames {
            events.push((
                slot,
                OutputEvent {
                    destination: (slot / 8) as u16,
                    offset: (frame - position) as u32,
                    message: Message::managed(&[
                        0xb0 | (slot % 8) as u8,
                        1,
                        (1 + (order + slot) % 127) as u8,
                    ])
                    .unwrap(),
                },
            ));
        }
    }
    events.sort_by_key(|(slot, event)| (event.offset, *slot));
    events.into_iter().map(|(_, event)| event).collect()
}
