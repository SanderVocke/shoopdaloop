//! Explicit opt-in entry point; ordinary backend mutations retain their protocol.
use crate::{EngineClock, Link, RtBackend};
use anyhow::{anyhow, Result};
use shoop_backend::{
    BackendBusControl, BackendGrabRequest, BackendImmediateCommand, BackendLoopMode, TransitionMode,
};
use shoop_backend::{BackendBusId, BackendLoopId, BackendTrackId};
use shoop_rt_protocol::immediate::{
    Batch, Command, Diagnostics, TransitionMode as RtMode, COMMANDS,
};
use shoop_rt_transport::carrier::Carrier;

pub(crate) type ResponseHandler = dyn FnMut(&shoop_rt_protocol::immediate::Response) + Send;

#[derive(Clone, Copy, Debug)]
pub enum ImmediateCommand {
    LoopGain(BackendLoopId, f32),
    LoopBalance(BackendLoopId, f32),
    Transition(BackendLoopId, TransitionMode),
    Grab(BackendGrabRequest),
    GrabGesture {
        loop_id: BackendLoopId,
        sync: BackendLoopId,
        target: Option<BackendLoopId>,
        cycles: u32,
        play_after_record: bool,
    },
    InputGain(BackendTrackId, f32),
    InputBalance(BackendTrackId, f32),
    OutputGain(BackendTrackId, f32),
    OutputBalance(BackendTrackId, f32),
    OutputMute(BackendTrackId, bool),
    BusGain(BackendBusId, f32),
    BusBalance(BackendBusId, f32),
    BusMute(BackendBusId, bool),
}
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    /// Observes ordered RT results while pumping. The default consumer drains
    /// them without creating UI operations or retaining an unbounded backlog.
    /// A result can precede the snapshot covering its execution frame.
    pub fn set_immediate_response_handler(
        &mut self,
        handler: impl FnMut(&shoop_rt_protocol::immediate::Response) + Send + 'static,
    ) {
        self.immediate_response_handler = Some(alloc::boxed::Box::new(handler));
    }
    pub(crate) fn submit_ui_immediate(
        &mut self,
        commands: &[BackendImmediateCommand],
    ) -> Result<bool> {
        if commands.is_empty() || commands.len() > COMMANDS {
            return Err(anyhow!("immediate groups contain 1 to {COMMANDS} commands"));
        }
        let mut values = [ImmediateCommand::BusMute(BackendBusId::from_raw(0), false); COMMANDS];
        // Non-finite values remain individual RT failures.
        let bounded = |value: f32, min: f32, max: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                f32::NAN
            }
        };
        let amplitude = |value: f32| {
            if value.is_finite() {
                crate::mirror::db_amplitude(value)
            } else {
                f32::NAN
            }
        };
        for (output, command) in values.iter_mut().zip(commands) {
            use BackendImmediateCommand::*;
            *output = match *command {
                LoopGain(id, v) => ImmediateCommand::LoopGain(id, bounded(v, 0.0, 1.0)),
                LoopBalance(id, v) => ImmediateCommand::LoopBalance(id, bounded(v, -1.0, 1.0)),
                Transition(id, mode) => ImmediateCommand::Transition(id, mode),
                Grab(request) => ImmediateCommand::Grab(request),
                GrabGesture {
                    loop_id,
                    sync,
                    target,
                    cycles,
                    play_after_record,
                } => ImmediateCommand::GrabGesture {
                    loop_id,
                    sync,
                    target,
                    cycles,
                    play_after_record,
                },
                OutputGainDb(id, v) => ImmediateCommand::OutputGain(id, amplitude(v)),
                InputGainDb(id, v) => ImmediateCommand::InputGain(id, amplitude(v)),
                OutputBalance(id, v) => ImmediateCommand::OutputBalance(id, bounded(v, -1.0, 1.0)),
                InputBalance(id, v) => ImmediateCommand::InputBalance(id, bounded(v, -1.0, 1.0)),
                OutputMute(id, v) => ImmediateCommand::OutputMute(id, v),
                BusGainDb(id, v) => {
                    let gain = match BackendBusControl::GainDb(v).normalized(0) {
                        Ok(BackendBusControl::GainDb(value)) => amplitude(value),
                        _ => f32::NAN,
                    };
                    ImmediateCommand::BusGain(id, gain)
                }
                BusBalance(id, v) => {
                    let stereo = self
                        .mirror
                        .buses
                        .get(&id)
                        .is_some_and(|b| b.channels.len() == 2);
                    ImmediateCommand::BusBalance(
                        id,
                        if stereo {
                            bounded(v, -1.0, 1.0)
                        } else {
                            f32::NAN
                        },
                    )
                }
                BusMute(id, v) => ImmediateCommand::BusMute(id, v),
            };
        }
        self.try_immediate_group(&values[..commands.len()])
    }
    /// Nonblocking, without request tracking. Control orders groups with legacy
    /// mutations at settled boundaries. Acceptance is local;
    /// observe conditions in snapshots, never infer an individual execution receipt.
    pub fn immediate_group(&mut self, commands: &[ImmediateCommand]) -> Result<()> {
        if self.try_immediate_group(commands)? {
            Ok(())
        } else {
            Err(anyhow!("immediate group locally refused: transport full"))
        }
    }

    fn try_immediate_group(&mut self, commands: &[ImmediateCommand]) -> Result<bool> {
        if self.link != Link::Ready {
            return Err(anyhow!("immediate groups require a connected session"));
        }
        let (graph, topology) = self
            .snapshots
            .observed_configuration
            .ok_or_else(|| anyhow!("no confirmed snapshot configuration"))?;
        if commands.is_empty() || commands.len() > COMMANDS {
            return Err(anyhow!("immediate groups contain 1 to {COMMANDS} commands"));
        }
        let loop_slot = |id| {
            self.mirror
                .loops
                .get(&id)
                .map(|l| self.snapshots.loop_address(l.slot))
                .unwrap_or(u32::MAX)
        };
        let strip = |id| {
            self.mirror
                .tracks
                .get(&id)
                .map(|t| self.snapshots.strip_address(t.strip))
                .unwrap_or(u32::MAX)
        };
        let bus = |id| {
            self.mirror
                .buses
                .get(&id)
                .map(|b| self.snapshots.bus_address(b.slot))
                .unwrap_or(u32::MAX)
        };
        let mut values = [Command::BusMute(0, false); COMMANDS];
        for (output, command) in values.iter_mut().zip(commands) {
            use ImmediateCommand::*;
            *output = match *command {
                LoopGain(id, v) => Command::LoopGain(loop_slot(id), v),
                LoopBalance(id, v) => Command::LoopBalance(loop_slot(id), v),
                Transition(id, mode) => Command::Transition(
                    loop_slot(id),
                    match mode {
                        TransitionMode::Stopped => RtMode::Stopped,
                        TransitionMode::Playing => RtMode::Playing,
                        TransitionMode::Recording => RtMode::Recording,
                    },
                ),
                Grab(request) => {
                    use shoop_rt_protocol::immediate::Grab as Value;
                    let number = |v: Option<i32>| v.map(|v| u32::try_from(v).unwrap_or(u32::MAX));
                    let sync = self
                        .mirror
                        .loops
                        .get(&request.loop_id)
                        .and_then(|l| l.sync)
                        .map(loop_slot);
                    let valid_mode = matches!(
                        request.go_to_mode,
                        BackendLoopMode::Stopped
                            | BackendLoopMode::Playing
                            | BackendLoopMode::Recording
                    );
                    Command::Grab(
                        loop_slot(request.loop_id),
                        Value {
                            sync,
                            reverse_start_cycle: number(request.reverse_start_cycle),
                            cycles_length: if valid_mode {
                                number(request.cycles_length)
                            } else {
                                Some(u32::MAX)
                            },
                            go_to_cycle: number(request.go_to_cycle),
                            mode: match request.go_to_mode {
                                BackendLoopMode::Playing => RtMode::Playing,
                                BackendLoopMode::Recording => RtMode::Recording,
                                _ => RtMode::Stopped,
                            },
                        },
                    )
                }
                GrabGesture {
                    loop_id,
                    sync,
                    target,
                    cycles,
                    play_after_record,
                } => Command::GrabGesture(
                    loop_slot(loop_id),
                    shoop_rt_protocol::immediate::GrabGesture {
                        sync: loop_slot(sync),
                        target: target.map(loop_slot),
                        cycles,
                        play_after_record,
                    },
                ),
                InputGain(id, v) => Command::InputGain(strip(id), v),
                InputBalance(id, v) => Command::InputBalance(strip(id), v),
                OutputGain(id, v) => Command::OutputGain(strip(id), v),
                OutputBalance(id, v) => Command::OutputBalance(strip(id), v),
                OutputMute(id, v) => Command::OutputMute(strip(id), v),
                BusGain(id, v) => Command::BusGain(bus(id), v),
                BusBalance(id, v) => Command::BusBalance(bus(id), v),
                BusMute(id, v) => Command::BusMute(bus(id), v),
            };
        }
        let batch = Batch::new(graph, topology, &values[..commands.len()])
            .ok_or_else(|| anyhow!("invalid immediate group"))?;
        self.send();
        if !self.requests.outgoing.is_empty() {
            return Ok(false);
        }
        match self.client.immediate(batch) {
            Ok(()) => Ok(true),
            Err(shoop_rt_transport::client::ClientError::Carrier(
                shoop_rt_transport::carrier::CarrierError::Full,
            )) => Ok(false),
            Err(error) => Err(anyhow!("immediate group locally refused: {error:?}")),
        }
    }
    pub fn immediate_diagnostics(&self) -> Diagnostics {
        self.status.immediate
    }
}
