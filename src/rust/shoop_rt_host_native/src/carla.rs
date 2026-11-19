use crate::CarlaHostOptions;
use rt_buffered::{BufferLimits, PreparedTiming, Timing};
use rt_runtime::{graph::PortCounts, Error, Quantum};
use shoop_carla::{
    buffered::Worker,
    native::{carla_runtime_availability, CarlaMainThreadUiDispatcher},
    service::{ChainControl, ChainMonitor, ChainService, ControlTicket},
};
use shoop_plugin_protocol::{CarlaChainType, CarlaProcessorLifecycle, ChainId, WorkerExitKind};
use shoop_rt_engine::{
    EngineDescription, HostedCheckpointTicket, HostedControlTicket, HostedLogTicket,
    HostedProcessor, HostedProcessorControls, HostedProcessorLog, HostedProcessorObservation,
    PreparedProcessor, PreparedProcessors,
};
use shoop_rt_protocol::{
    processors::{
        CarlaHosting, EffectiveCarla, HostedFailure, HostedLifecycle, HostedStatus, Unavailable,
    },
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
        Ok(prepared) => {
            processors.instances.extend(prepared.instances);
            processors.reserved_bytes = prepared.bytes;
            processors.effective.carla = prepared.effective;
            (processors, prepared.services)
        }
        Err(reason) => {
            processors.effective.carla = EffectiveCarla::unavailable(reason);
            (processors, Vec::new())
        }
    }
}

struct PreparedBanks {
    instances: Vec<PreparedProcessor>,
    services: Vec<ChainService>,
    bytes: u64,
    effective: EffectiveCarla,
}

