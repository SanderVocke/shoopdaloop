//! Explicit opt-in entry point; ordinary backend mutations retain their protocol.
use crate::{EngineClock, Link, RtBackend};
use anyhow::{anyhow, Result};
use shoop_backend::{BackendBusId, BackendLoopId, BackendTrackId};
use shoop_rt_protocol::immediate::{Batch, Command, Diagnostics, COMMANDS};
use shoop_rt_transport::carrier::Carrier;

#[derive(Clone, Copy, Debug)]
pub enum ImmediateCommand {
    LoopGain(BackendLoopId, f32),
    LoopBalance(BackendLoopId, f32),
    Start(BackendLoopId),
    Stop(BackendLoopId),
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
    /// Nonblocking, without request tracking. Control orders groups with legacy
    /// mutations at settled boundaries. Acceptance is local;
    /// observe conditions in snapshots, never infer an individual execution receipt.
    pub fn immediate_group(&mut self, commands: &[ImmediateCommand]) -> Result<()> {
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
                Start(id) => Command::Start(loop_slot(id)),
                Stop(id) => Command::Stop(loop_slot(id)),
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
            return Err(anyhow!(
                "earlier commands are waiting for transport capacity"
            ));
        }
        self.client
            .immediate(batch)
            .map_err(|e| anyhow!("immediate group locally refused: {e:?}"))
    }
    pub fn immediate_diagnostics(&self) -> Diagnostics {
        self.status.immediate
    }
}
