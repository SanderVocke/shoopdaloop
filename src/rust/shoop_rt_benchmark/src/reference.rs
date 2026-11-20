use crate::{
    fixture::*,
    runner::{Control, Driver, Measurement},
};
use anyhow::{anyhow, Result};
use rt_fpga_compiler::StateProgramTarget;
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::JobProcessor,
    midi::{BufferLimits, InputEvent, Message, MidiMemory},
    pipeline::Pipeline,
    rt_interface::{Configuration, LoopBinding, Processor},
    storage::{MediaStore, ViewId},
    target::RustTarget,
};
use shoop_rt_protocol::*;
use shoop_rt_transport::{carrier::Outbox, rt};
use std::time::Instant;

pub fn graph(media: Media) -> Result<rt_fpga_compiler::LogicalStateProgram> {
    use rt_fpga_compiler::{shoop_graph::*, CompileContext, MidiOperation as M};
    let id = |slot| Id {
        slot,
        generation: 1,
    };
    let channels = media.audio_channels() as u32;
    let layout = ChannelLayout::Direct {
        audio: channels,
        midi: false,
    };
    let mut graph = Graph::default();
    graph.strips.push(StripView {
        id: id(0),
        name: "benchmark".into(),
        layout,
        controls: StripControls::default(),
        default_dry_playback: false,
        processor: None,
    });
    graph.buses.push(BusView {
        id: id(0),
        name: "master".into(),
        channels,
        master: true,
        gain_db: 0.,
        balance: 0.,
        muted: false,
        processor: None,
    });
    for slot in 0..LOOPS {
        graph.loops.push(LoopView {
            id: id(slot),
            strip: Some(id(0)),
            layout,
            gain: GAIN,
            balance: 0.,
            boundary_smoothing_frames: 0,
            sync_source: (slot != 0).then_some(id(0)),
            repeat_sync: false,
            composite: None,
        });
    }
    let port = |owner, role, index| PortId {
        owner,
        role,
        index,
        signal: SignalType::Audio,
    };
    for channel in 0..channels {
        for owner in [Owner::Host, Owner::Strip(id(0)), Owner::Bus(id(0))] {
            for role in [PortRole::Input, PortRole::Output] {
                graph.ports.push(PortView {
                    id: port(owner, role, channel),
                    channel,
                    name: "audio".into(),
                });
            }
        }
        for (source, destination) in [
            (
                port(Owner::Host, PortRole::Input, channel),
                port(Owner::Strip(id(0)), PortRole::Input, channel),
            ),
            (
                port(Owner::Strip(id(0)), PortRole::Output, channel),
                port(Owner::Bus(id(0)), PortRole::Input, channel),
            ),
            (
                port(Owner::Bus(id(0)), PortRole::Output, channel),
                port(Owner::Host, PortRole::Output, channel),
            ),
        ] {
            graph.connections.push(Connection {
                source,
                destination,
            });
        }
    }
    let logical = rt_fpga_compiler::compile_with_chunk_frames(
        &graph,
        CompileContext {
            quantum: QUANTUM as u32,
            limits: Default::default(),
        },
        FPGA_CHUNK_FRAMES as u32,
    )
    .map_err(|e| anyhow!("graph preparation: {e:?}"))?;
    if !media.midi() {
        return Ok(logical);
    }
    let mut operations = vec![M::Input {
        port: 0,
        destination: 0,
    }];
    for slot in 0..LOOPS {
        operations.push(M::Capture {
            loop_id: id(slot),
            sequence: slot as u16,
            source: 0,
        });
        operations.push(M::Playback {
            loop_id: id(slot),
            sequence: slot as u16,
            cursor: slot as u16,
            channels: u16::MAX,
            destination: slot as u16 + 1,
        });
    }
    let mut level: Vec<_> = (1..=LOOPS as u16).collect();
    let mut next = LOOPS as u16 + 1;
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| {
                let destination = next;
                next += 1;
                operations.push(M::Merge {
                    left: pair[0],
                    right: pair[1],
                    destination,
                });
                destination
            })
            .collect();
    }
    operations.push(M::Output {
        source: level[0],
        port: 0,
    });
    logical
        .with_midi_data(&operations)
        .map_err(|e| anyhow!("MIDI preparation: {e:?}"))
}

pub fn midi_limits() -> rt_fpga_reference::midi::StoreLimits {
    rt_fpga_reference::midi::StoreLimits {
        block_events: MIDI_BLOCK_EVENTS,
        blocks: 512,
        max_events: 2048,
        views: 256,
        retirements: 128,
    }
}

