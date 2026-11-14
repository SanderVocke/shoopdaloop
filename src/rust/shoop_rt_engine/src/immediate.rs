//! Dedicated fire-and-forget command execution; no legacy scheduling or receipts.
use crate::{
    node::{EngineNode, EngineOp},
    pool::Pool,
    ring::Ring,
};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};
use rt_runtime::immediate::{self, Boundary, Handler};
use shoop_loops::{
    runtime::{Immediate, LoopPlanner},
    LoopId,
};
use shoop_rt_protocol::{
    immediate::{Batch, Command, Diagnostics, FIFO_BATCHES, STAGED_BATCHES},
    LoopSlotId, SlotId,
};

#[derive(Default)]
struct Failures {
    count: AtomicU32,
    last: AtomicU32,
}
pub(crate) struct Control {
    tx: immediate::Producer<Batch>,
    staged: Ring<Batch>,
    pub active: bool,
    failures: Arc<Failures>,
    diagnostics: Diagnostics,
    sent: Option<(Diagnostics, u64)>,
}
struct Render {
    rx: immediate::Consumer<Batch>,
    pool: Pool,
    failures: Arc<Failures>,
}
pub(crate) fn prepare(pool: Pool) -> (Control, impl Handler<EngineNode, LoopPlanner>) {
    let (tx, rx) = immediate::channel(FIFO_BATCHES);
    let failures = Arc::new(Failures::default());
    (
        Control {
            tx,
            staged: Ring::new(STAGED_BATCHES),
            active: false,
            failures: failures.clone(),
            diagnostics: Diagnostics::default(),
            sent: None,
        },
        Render { rx, pool, failures },
    )
}
impl Control {
    pub fn full(&self) -> bool {
        self.staged.is_full()
    }
    pub fn submit(&mut self, batch: Batch) {
        self.staged.push(batch).expect("checked staging capacity");
        self.advance();
    }
    pub fn advance(&mut self) {
        while let Some(batch) = self.staged.front() {
            if self.tx.push(batch).is_err() {
                break;
            }
            self.staged.pop();
        }
    }
    pub fn refuse(&mut self, code: u8) {
        self.diagnostics.control_refused = self.diagnostics.control_refused.saturating_add(1);
        self.diagnostics.last_control = code;
    }
    pub fn disconnect(&mut self) {
        self.staged.clear();
        self.sent = None;
    }
    pub fn project(&mut self, out: &mut shoop_rt_transport::carrier::Outbox, frame: u64) {
        let mut d = self.diagnostics;
        d.rt_refused = self.failures.count.load(Ordering::Relaxed);
        d.last_rt = self.failures.last.load(Ordering::Relaxed) as u8;
        if self.sent != Some((d, frame))
            && out
                .push(shoop_rt_protocol::EngineMessage::Telemetry(
                    shoop_rt_protocol::Telemetry::Immediate(d),
                ))
                .is_ok()
        {
            self.sent = Some((d, frame));
        }
    }
}
fn loop_command(command: Command) -> Option<(LoopSlotId, Immediate)> {
    match command {
        Command::LoopGain(id, v) => Some((id, Immediate::Gain(v))),
        Command::LoopBalance(id, v) => Some((id, Immediate::Balance(v))),
        Command::Start(id) => Some((id, Immediate::Playback(true))),
        Command::Stop(id) => Some((id, Immediate::Playback(false))),
        _ => None,
    }
}
fn loop_id(id: LoopSlotId) -> Result<LoopId, u8> {
    LoopId::from_parts(id.0.index, id.0.generation).map_err(|_| 3)
}
impl Render {
    fn node(&self, command: Command) -> Result<(rt_runtime::NodeId, SlotId, EngineOp), u8> {
        use Command::*;
        let (id, slot, op) = match command {
            InputGain(id, v) if (id.0.index as usize) < self.pool.strips() => (
                self.pool.strip_input(id.0.index as usize),
                id.0,
                EngineOp::Gain(v),
            ),
            InputBalance(id, v) if (id.0.index as usize) < self.pool.strips() => (
                self.pool.strip_input(id.0.index as usize),
                id.0,
                EngineOp::Balance(v),
            ),
            OutputGain(id, v) if (id.0.index as usize) < self.pool.strips() => (
                self.pool.strip_output(id.0.index as usize),
                id.0,
                EngineOp::Gain(v),
            ),
            OutputBalance(id, v) if (id.0.index as usize) < self.pool.strips() => (
                self.pool.strip_output(id.0.index as usize),
                id.0,
                EngineOp::Balance(v),
            ),
            OutputMute(id, v) if (id.0.index as usize) < self.pool.strips() => (
                self.pool.strip_output(id.0.index as usize),
                id.0,
                EngineOp::Mute(v),
            ),
            BusGain(id, v) if (id.0.index as usize) < self.pool.buses() => {
                (self.pool.bus(id.0.index as usize), id.0, EngineOp::Gain(v))
            }
            BusBalance(id, v) if (id.0.index as usize) < self.pool.buses() => (
                self.pool.bus(id.0.index as usize),
                id.0,
                EngineOp::Balance(v),
            ),
            BusMute(id, v) if (id.0.index as usize) < self.pool.buses() => {
                (self.pool.bus(id.0.index as usize), id.0, EngineOp::Mute(v))
            }
            _ => return Err(3),
        };
        Ok((id, slot, op))
    }
    fn validate(
        &self,
        batch: &Batch,
        boundary: &Boundary<'_, EngineNode, LoopPlanner>,
    ) -> Result<(), u8> {
        if batch.graph != boundary.graph.0
            || batch.topology != boundary.planner.resolver().epoch().0
        {
            return Err(2);
        }
        if !batch.valid() {
            return Err(5);
        }
        for &command in batch.commands() {
            if let Some((id, op)) = loop_command(command) {
                let target = loop_id(id)?;
                if (id.0.index as usize) >= self.pool.loops()
                    || boundary
                        .processors
                        .get(self.pool.playback(id.0.index as usize))
                        .is_none()
                    || boundary.planner.resolver().state(target).is_err()
                {
                    return Err(3);
                }
                boundary
                    .planner
                    .validate_immediate(target, op)
                    .map_err(|_| 4)?;
            } else {
                let (node, target, _) = self.node(command)?;
                if boundary
                    .processors
                    .get(node)
                    .and_then(EngineNode::immediate_generation)
                    != Some(target.generation)
                {
                    return Err(3);
                }
            }
        }
        Ok(())
    }
}
impl Handler<EngineNode, LoopPlanner> for Render {
    fn begin_callback(&mut self) {
        self.rx.begin_callback();
    }
    fn apply(&mut self, boundary: Boundary<'_, EngineNode, LoopPlanner>) {
        while let Some(batch) = self.rx.pop() {
            if let Err(code) = self.validate(&batch, &boundary) {
                self.failures.last.store(u32::from(code), Ordering::Relaxed);
                let count = self.failures.count.load(Ordering::Relaxed);
                self.failures
                    .count
                    .store(count.saturating_add(1), Ordering::Relaxed);
                continue;
            }
            for &command in batch.commands() {
                if let Some((id, op)) = loop_command(command) {
                    boundary
                        .planner
                        .apply_immediate(loop_id(id).expect("validated target"), op)
                        .expect("preflighted command");
                } else {
                    let (node, _, op) = self.node(command).expect("validated target");
                    boundary
                        .processors
                        .get_mut(node)
                        .expect("validated processor")
                        .apply_signal(op, boundary.graph)
                        .expect("preflighted command");
                }
            }
        }
    }
}
