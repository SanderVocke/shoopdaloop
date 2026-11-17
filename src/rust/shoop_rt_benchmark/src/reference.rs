use crate::{
    fixture::*,
    runner::{Control, Driver, Measurement},
};
use anyhow::{anyhow, Result};
use rt_fpga_compiler::{
    graph::{GraphDescription, LoopDescription},
    Identity, StateProgramTarget,
};
use rt_fpga_reference::{
    compiler::RustTarget,
    fixed::Fixed,
    jobs::JobProcessor,
    pipeline::Pipeline,
    rt_interface::{Configuration, LoopBinding, Processor},
    storage::{MediaStore, ViewId},
};
use shoop_rt_protocol::*;
use shoop_rt_transport::{carrier::Outbox, rt};
use std::time::Instant;

pub fn graph() -> Result<rt_fpga_compiler::LogicalStateProgram> {
    GraphDescription {
        quantum: QUANTUM as u32,
        loops: (0..LOOPS)
            .map(|slot| LoopDescription {
                id: Identity {
                    slot,
                    generation: 1,
                },
                source: (slot != 0).then_some(0),
                input: true,
                output: true,
                gain: GAIN,
                repeat_sync: false,
            })
            .collect(),
    }
    .compile_with_channels(CHANNELS)
    .map_err(|e| anyhow!("graph preparation: {e:?}"))
}

pub fn limits() -> Limits {
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
        host_audio_inputs: 2,
        host_audio_outputs: 2,
        host_midi_inputs: 0,
        host_midi_outputs: 0,
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
    }
    fn process(&mut self) -> Result<Measurement> {
        let start = Instant::now();
        let [left, right] = &mut self.outputs;
        self.processor
            .process(
                &mut self.jobs,
                &mut self.media,
                &[&self.inputs[0], &self.inputs[1]],
                &mut [left, right],
            )
            .map_err(|e| anyhow!("reference processing: {e:?}"))?;
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        for (source, target) in self.outputs.iter().zip(&mut self.samples) {
            for (sample, dest) in source.iter().zip(target) {
                *dest = sample.to_f32();
            }
        }
        Ok(Measurement {
            elapsed_ns,
            storage_bytes: ((self.media.slot_count() - self.media.free_chunk_count()) * 4) as u64,
            ..Default::default()
        })
    }
    fn output(&self, channel: usize) -> &[f32] {
        &self.samples[channel]
    }
    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"driver":"stepped-dummy","numeric":"Q8.24","timing":"functional-model-host-wall-clock","chunk_frames":1,"block_chunks":128,"media_capacity_bytes":MEMORY_BYTES,"stereo_media_views":LOOPS*CHANNELS,"smoothing_frames":0,"ramp_frames":0})
    }
}

pub fn prepare() -> Result<(Control<rt::Endpoint>, Reference)> {
    let logical = graph()?;
    let mut media = MediaStore::with_block_chunk_limit(1, 128, MEMORY_BYTES / 4)
        .map_err(|e| anyhow!("media: {e:?}"))?;
    media
        .create_recordings((LOOPS * CHANNELS) as usize)
        .map_err(|e| anyhow!("views: {e:?}"))?;
    media
        .reserve_chunk_capacity(MEMORY_BYTES / 4)
        .map_err(|e| anyhow!("reserve: {e:?}"))?;
    for view in 0..LOOPS * CHANNELS {
        media
            .reserve_view_capacity(ViewId(view as usize), 4096)
            .map_err(|e| anyhow!("reserve view: {e:?}"))?;
    }
    let views: Vec<_> = (0..u64::from(LOOPS * CHANNELS)).collect();
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
            limits: limits(),
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
    let driver = Reference {
        processor,
        media,
        jobs: JobProcessor::with_history_slots(
            QUANTUM,
            resources.buffers as usize,
            resources.history_slots as usize,
        ),
        inputs: std::array::from_fn(|_| vec![Fixed::ZERO; QUANTUM]),
        outputs: std::array::from_fn(|_| vec![Fixed::ZERO; QUANTUM]),
        samples: std::array::from_fn(|_| vec![0.; QUANTUM]),
    };
    let mut control = Control::new(endpoint, Outbox::new(256, 256, 0))?;
    control.configuration = (1, 1);
    Ok((control, driver))
}
