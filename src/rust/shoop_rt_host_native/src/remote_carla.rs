use crate::{carla, CarlaHostOptions};
use rt_buffered::{BufferLimits, PreparedTiming, Timing};
use rt_runtime::{graph::PortCounts, Error, Quantum};
use shoop_remote_fx::{Action, Client, Config, Kind, Lifecycle, Ticket};
use shoop_rt_engine::{
    DynamicCarla, EngineDescription, HostedCheckpointTicket, HostedControlTicket, HostedLogTicket,
    HostedProcessorControls, HostedProcessorObservation, PreparedProcessor,
};
use shoop_rt_protocol::{
    processors::{
        CarlaHosting, HostedFailure, HostedLifecycle, HostedStatus, RemoteFxReconnect,
        RemoteFxTarget,
    },
    ProcessorKind, ProcessorSlot, Refusal,
};
use std::collections::BTreeMap;

pub(crate) struct Providers {
    remote: Controls,
    local: carla::Controls,
    options: CarlaHostOptions,
    sample_rate: u32,
    limits: BufferLimits,
    local_timing: Option<PreparedTiming>,
    remote_timing: PreparedTiming,
    default_target: Option<RemoteFxTarget>,
    copies: u8,
    legacy_remote: bool,
}

impl Providers {
    pub(crate) fn new(
        description: &EngineDescription,
        period: usize,
        options: CarlaHostOptions,
    ) -> Result<(DynamicCarla, Self), Error> {
        let quantum = Quantum::new(description.quantum)?;
        let batch = period.min(8192).div_ceil(quantum.frames()).max(1);
        let timing = Timing {
            batch_quanta: batch,
            slack_quanta: batch * 2,
        };
        let remote_timing = Timing {
            slack_quanta: options
                .remote
                .and_then(|remote| remote.slack_quanta)
                .map_or(timing.slack_quanta, usize::from),
            ..timing
        }
        .prepare(quantum)?;
        let legacy_remote = description.processors.carla.hosting == CarlaHosting::Remote
            && options.remote.is_some();
        let isolated = options.worker.is_some()
            && description.processors.carla.hosting == CarlaHosting::Subprocess;
        let local_available =
            !legacy_remote && shoop_carla::native::carla_runtime_availability().is_ok();
        let local_timing = local_available
            .then(|| timing.prepare(quantum))
            .transpose()?;
        let dynamic = DynamicCarla {
            local: if legacy_remote {
                Some(remote_timing.timing())
            } else {
                local_timing.map(PreparedTiming::timing)
            },
            remote: Some(remote_timing.timing()),
        };
        let default_target = options.remote.map(|remote| RemoteFxTarget {
            address: remote.address,
            reconnect: match (remote.retain_chain, remote.auto_reconnect) {
                (_, false) => RemoteFxReconnect::Manual,
                (true, true) => RemoteFxReconnect::Resume,
                (false, true) => RemoteFxReconnect::Recreate,
            },
            dry_on_loss: remote.dry_on_loss,
        });
        let copies = options
            .remote
            .map_or(shoop_remote_fx::DEFAULT_COPIES, |remote| remote.copies);
        Ok((
            dynamic,
            Self {
                remote: Controls::default(),
                local: carla::Controls::new(local_available, isolated),
                options,
                sample_rate: description.sample_rate,
                limits: description.processor_buffer_limits(ProcessorKind::CarlaRack)?,
                local_timing,
                remote_timing,
                default_target,
                copies,
                legacy_remote,
            },
        ))
    }

    fn at(&self, index: u16) -> &dyn HostedProcessorControls {
        if self.remote.clients.contains_key(&index) {
            &self.remote
        } else {
            &self.local
        }
    }
}

#[derive(Default)]
struct Controls {
    clients: BTreeMap<u16, Client>,
}