pub fn limits(media: Media) -> Limits {
    Limits {
        composite_entries: 0,
        composite_segments: 0,
        timing_relations: 0,
        content_chunk_bytes: 0,
        strips: 1,
        loops: LOOPS,
        strip_audio_channels: 2,
        master_channels: 2,
        buses: 1,
        bus_channels: 2,
        host_audio_inputs: media.audio_channels() as u16,
        host_audio_outputs: media.audio_channels() as u16,
        host_midi_inputs: u16::from(media.midi()),
        host_midi_outputs: u16::from(media.midi()),
        max_frames: QUANTUM as u32,
        quantum: QUANTUM as u32,
        processing_latency_frames: 0,
        smoothing_limit_frames: 0,
        reliable_credits: 0,
        bulk_credits: 0,
    }
}

pub struct Reference {
    processor: Processor,
    jobs: JobProcessor,
    media: MediaStore,
    inputs: [Vec<Fixed>; 2],
    media_kind: Media,
    midi: MidiMemory,
    midi_packets: Vec<MidiPacket>,
    midi_inputs: Vec<InputEvent>,
    emitted: Vec<MidiPacket>,
    outputs: [Vec<Fixed>; 2],
    samples: [Vec<f32>; 2],
}
impl Driver for Reference {
    fn next_frame(&self) -> u64 {
        self.processor.pipeline().state().frame()
    }
    fn fill_input(&mut self) {
        let first = self.next_frame();
        for (channel, samples) in self.inputs.iter_mut().enumerate() {
            for (offset, sample) in samples.iter_mut().enumerate() {
                *sample = Fixed::from_f32(input(channel, first + offset as u64));
            }
        }
        self.midi_inputs.clear();
        if self.media_kind.midi() {
            midi_input(first, &mut self.midi_packets);
            self.midi_inputs
                .extend(self.midi_packets.iter().map(|e| InputEvent {
                    input: 0,
                    offset: e.offset,
                    message: Message::parse(&e.bytes).unwrap(),
                }));
        }
    }
    fn process(&mut self) -> Result<Measurement> {
        let start = Instant::now();
        let [left, right] = &mut self.outputs;
        let input_refs = [&self.inputs[0][..], &self.inputs[1][..]];
        let mut output_refs = [&mut left[..], &mut right[..]];
        let channels = self.media_kind.audio_channels();
        self.emitted.clear();
        if self.media_kind.midi() {
            let emitted = &mut self.emitted;
            self.processor
                .process_with_midi(
                    &mut self.jobs,
                    &mut self.media,
                    &input_refs[..channels],
                    &mut output_refs[..channels],
                    QUANTUM,
                    &mut self.midi,
                    &self.midi_inputs,
                    &mut |event| {
                        if emitted.len() == emitted.capacity() {
                            return Err(rt_fpga_reference::midi::Error::Capacity);
                        }
                        emitted.push(MidiPacket {
                            offset: event.offset,
                            bytes: event.message.as_bytes().try_into().unwrap(),
                        });
                        Ok(())
                    },
                )
                .map_err(|e| anyhow!("reference MIDI processing: {e:?}"))?;
        } else {
            self.processor
                .process(
                    &mut self.jobs,
                    &mut self.media,
                    &input_refs,
                    &mut output_refs,
                )
                .map_err(|e| anyhow!("reference processing: {e:?}"))?;
        }
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        for (source, target) in self.outputs.iter().zip(&mut self.samples) {
            for (sample, dest) in source.iter().zip(target) {
                *dest = sample.to_f32();
            }
        }
        Ok(Measurement {
            elapsed_ns,
            storage_bytes: ((self.media.slot_count() - self.media.free_chunk_count())
                * FPGA_CHUNK_FRAMES
                * 4) as u64
                + self.midi.store.usage().blocks as u64 * u64::from(MIDI_BLOCK_EVENTS) * 16,
            midi_storage_blocks: self
                .media_kind
                .midi()
                .then_some(self.midi.store.usage().blocks as u64),
            midi_input_events: self.midi_inputs.len() as u64,
            midi_output_events: self.emitted.len() as u64,
            midi_record_reads: Some(self.jobs.midi_work().record_reads),
            midi_captured_events: Some(self.jobs.midi_work().captured_events),
            midi_merged_events: Some(self.jobs.midi_work().merged_events),
            ..Default::default()
        })
    }
    fn output(&self, channel: usize) -> &[f32] {
        &self.samples[channel]
    }
    fn midi_output(&self) -> &[MidiPacket] {
        &self.emitted
    }
    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"driver":"stepped-dummy","numeric":"Q8.24","timing":"functional-model-host-wall-clock","chunk_frames":FPGA_CHUNK_FRAMES,"block_chunks":FPGA_BLOCK_CHUNKS,"media_capacity_bytes":MEMORY_BYTES,"stereo_media_views":LOOPS*self.media_kind.audio_channels() as u32,"smoothing_frames":0,"ramp_frames":0})
    }
}

