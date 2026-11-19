use crate::CarlaHostOptions;
use rt_buffered::{BufferLimits, PreparedTiming, Timing};
use rt_runtime::{graph::PortCounts, Error, Quantum};
use shoop_carla::{
    buffered::Worker,
    native::{carla_runtime_availability, CarlaMainThreadUiDispatcher},
    service::{ChainEditor, ChainService},
};
use shoop_plugin_protocol::{CarlaChainType, ChainId};
use shoop_rt_engine::{
    EngineDescription, HostedProcessor, PreparedProcessor, PreparedProcessors, ProcessorEditors,
};
use shoop_rt_protocol::{
    processors::{CarlaHosting, EffectiveCarla, Unavailable},
    ProcessorKind, Refusal,
};
use std::path::Path;

pub(crate) fn prepare(
    description: &EngineDescription,
    period_frames: usize,
    options: &CarlaHostOptions,
    mut processors: PreparedProcessors,
) -> (PreparedProcessors, Vec<ChainService>) {
    let settings = description.processors.carla;
    if settings.hosting == CarlaHosting::Disabled {
        return (processors, Vec::new());
    }
    let prepared = prepare_enabled(
        description,
        period_frames,
        options.worker.as_deref(),
        options.ui.as_ref(),
        &processors,
    );
    match prepared {
        Ok((instances, services, bytes)) => {
            processors.instances.extend(instances);
            processors.reserved_bytes = bytes;
            processors.effective.carla = EffectiveCarla {
                rack_instances: settings.rack_instances,
                patchbay_instances: settings.patchbay_instances,
                unavailable: None,
            };
            (processors, services)
        }
        Err(reason) => {
            processors.effective.carla = EffectiveCarla::unavailable(reason);
            (processors, Vec::new())
        }
    }
}

fn prepare_enabled(
    description: &EngineDescription,
    period_frames: usize,
    executable: Option<&Path>,
    ui: Option<&CarlaMainThreadUiDispatcher>,
    processors: &PreparedProcessors,
) -> Result<(Vec<PreparedProcessor>, Vec<ChainService>, u64), Unavailable> {
    let settings = description.processors.carla;
    if settings.configured_instances() == 0 {
        return Err(Unavailable::NoCapacity);
    }
    if description.strip_channels < 2 {
        return Err(Unavailable::ChannelCapacity);
    }
    let isolated = settings.hosting == CarlaHosting::Subprocess;
    if isolated && executable.is_none() {
        return Err(Unavailable::Runtime);
    }
    let quantum = Quantum::new(description.quantum).map_err(|_| Unavailable::Memory)?;
    // Submit a period-sized block. Leave two periods for dispatch and DSP,
    // including when the driver executes several quanta back-to-back.
    let batch_quanta = period_frames.div_ceil(quantum.frames()).max(1);
    let timing = Timing {
        batch_quanta,
        slack_quanta: batch_quanta * 2,
    }
    .prepare(quantum)
    .map_err(|_| Unavailable::Memory)?;
    let limits = description
        .processor_buffer_limits(ProcessorKind::CarlaRack)
        .map_err(|_| Unavailable::Memory)?;
    let bytes = reservation(description, timing, limits, isolated, processors)
        .map_err(|_| Unavailable::Memory)?;
    carla_runtime_availability().map_err(|_| Unavailable::Runtime)?;

    let slots = settings.configured_instances() as usize * 2;
    let mut instances = Vec::with_capacity(slots);
    let mut services = Vec::with_capacity(slots);
    for _ in 0..2 {
        for (chain, kind, count) in [
            (
                CarlaChainType::Rack,
                ProcessorKind::CarlaRack,
                settings.rack_instances,
            ),
            (
                CarlaChainType::Patchbay,
                ProcessorKind::CarlaPatchbay,
                settings.patchbay_instances,
            ),
        ] {
            for _ in 0..count {
                let ready = if let Some(executable) = executable.filter(|_| isolated) {
                    ChainService::start_isolated(
                        executable.to_path_buf(),
                        ChainId(services.len() as u64 + 1),
                        chain,
                        description.sample_rate,
                        timing,
                        limits,
                    )
                } else {
                    ChainService::start_in_process(
                        chain,
                        description.sample_rate,
                        timing,
                        limits,
                        ui.cloned(),
                    )
                };
                let (endpoint, service) = ready.map_err(|_| Unavailable::Runtime)?;
                services.push(service);
                instances.push(PreparedProcessor {
                    kind,
                    backend: Box::new(endpoint),
                    timing: timing.timing(),
                });
            }
        }
    }
    Ok((instances, services, bytes))
}

pub(crate) struct Editors {
    first: usize,
    chains: Vec<ChainEditor>,
}

impl Editors {
    pub(crate) fn new(first: usize, services: &[ChainService]) -> Option<Self> {
        services
            .iter()
            .any(|service| service.editor().is_some())
            .then(|| Self {
                first,
                chains: services.iter().map(ChainService::editor_handle).collect(),
            })
    }

    fn editor(
        &self,
        index: u16,
    ) -> Option<std::sync::Arc<dyn shoop_carla::processor::CarlaExternalUi>> {
        self.chains
            .get(usize::from(index).checked_sub(self.first)?)?
            .current()
    }
}

impl ProcessorEditors for Editors {
    fn visible(&self, index: u16) -> Option<bool> {
        Some(self.editor(index)?.is_visible())
    }

    fn set_visible(&self, index: u16, visible: bool) -> Result<(), Refusal> {
        self.editor(index)
            .ok_or(Refusal::Unsupported)?
            .set_visible(visible)
            .map_err(|_| Refusal::Internal)
    }
}

fn reservation(
    description: &EngineDescription,
    timing: PreparedTiming,
    limits: BufferLimits,
    isolated: bool,
    processors: &PreparedProcessors,
) -> Result<u64, Error> {
    let ports = PortCounts {
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
    };
    let adapter = limits.async_storage_bytes_for(ports, timing.quantum(), timing.timing())?;
    let transport = Worker::transport_bytes(CarlaChainType::Rack, timing, limits)?;
    let aux = HostedProcessor::additional_prepared_bytes(
        limits,
        description.global_midi_input_capacity()?,
    )?;
    let audio = shoop_carla::native::CARLA_MAX_BUFFER_SIZE * 4 * std::mem::size_of::<f32>();
    let ipc = if isolated {
        shoop_carla::carla_shared_memory::FILE_SIZE + audio
    } else {
        0
    };
    // A retired owner's replacement can be prepared before releasing the previous processor.
    let per_slot = adapter + transport + aux + 2 * (audio + ipc);
    let total = (per_slot as u64)
        .checked_mul(u64::from(description.processors.carla.configured_instances()) * 2)
        .and_then(|bytes| bytes.checked_add(processors.reserved_bytes))
        .ok_or(Error::Capacity)?;
    let capture = description.worker_capture_storage_bytes(timing.latency_frames())? as u64;
    if total.saturating_add(capture) > processors.effective.memory_bytes {
        return Err(Error::Capacity);
    }
    Ok(total)
}
