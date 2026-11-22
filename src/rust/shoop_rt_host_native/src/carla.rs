use crate::CarlaHostOptions;
use rt_buffered::{BufferLimits, PreparedTiming};
use shoop_carla::service::{ChainControl, ChainMonitor, ChainService, ControlTicket};
use shoop_plugin_protocol::{CarlaChainType, CarlaProcessorLifecycle, ChainId, WorkerExitKind};
use shoop_rt_engine::{
    HostedCheckpointTicket, HostedControlTicket, HostedLogTicket, HostedProcessorControls,
    HostedProcessorLog, HostedProcessorObservation, PreparedProcessor,
};
use shoop_rt_protocol::{
    processors::{HostedFailure, HostedLifecycle, HostedStatus},
    ProcessorKind, Refusal,
};
use std::{collections::BTreeMap, time::Duration};

pub(crate) struct Controls {
    chains: BTreeMap<u16, Chain>,
    available: bool,
    isolated: bool,
}

struct Chain {
    monitor: ChainMonitor,
    control: ChainControl,
    service: ChainService,
}

pub(crate) fn chain_type(kind: ProcessorKind) -> Result<CarlaChainType, Refusal> {
    Ok(match kind {
        ProcessorKind::CarlaRack | ProcessorKind::RemoteCarlaRack => CarlaChainType::Rack,
        ProcessorKind::CarlaPatchbay | ProcessorKind::RemoteCarlaPatchbay => {
            CarlaChainType::Patchbay
        }
        ProcessorKind::CarlaPatchbay16x | ProcessorKind::RemoteCarlaPatchbay16x => {
            CarlaChainType::Patchbay16x
        }
        _ => return Err(Refusal::Unsupported),
    })
}

impl Controls {
    pub(crate) fn new(available: bool, isolated: bool) -> Self {
        Self {
            chains: BTreeMap::new(),
            available,
            isolated,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        index: u16,
        kind: ProcessorKind,
        sample_rate: u32,
        timing: PreparedTiming,
        limits: BufferLimits,
        options: &CarlaHostOptions,
    ) -> Result<PreparedProcessor, Refusal> {
        let chain = chain_type(kind)?;
        let result = if self.isolated {
            let worker = options.worker.clone().ok_or(Refusal::Unsupported)?;
            ChainService::start_isolated(
                worker,
                ChainId(u64::from(index) + 1),
                chain,
                sample_rate,
                timing,
                limits,
            )
        } else {
            ChainService::start_in_process(chain, sample_rate, timing, limits, options.ui.clone())
        };
        let (endpoint, service) = result.map_err(|error| {
            eprintln!("Could not start local Carla: {error:#}");
            Refusal::Unsupported
        })?;
        self.chains.insert(
            index,
            Chain {
                monitor: service.monitor(),
                control: service.control(),
                service,
            },
        );
        Ok(PreparedProcessor {
            kind,
            backend: Box::new(endpoint),
            timing: timing.timing(),
        })
    }

    pub(crate) fn release(&mut self, index: u16) {
        if let Some(chain) = self.chains.remove(&index) {
            let _ = chain.service.stop(Duration::from_secs(2));
        }
    }

    fn chain(&self, index: u16) -> Option<&Chain> {
        self.chains.get(&index)
    }
}

impl HostedProcessorControls for Controls {
    fn has_editors(&self) -> bool {
        self.available
    }

    fn has_logs(&self) -> bool {
        self.isolated
    }

    fn has_checkpoints(&self) -> bool {
        self.available
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

    fn restore_candidate(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
        state: String,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        let chain = self.chain(index).ok_or(Refusal::Unsupported)?;
        let ticket = chain
            .control
            .restore_candidate(owner, generation, state)
            .map_err(|_| Refusal::Internal)?;
        Ok(Box::new(PendingCandidate(ticket)))
    }
}

struct PendingCandidate(ControlTicket);

impl HostedControlTicket for PendingCandidate {
    fn poll(&mut self) -> Option<Result<(), Refusal>> {
        self.0
            .poll()
            .map(|result| result.map_err(|_| Refusal::InvalidValue))
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