fn prepare_enabled(
    description: &EngineDescription,
    period_frames: usize,
    executable: Option<&Path>,
    ui: Option<&CarlaMainThreadUiDispatcher>,
    processors: &PreparedProcessors,
) -> Result<PreparedBanks, Unavailable> {
    let settings = description.processors.carla;
    if settings.configured_instances() == 0 {
        return Err(Unavailable::NoCapacity);
    }
    if description.strip_channels < 2 {
        return Err(Unavailable::ChannelCapacity);
    }
    let effective = EffectiveCarla {
        rack_instances: settings.rack_instances,
        patchbay_instances: settings.patchbay_instances,
        patchbay16x_instances: if u16::from(description.strip_channels)
            >= CarlaChainType::Patchbay16x.audio_channels()
        {
            settings.patchbay16x_instances
        } else {
            0
        },
        unavailable: None,
    };
    if effective.instances() == 0 {
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
    let banks = [
        (
            CarlaChainType::Rack,
            ProcessorKind::CarlaRack,
            effective.rack_instances,
        ),
        (
            CarlaChainType::Patchbay,
            ProcessorKind::CarlaPatchbay,
            effective.patchbay_instances,
        ),
        (
            CarlaChainType::Patchbay16x,
            ProcessorKind::CarlaPatchbay16x,
            effective.patchbay16x_instances,
        ),
    ];
    let bytes = reservation(description, timing, limits, isolated, processors, &banks)
        .map_err(|_| Unavailable::Memory)?;
    carla_runtime_availability().map_err(|_| Unavailable::Runtime)?;

    let slots = effective.instances() as usize * 2;
    let mut instances = Vec::with_capacity(slots);
    let mut services = Vec::with_capacity(slots);
    for _ in 0..2 {
        for (chain, kind, count) in banks {
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
    Ok(PreparedBanks {
        instances,
        services,
        bytes,
        effective,
    })
}

pub(crate) struct Controls {
    first: usize,
    chains: Vec<Chain>,
    has_editors: bool,
    has_logs: bool,
}

struct Chain {
    monitor: ChainMonitor,
    control: ChainControl,
}

impl Controls {
    pub(crate) fn new(first: usize, services: &[ChainService]) -> Self {
        Self {
            first,
            chains: services
                .iter()
                .map(|service| Chain {
                    monitor: service.monitor(),
                    control: service.control(),
                })
                .collect(),
            has_editors: services.iter().any(|service| service.editor().is_some()),
            has_logs: services
                .iter()
                .any(|service| service.monitor().owned_health().1.generation != 0),
        }
    }

    fn chain(&self, index: u16) -> Option<&Chain> {
        self.chains.get(usize::from(index).checked_sub(self.first)?)
    }
}

impl HostedProcessorControls for Controls {
    fn has_editors(&self) -> bool {
        self.has_editors
    }

    fn has_logs(&self) -> bool {
        self.has_logs
    }

    fn has_checkpoints(&self) -> bool {
        !self.chains.is_empty()
    }

    fn status(&self, index: u16) -> Option<HostedProcessorObservation> {
        let chain = &self.chain(index)?.monitor;
        let (owner, health) = chain.owned_health();
        let status = HostedStatus {
            generation: health.generation,
            active: health.active,
            lifecycle: match health.lifecycle {
                CarlaProcessorLifecycle::Stopped => HostedLifecycle::Stopped,
                CarlaProcessorLifecycle::Starting => HostedLifecycle::Starting,
                CarlaProcessorLifecycle::Running => HostedLifecycle::Running,
                CarlaProcessorLifecycle::Degraded => HostedLifecycle::Degraded,
                CarlaProcessorLifecycle::Crashed => HostedLifecycle::Crashed,
                CarlaProcessorLifecycle::Restarting => HostedLifecycle::Restarting,
                CarlaProcessorLifecycle::Unavailable => HostedLifecycle::Unavailable,
            },
            failure: match health.exit {
                WorkerExitKind::None | WorkerExitKind::Requested | WorkerExitKind::UiClosed => None,
                WorkerExitKind::StartupFailure => Some(HostedFailure::Startup),
                WorkerExitKind::ProtocolFailure => Some(HostedFailure::Protocol),
                WorkerExitKind::UnexpectedExit => Some(HostedFailure::UnexpectedExit),
                WorkerExitKind::Unresponsive => Some(HostedFailure::Unresponsive),
                WorkerExitKind::ParentDisconnected => Some(HostedFailure::Disconnected),
            },
            editor_visible: chain.editor().map(|editor| editor.is_visible()),
        };
        Some(HostedProcessorObservation { owner, status })
    }

    fn set_visible(&self, index: u16, visible: bool) -> Result<(), Refusal> {
        self.chain(index)
            .and_then(|chain| chain.monitor.editor())
            .ok_or(Refusal::Unsupported)?
            .set_visible(visible)
            .map_err(|_| Refusal::Internal)
    }

    fn recover(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        let chain = self.chain(index).ok_or(Refusal::Unsupported)?;
        let ticket = chain
            .control
            .recover(owner, generation, true)
            .map_err(|_| Refusal::Internal)?;
        Ok(Box::new(PendingControl(ticket)))
    }

    fn clear_logs(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        let chain = self.chain(index).ok_or(Refusal::Unsupported)?;
        let ticket = chain
            .control
            .clear_logs(owner, generation)
            .map_err(|_| Refusal::Internal)?;
        Ok(Box::new(PendingControl(ticket)))
    }

    fn read_logs(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedLogTicket>, Refusal> {
        let chain = self.chain(index).ok_or(Refusal::Unsupported)?;
        let ticket = chain
            .control
            .read_logs(owner, generation)
            .map_err(|_| Refusal::Internal)?;
        Ok(Box::new(PendingLogs(ticket)))
    }

    fn capture_checkpoint(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedCheckpointTicket>, Refusal> {
        let chain = self.chain(index).ok_or(Refusal::Unsupported)?;
        let ticket = chain
            .control
            .capture_state(owner, generation)
            .map_err(|_| Refusal::Internal)?;
        Ok(Box::new(PendingCheckpoint(ticket)))
    }
}

struct PendingCheckpoint(shoop_carla::service::StateTicket);

impl HostedCheckpointTicket for PendingCheckpoint {
    fn poll(&mut self) -> Option<Result<String, Refusal>> {
        self.0
            .poll()
            .map(|result| result.map_err(|_| Refusal::Internal))
    }
}

struct PendingControl(ControlTicket);

impl HostedControlTicket for PendingControl {
    fn poll(&mut self) -> Option<Result<(), Refusal>> {
        self.0
            .poll()
            .map(|result| result.map_err(|_| Refusal::Internal))
    }
}

struct PendingLogs(shoop_carla::service::LogTicket);

impl HostedLogTicket for PendingLogs {
    fn poll(&mut self) -> Option<Result<Vec<HostedProcessorLog>, Refusal>> {
        self.0.poll().map(|result| {
            result
                .map(|logs| {
                    logs.into_iter()
                        .map(|log| HostedProcessorLog {
                            generation: log.generation,
                            stdout: log.stdout,
                            stderr: log.stderr,
                            stdout_dropped: log.stdout_dropped_bytes,
                            stderr_dropped: log.stderr_dropped_bytes,
                        })
                        .collect()
                })
                .map_err(|_| Refusal::Internal)
        })
    }
}

fn reservation(
    description: &EngineDescription,
    timing: PreparedTiming,
    limits: BufferLimits,
    isolated: bool,
    processors: &PreparedProcessors,
    banks: &[(CarlaChainType, ProcessorKind, u16)],
) -> Result<u64, Error> {
    let aux = HostedProcessor::additional_prepared_bytes(
        limits,
        description.global_midi_input_capacity()?,
    )?;
    let mut total = processors.reserved_bytes;
    for &(chain, _, count) in banks.iter().filter(|(_, _, count)| *count != 0) {
        let channels = usize::from(chain.audio_channels());
        let ports = PortCounts {
            audio_inputs: channels,
            audio_outputs: channels,
            midi_inputs: 1,
            midi_outputs: 1,
        };
        let adapter = limits.async_storage_bytes_for(ports, timing.quantum(), timing.timing())?;
        let transport = Worker::transport_bytes(chain, timing, limits)?;
        let audio =
            shoop_carla::native::CARLA_MAX_BUFFER_SIZE * 2 * channels * std::mem::size_of::<f32>();
        let ipc = if isolated {
            shoop_carla::carla_shared_memory::FILE_SIZE + audio
        } else {
            0
        };
        // A retired owner's replacement can be prepared before releasing its predecessor.
        let per_slot = adapter + transport + aux + 2 * (audio + ipc);
        total = (per_slot as u64)
            .checked_mul(u64::from(count) * 2)
            .and_then(|bytes| bytes.checked_add(total))
            .ok_or(Error::Capacity)?;
    }
    let capture = description.worker_capture_storage_bytes(timing.latency_frames())? as u64;
    if total.saturating_add(capture) > processors.effective.memory_bytes {
        return Err(Error::Capacity);
    }
    Ok(total)
}