impl Controls {
    fn prepare(
        &mut self,
        index: u16,
        kind: ProcessorKind,
        target: RemoteFxTarget,
        sample_rate: u32,
        timing: PreparedTiming,
        limits: BufferLimits,
        copies: u8,
    ) -> Result<PreparedProcessor, Refusal> {
        let chain = match carla::chain_type(kind)? {
            shoop_plugin_protocol::CarlaChainType::Rack => Kind::Rack,
            shoop_plugin_protocol::CarlaChainType::Patchbay => Kind::Patchbay,
            shoop_plugin_protocol::CarlaChainType::Patchbay16x => Kind::Patchbay16x,
        };
        let channels = usize::from(chain.channels());
        let ports = PortCounts {
            audio_inputs: channels,
            audio_outputs: channels,
            midi_inputs: 1,
            midi_outputs: 1,
        };
        let config = Config::new(sample_rate, timing, ports, limits, copies)
            .map_err(|_| Refusal::InvalidValue)?;
        let (endpoint, client) =
            Client::prepare_slot(chain, config, Some(target)).map_err(|error| {
                eprintln!("Prepare remote Carla: {error}");
                Refusal::Internal
            })?;
        self.clients.insert(index, client);
        Ok(PreparedProcessor {
            kind,
            backend: Box::new(endpoint),
            timing: timing.timing(),
        })
    }

    fn client(&self, index: u16) -> Result<&Client, Refusal> {
        self.clients.get(&index).ok_or(Refusal::InvalidId)
    }
}

impl HostedProcessorControls for Controls {
    fn claim_ready(&self, slot: shoop_rt_protocol::ProcessorSlot) -> Result<bool, Refusal> {
        let Ok(client) = self.client(slot.index) else {
            return Ok(true);
        };
        match client.claimed(slot.generation) {
            Some(true) => Ok(true),
            Some(false) => Err(Refusal::NotConnected),
            None => Ok(false),
        }
    }
    fn has_editors(&self) -> bool {
        true
    }

    fn has_logs(&self) -> bool {
        false
    }

    fn has_checkpoints(&self) -> bool {
        true
    }

    fn status(&self, index: u16) -> Option<HostedProcessorObservation> {
        let observed = self.client(index).ok()?.observation();
        Some(HostedProcessorObservation {
            owner: observed.owner,
            status: HostedStatus {
                generation: observed.generation,
                lifecycle: match observed.status.lifecycle {
                    Lifecycle::Stopped => HostedLifecycle::Stopped,
                    Lifecycle::Starting => HostedLifecycle::Starting,
                    Lifecycle::Running => HostedLifecycle::Running,
                    Lifecycle::Degraded => HostedLifecycle::Degraded,
                    Lifecycle::Crashed => HostedLifecycle::Crashed,
                    Lifecycle::Restarting => HostedLifecycle::Restarting,
                    Lifecycle::Unavailable => HostedLifecycle::Unavailable,
                },
                active: observed.status.active,
                failure: if !observed.connected {
                    Some(HostedFailure::Disconnected)
                } else if observed.status.lifecycle == Lifecycle::Crashed {
                    Some(HostedFailure::UnexpectedExit)
                } else {
                    None
                },
                editor_visible: observed.status.editor_visible,
            },
        })
    }

    fn set_visible(&self, index: u16, visible: bool) -> Result<(), Refusal> {
        let client = self.client(index)?;
        let observed = client.observation();
        client
            .request(observed.owner, observed.generation, Action::Editor(visible))
            .map_err(|_| Refusal::NotConnected)?
            .wait()
            .map(|_| ())
            .map_err(|_| Refusal::Internal)
    }

    fn recover(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        let ticket = self
            .client(index)?
            .recover(owner, generation)
            .map_err(|_| Refusal::Busy)?;
        Ok(Box::new(Control(ticket)))
    }

    fn clear_logs(
        &self,
        _index: u16,
        _owner: u64,
        _generation: u64,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        Err(Refusal::Unsupported)
    }

