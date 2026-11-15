//! Dedicated fire-and-forget command execution; no legacy scheduling or receipts.
use crate::{
    node::{EngineNode, EngineOp},
    pool::Pool,
    ring::Ring,
};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
#[derive(Default)]
struct Progress {
    // The read cursor advances after application. Control also waits for a
    // snapshot covering that frame before releasing later legacy work.
    retired: AtomicU32,
    frame: AtomicU64,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Incoming {
    Request(u64, shoop_rt_protocol::Request),
    Batch(Batch),
}
pub(crate) struct Control {
    tx: immediate::Producer<Batch>,
    pub incoming: Ring<Incoming>,
    waiting: usize,
    published: u32,
    progress: Arc<Progress>,
    pub reconcile: bool,
    observed: bool,
    failures: Arc<Failures>,
    diagnostics: Diagnostics,
    sent: Option<(Diagnostics, u64)>,
}
struct Render {
    rx: immediate::Consumer<Batch>,
    pool: Pool,
    failures: Arc<Failures>,
    progress: Arc<Progress>,
}
pub(crate) fn prepare(
    pool: Pool,
    credits: usize,
) -> (Control, impl Handler<EngineNode, LoopPlanner>) {
    let (tx, rx) = immediate::channel(FIFO_BATCHES);
    let failures = Arc::new(Failures::default());
    let progress = Arc::new(Progress::default());
    (
        Control {
            tx,
            incoming: Ring::new(credits + STAGED_BATCHES),
            waiting: 0,
            published: 0,
            progress: progress.clone(),
            reconcile: false,
            observed: false,
            failures: failures.clone(),
            diagnostics: Diagnostics::default(),
            sent: None,
        },
        Render {
            rx,
            pool,
            failures,
            progress,
        },
    )
}
impl Control {
    pub fn full(&self) -> bool {
        self.waiting == STAGED_BATCHES
    }
    pub fn stage(&mut self, batch: Batch) {
        self.incoming
            .push(Incoming::Batch(batch))
            .expect("bounded inbox");
        self.waiting += 1;
    }
    pub fn pop(&mut self) -> Option<Incoming> {
        let incoming = self.incoming.pop()?;
        if matches!(incoming, Incoming::Batch(_)) {
            self.waiting -= 1;
        }
        Some(incoming)
    }
    pub fn publish(&mut self, batch: Batch) -> bool {
        if self.tx.push(batch).is_err() {
            return false;
        }
        self.published = self.published.wrapping_add(1);
        self.reconcile = true;
        self.observed = true;
        true
    }
    pub fn settled_frame(&self) -> Option<u64> {
        (self.progress.retired.load(Ordering::Acquire) == self.published)
            .then(|| self.progress.frame.load(Ordering::Relaxed))
    }
    pub fn busy(&self) -> bool {
        self.reconcile
    }
    pub fn refuse(&mut self, code: u8) {
        self.diagnostics.control_refused = self.diagnostics.control_refused.saturating_add(1);
        self.diagnostics.last_control = code;
    }
    pub fn disconnect(&mut self) {
        self.incoming.clear();
        self.waiting = 0;
        self.sent = None;
    }
    pub fn project(&mut self, out: &mut shoop_rt_transport::carrier::Outbox, frame: u64) {
        if !self.observed && self.diagnostics.control_refused == 0 {
            return;
        }
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
    fn node(pool: &Pool, command: Command) -> Result<(rt_runtime::NodeId, SlotId, EngineOp), u8> {
        use Command::*;
        let (id, slot, op) = match command {
            InputGain(id, v) if (id.0.index as usize) < pool.strips() => (
                pool.strip_input(id.0.index as usize),
                id.0,
                EngineOp::Gain(v),
            ),
            InputBalance(id, v) if (id.0.index as usize) < pool.strips() => (
                pool.strip_input(id.0.index as usize),
                id.0,
                EngineOp::Balance(v),
            ),
            OutputGain(id, v) if (id.0.index as usize) < pool.strips() => (
                pool.strip_output(id.0.index as usize),
                id.0,
                EngineOp::Gain(v),
            ),
            OutputBalance(id, v) if (id.0.index as usize) < pool.strips() => (
                pool.strip_output(id.0.index as usize),
                id.0,
                EngineOp::Balance(v),
            ),
            OutputMute(id, v) if (id.0.index as usize) < pool.strips() => (
                pool.strip_output(id.0.index as usize),
                id.0,
                EngineOp::Mute(v),
            ),
            BusGain(id, v) if (id.0.index as usize) < pool.buses() => {
                (pool.bus(id.0.index as usize), id.0, EngineOp::Gain(v))
            }
            BusBalance(id, v) if (id.0.index as usize) < pool.buses() => {
                (pool.bus(id.0.index as usize), id.0, EngineOp::Balance(v))
            }
            BusMute(id, v) if (id.0.index as usize) < pool.buses() => {
                (pool.bus(id.0.index as usize), id.0, EngineOp::Mute(v))
            }
            _ => return Err(3),
        };
        Ok((id, slot, op))
    }
    fn execute(
        pool: &Pool,
        command: Command,
        boundary: &mut Boundary<'_, EngineNode, LoopPlanner>,
    ) -> Result<(), u8> {
        if !command.valid() {
            return Err(5);
        }
        if let Some((id, op)) = loop_command(command) {
            let target = loop_id(id)?;
            if (id.0.index as usize) >= pool.loops()
                || boundary
                    .processors
                    .get(pool.playback(id.0.index as usize))
                    .is_none()
            {
                return Err(3);
            }
            if matches!(op, Immediate::Playback(_))
                && !boundary
                    .processors
                    .get(pool.fader(id.0.index as usize))
                    .is_some_and(EngineNode::immediate_playback_supported)
            {
                return Err(4);
            }
            boundary
                .planner
                .apply_immediate(target, op)
                .map_err(|error| {
                    if error == rt_runtime::Error::InvalidId {
                        3
                    } else {
                        4
                    }
                })
        } else {
            let (node, target, op) = Self::node(pool, command)?;
            if boundary
                .processors
                .get(node)
                .and_then(EngineNode::immediate_generation)
                != Some(target.generation)
            {
                return Err(3);
            }
            boundary
                .processors
                .get_mut(node)
                .ok_or(3)?
                .apply_signal(op, boundary.graph)
                .map_err(|_| 4)
        }
    }
}
impl Handler<EngineNode, LoopPlanner> for Render {
    fn begin_callback(&mut self) {
        self.rx.begin_callback();
    }
    fn apply(&mut self, mut boundary: Boundary<'_, EngineNode, LoopPlanner>) {
        while let Some(batch) = self.rx.pop() {
            if batch.graph != boundary.graph.0
                || batch.topology != boundary.planner.resolver().epoch().0
            {
                refuse(&self.failures, 2);
            } else {
                for &command in batch.commands() {
                    if let Err(code) = Self::execute(&self.pool, command, &mut boundary) {
                        refuse(&self.failures, code);
                    }
                }
            }
            self.progress
                .frame
                .store(boundary.frame.0, Ordering::Relaxed);
            let retired = self
                .progress
                .retired
                .load(Ordering::Relaxed)
                .wrapping_add(1);
            self.progress.retired.store(retired, Ordering::Release);
        }
    }
}

fn refuse(failures: &Failures, code: u8) {
    failures.last.store(u32::from(code), Ordering::Relaxed);
    let count = failures.count.load(Ordering::Relaxed);
    failures
        .count
        .store(count.saturating_add(1), Ordering::Relaxed);
}