pub fn prepare(media_kind: Media) -> Result<(Control<rt::Endpoint>, Reference)> {
    let logical = graph(media_kind)?;
    let channels = media_kind.audio_channels() as u32;
    let chunks = MEMORY_BYTES / (FPGA_CHUNK_FRAMES * 4);
    let mut media =
        MediaStore::with_block_chunk_limit(FPGA_CHUNK_FRAMES, FPGA_BLOCK_CHUNKS, chunks)
            .map_err(|e| anyhow!("media: {e:?}"))?;
    if channels > 0 {
        media
            .create_recordings((LOOPS * channels) as usize)
            .map_err(|e| anyhow!("views: {e:?}"))?;
    }
    media
        .reserve_chunk_capacity(chunks)
        .map_err(|e| anyhow!("reserve: {e:?}"))?;
    for view in 0..LOOPS * channels {
        media
            .reserve_view_capacity(ViewId(view as usize), 4096)
            .map_err(|e| anyhow!("reserve view: {e:?}"))?;
    }
    let views: Vec<_> = (0..u64::from(LOOPS * channels)).collect();
    let compiled = RustTarget::new(&views)
        .build(&logical)
        .map_err(|e| anyhow!("reference lowering: {e:?}"))?;
    let resources = compiled.resources;
    let bindings = compiled
        .bindings
        .iter()
        .map(|binding| LoopBinding {
            gain: binding.gain,
            meter: binding.meter,
            initial_gain: GAIN,
            ..LoopBinding::new(binding.id)
        })
        .collect();
    let pipeline = Pipeline::prepare(
        compiled.plan,
        compiled.program,
        compiled.memory,
        resources.queue_capacity as usize,
    )
    .map_err(|e| anyhow!("pipeline: {e:?}"))?;
    let (control, execution) =
        rt::channel(256, 3, snapshot::capacity(1, LOOPS as usize, 1).unwrap())
            .map_err(|e| anyhow!("channel: {e:?}"))?;
    let endpoint = rt::Endpoint::new(
        control,
        rt::Settings {
            graph: 1,
            topology: 1,
            limits: limits(media_kind),
            driver: DriverStatus {
                kind: DriverKind::Dummy,
                state: DriverRunState::Running,
                sample_rate: SAMPLE_RATE,
                buffer_frames: QUANTUM as u32,
            },
        },
    )
    .map_err(|e| anyhow!("endpoint: {e:?}"))?;
    let processor = Processor::prepare(
        pipeline,
        execution,
        Configuration {
            graph: 1,
            topology: 1,
            frame: 0,
        },
        bindings,
    )
    .map_err(|e| anyhow!("processor: {e:?}"))?;
    let mut jobs = JobProcessor::with_history_slots(
        QUANTUM,
        resources.buffers as usize,
        resources.history_slots as usize,
    );
    jobs.prepare_midi(BufferLimits {
        buffers: 128,
        cursors: LOOPS as u16,
        scratch_events: MIDI_SCRATCH_EVENTS,
        input_ports: 1,
        output_ports: 1,
    })
    .map_err(|e| anyhow!("MIDI buffers: {e:?}"))?;
    let driver = Reference {
        media_kind,
        midi: MidiMemory::prepare(midi_limits(), LOOPS as u16)
            .map_err(|e| anyhow!("MIDI memory: {e:?}"))?,
        midi_packets: Vec::with_capacity(MIDI_INPUT_CAPACITY),
        midi_inputs: Vec::with_capacity(MIDI_INPUT_CAPACITY),
        emitted: Vec::with_capacity(MIDI_OUTPUT_CAPACITY),
        processor,
        media,
        jobs,
        inputs: std::array::from_fn(|_| vec![Fixed::ZERO; QUANTUM]),
        outputs: std::array::from_fn(|_| vec![Fixed::ZERO; QUANTUM]),
        samples: std::array::from_fn(|_| vec![0.; QUANTUM]),
    };
    let mut control = Control::new(endpoint, Outbox::new(256, 256, 0))?;
    control.configuration = (1, 1);
    Ok((control, driver))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_fpga_compiler::Operation;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn media_profiles_preserve_audio_work_and_omit_audio_io_from_midi_only() {
        let audio = graph(Media::Audio).unwrap();
        let combined = graph(Media::AudioMidi).unwrap();
        let midi = graph(Media::Midi).unwrap();
        assert_eq!(audio.loops, combined.loops);
        assert_eq!(audio.sync_pass, midi.sync_pass);
        assert_eq!(audio.initial_state, combined.initial_state);
        let audio_operations: Vec<_> = combined
            .work_pass
            .iter()
            .copied()
            .filter(|op| !matches!(op, Operation::MidiData(_)))
            .collect();
        assert_eq!(audio_operations, audio.work_pass);
        assert!(midi
            .work_pass
            .iter()
            .all(|op| matches!(op, Operation::MidiData(_) | Operation::Silence { .. })));
        assert_eq!(midi.resources.input_ports, 0);
        assert_eq!(midi.resources.output_ports, 0);
        assert_eq!(midi.loops.len(), LOOPS as usize);
    }
}
