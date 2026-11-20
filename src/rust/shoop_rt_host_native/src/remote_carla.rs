use crate::RemoteCarla;
use rt_buffered::{PreparedTiming, Timing};
use rt_runtime::{graph::PortCounts, Error, Quantum};
use shoop_remote_fx::{Action, Client, Config, Kind, Lifecycle, Ticket};
use shoop_rt_engine::{
    EngineDescription, HostedCheckpointTicket, HostedControlTicket, HostedLogTicket,
    HostedProcessor, HostedProcessorControls, HostedProcessorObservation, PreparedProcessor,
    PreparedProcessors,
};
use shoop_rt_protocol::{
    processors::{EffectiveCarla, HostedFailure, HostedLifecycle, HostedStatus, Unavailable},
    ProcessorKind, Refusal,
};

pub(crate) fn prepare(
    description: &EngineDescription,
    period: usize,
    remote: Option<RemoteCarla>,
    mut processors: PreparedProcessors,
) -> (PreparedProcessors, Vec<Client>) {
    match prepare_banks(description, period, remote, &processors) {
        Ok((instances, services, bytes, effective)) => {
            processors.instances.extend(instances);
            processors.reserved_bytes = bytes;
            processors.effective.carla = effective;
            (processors, services)
        }
        Err(reason) => {
            processors.effective.carla = EffectiveCarla::unavailable(reason);
            (processors, Vec::new())
        }
    }
}

type Prepared = (Vec<PreparedProcessor>, Vec<Client>, u64, EffectiveCarla);

fn prepare_banks(
    description: &EngineDescription,
    period: usize,
    remote: Option<RemoteCarla>,
    processors: &PreparedProcessors,
) -> Result<Prepared, Unavailable> {
    let remote = remote.ok_or(Unavailable::Remote)?;
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
        patchbay16x_instances: if description.strip_channels >= 16 {
            settings.patchbay16x_instances
        } else {
            0
        },
        unavailable: None,
    };
    if effective.instances() == 0 {
        return Err(Unavailable::ChannelCapacity);
    }
    let timing = timing(description.quantum, period, remote.slack_quanta)
        .map_err(|_| Unavailable::Memory)?;
    let limits = description
        .processor_buffer_limits(ProcessorKind::CarlaRack)
        .map_err(|_| Unavailable::Memory)?;
    let auxiliary = HostedProcessor::additional_prepared_bytes(
        limits,
        description
            .global_midi_input_capacity()
            .map_err(|_| Unavailable::Memory)?,
    )
    .map_err(|_| Unavailable::Memory)?;
    let banks = [
        (
            Kind::Rack,
            ProcessorKind::CarlaRack,
            effective.rack_instances,
        ),
        (
            Kind::Patchbay,
            ProcessorKind::CarlaPatchbay,
            effective.patchbay_instances,
        ),
        (
            Kind::Patchbay16x,
            ProcessorKind::CarlaPatchbay16x,
            effective.patchbay16x_instances,
        ),
    ];
    let mut configurations = Vec::new();
    let mut bytes = processors.reserved_bytes;
    for (chain, kind, count) in banks.into_iter().filter(|(_, _, count)| *count > 0) {
        let channels = usize::from(chain.channels());
        let ports = PortCounts {
            audio_inputs: channels,
            audio_outputs: channels,
            midi_inputs: 1,
            midi_outputs: 1,
        };
        let config = Config::new(
            description.sample_rate,
            timing,
            ports,
            limits,
            remote.copies,
        )
        .map_err(|_| Unavailable::Memory)?;
        let adapter = limits
            .async_storage_bytes_for(ports, timing.quantum(), timing.timing())
            .map_err(|_| Unavailable::Memory)?;
        let transfer = shoop_remote_fx::transport_bytes(config).map_err(|_| Unavailable::Memory)?;
        let slot =
            (adapter + transfer + auxiliary + shoop_rt_protocol::MAX_PROCESSOR_CHECKPOINT_BYTES)
                as u64;
        bytes = bytes
            .checked_add(slot * u64::from(count) * 2)
            .ok_or(Unavailable::Memory)?;
        configurations.push((chain, kind, count, config));
    }
    let capture = description
        .worker_capture_storage_bytes(timing.latency_frames())
        .map_err(|_| Unavailable::Memory)? as u64;
    if bytes.saturating_add(capture) > processors.effective.memory_bytes {
        return Err(Unavailable::Memory);
    }
    let slots = effective.instances() as usize * 2;
    let mut instances = Vec::with_capacity(slots);
    let mut services = Vec::with_capacity(slots);
    for _ in 0..2 {
        for &(chain, kind, count, config) in &configurations {
            for _ in 0..count {
                let (endpoint, client) =
                    Client::prepare(remote.address, chain, config).map_err(|error| {
                        eprintln!("Remote Carla at {}: {error}", remote.address);
                        Unavailable::Remote
                    })?;
                instances.push(PreparedProcessor {
                    kind,
                    backend: Box::new(endpoint),
                    timing: timing.timing(),
                });
                services.push(client);
            }
        }
    }
    Ok((instances, services, bytes, effective))
}

fn timing(quantum: usize, period: usize, slack: Option<u16>) -> Result<PreparedTiming, Error> {
    let quantum = Quantum::new(quantum)?;
    let batch = period.div_ceil(quantum.frames()).max(1);
    Timing {
        batch_quanta: batch,
        slack_quanta: slack.map(usize::from).unwrap_or(batch * 2),
    }
    .prepare(quantum)
}

pub(crate) struct Controls {
    first: usize,
    clients: Vec<Client>,
}

impl Controls {
    pub(crate) fn new(first: usize, clients: Vec<Client>) -> Self {
        Self { first, clients }
    }

    fn client(&self, index: u16) -> Result<&Client, Refusal> {
        usize::from(index)
            .checked_sub(self.first)
            .and_then(|index| self.clients.get(index))
            .ok_or(Refusal::InvalidId)
    }
}

impl HostedProcessorControls for Controls {
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