    fn read_logs(
        &self,
        _index: u16,
        _owner: u64,
        _generation: u64,
    ) -> Result<Box<dyn HostedLogTicket>, Refusal> {
        Err(Refusal::Unsupported)
    }

    fn capture_checkpoint(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedCheckpointTicket>, Refusal> {
        let ticket = self
            .client(index)?
            .request(owner, generation, Action::Save)
            .map_err(|_| Refusal::Busy)?;
        Ok(Box::new(Checkpoint(ticket)))
    }

    fn restore_candidate(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
        state: String,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        let ticket = self
            .client(index)?
            .request(owner, generation, Action::Restore(state))
            .map_err(|_| Refusal::Busy)?;
        Ok(Box::new(Control(ticket)))
    }
}

struct Control(Ticket);

impl HostedControlTicket for Control {
    fn poll(&mut self) -> Option<Result<(), Refusal>> {
        self.0
            .poll()
            .map(|result| result.map(|_| ()).map_err(|_| Refusal::Internal))
    }
}

struct Checkpoint(Ticket);

impl HostedCheckpointTicket for Checkpoint {
    fn poll(&mut self) -> Option<Result<String, Refusal>> {
        self.0.poll().map(|result| {
            result
                .map_err(|_| Refusal::Internal)?
                .ok_or(Refusal::Internal)
        })
    }
}

impl HostedProcessorControls for Providers {
    fn has_default_remote_target(&self) -> bool {
        self.default_target.is_some()
    }
    fn has_editors(&self) -> bool {
        self.local.has_editors() || self.remote.has_editors()
    }
    fn has_logs(&self) -> bool {
        self.local.has_logs()
    }
    fn has_checkpoints(&self) -> bool {
        self.local.has_checkpoints() || self.remote.has_checkpoints()
    }
    fn status(&self, index: u16) -> Option<HostedProcessorObservation> {
        self.at(index).status(index)
    }
    fn claim_ready(&self, slot: shoop_rt_protocol::ProcessorSlot) -> Result<bool, Refusal> {
        self.at(slot.index).claim_ready(slot)
    }
    fn prepare_dynamic_claim(
        &mut self,
        slot: ProcessorSlot,
        kind: ProcessorKind,
        target: Option<RemoteFxTarget>,
    ) -> Result<PreparedProcessor, Refusal> {
        if kind.is_remote_carla() || self.legacy_remote {
            let target = target
                .or(self.default_target)
                .ok_or(Refusal::NotConnected)?;
            self.remote.prepare(
                slot.index,
                kind,
                target,
                self.sample_rate,
                self.remote_timing,
                self.limits,
                self.copies,
            )
        } else {
            let timing = self.local_timing.ok_or(Refusal::Unsupported)?;
            self.local.prepare(
                slot.index,
                kind,
                self.sample_rate,
                timing,
                self.limits,
                &self.options,
            )
        }
    }

    fn release_dynamic_claim(&mut self, index: u16) {
        self.remote.clients.remove(&index);
        self.local.release(index);
    }
    fn set_visible(&self, index: u16, visible: bool) -> Result<(), Refusal> {
        self.at(index).set_visible(index, visible)
    }
    fn recover(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        self.at(index).recover(index, owner, generation)
    }
    fn clear_logs(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        self.at(index).clear_logs(index, owner, generation)
    }
    fn read_logs(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedLogTicket>, Refusal> {
        self.at(index).read_logs(index, owner, generation)
    }
    fn capture_checkpoint(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
    ) -> Result<Box<dyn HostedCheckpointTicket>, Refusal> {
        self.at(index).capture_checkpoint(index, owner, generation)
    }
    fn restore_candidate(
        &self,
        index: u16,
        owner: u64,
        generation: u64,
        state: String,
    ) -> Result<Box<dyn HostedControlTicket>, Refusal> {
        self.at(index)
            .restore_candidate(index, owner, generation, state)
    }
}
