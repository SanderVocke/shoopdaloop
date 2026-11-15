//! Ordered command execution and synchronous results, without legacy scheduling.
use crate::{
    node::{EngineNode, EngineOp},
    pool::Pool,
    ring::Ring,
};
use rt_runtime::immediate::{self, Boundary, Handler};
use shoop_loops::runtime::{Immediate, ImmediateGrab, LoopPlanner};
use shoop_rt_protocol::immediate::{
    Batch, Code, Command, CommandRecord, Diagnostics, Header, Response, COMMANDS, FIFO_RECORDS,
    STAGED_BATCHES,
};
#[derive(Clone, Copy, Debug)]
pub(crate) enum Incoming {
    Request(u64, shoop_rt_protocol::Request),
    Batch(Batch),
}
pub(crate) struct Control {
    tx: immediate::Producer<Header, CommandRecord>,
    pub incoming: Ring<Incoming>,
    waiting: usize,
    published: u32,
    retired: u32,
    frame: u64,
    discard: u32,
    pub reconcile: bool,
    observed: bool,
    diagnostics: Diagnostics,
    sent: Option<(Diagnostics, u64)>,
}
struct Render {
    rx: immediate::Consumer<Header, CommandRecord>,
    pool: Pool,
}
pub(crate) fn prepare(
    pool: Pool,
    credits: usize,
) -> (Control, impl Handler<EngineNode, LoopPlanner>) {
    let (tx, rx) = immediate::paired_channel(FIFO_RECORDS);
    (
        Control {
            tx,
            incoming: Ring::new(credits + STAGED_BATCHES),
            waiting: 0,
            published: 0,
            retired: 0,
            frame: 0,
            discard: 0,
            reconcile: false,
            observed: false,
            diagnostics: Diagnostics::default(),
            sent: None,
        },
        Render { rx, pool },
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
        let mut records = [CommandRecord::new(Command::Start(0)); COMMANDS];
        for (record, &command) in records.iter_mut().zip(batch.commands()) {
            *record = CommandRecord::new(command);
        }
        if self
            .tx
            .push(batch.header(), &records[..batch.commands().len()])
            .is_err()
        {
            return false;
        }
        self.published = self.published.wrapping_add(1);
        self.reconcile = true;
        self.observed = true;
        true
    }
    pub fn settled_frame(&self) -> Option<u64> {
        (self.retired == self.published).then_some(self.frame)
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
        self.discard = self.published.wrapping_sub(self.retired);
    }
    pub fn clock(&self) -> immediate::Clock {
        self.tx.clock()
    }

    pub fn responses(&mut self, out: &mut shoop_rt_transport::carrier::Outbox, connected: bool) {
        while let Some(group) = self.tx.response() {
            let header = group.header;
            let mut records = [CommandRecord::new(Command::Start(0)); COMMANDS];
            let mut count = 0;
            for &record in group.commands() {
                records[count] = record;
                count += 1;
            }
            if connected && self.discard == 0 {
                let response =
                    Response::new(header, &records[..count]).expect("processed response");
                if out
                    .push(shoop_rt_protocol::EngineMessage::ImmediateResponse(
                        response,
                    ))
                    .is_err()
                {
                    group.leave_unread();
                    break;
                }
            }
            if self.discard > 0 {
                self.discard -= 1;
            }
            let mut record_failure = |code: Code| {
                if let Some(code) = code.diagnostic() {
                    self.diagnostics.rt_refused = self.diagnostics.rt_refused.saturating_add(1);
                    self.diagnostics.last_rt = code;
                }
            };
            if header.code() != Code::Applied {
                record_failure(header.code());
            } else {
                for record in &records[..count] {
                    record_failure(record.code());
                }
            }
            self.frame = header.frame();
            self.retired = self.retired.wrapping_add(1);
        }
    }
    pub fn project(&mut self, out: &mut shoop_rt_transport::carrier::Outbox, frame: u64) {
        if !self.observed && self.diagnostics.control_refused == 0 {
            return;
        }
        let d = self.diagnostics;
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
fn loop_command(command: Command) -> Option<(u32, Immediate)> {
    match command {
        Command::LoopGain(id, v) => Some((id, Immediate::Gain(v))),
        Command::LoopBalance(id, v) => Some((id, Immediate::Balance(v))),
        Command::Start(id) => Some((id, Immediate::Playback(true))),
        Command::Stop(id) => Some((id, Immediate::Playback(false))),
        Command::Record(id) => Some((id, Immediate::Record)),
        _ => None,
    }
}
impl Render {
    fn node(pool: &Pool, command: Command) -> Result<(rt_runtime::NodeId, EngineOp), Code> {
        use Command::*;
        let (node, op) = match command {
            InputGain(id, v) if (id as usize) < pool.strips() => {
                (pool.strip_input(id as usize), EngineOp::Gain(v))
            }
            InputBalance(id, v) if (id as usize) < pool.strips() => {
                (pool.strip_input(id as usize), EngineOp::Balance(v))
            }
            OutputGain(id, v) if (id as usize) < pool.strips() => {
                (pool.strip_output(id as usize), EngineOp::Gain(v))
            }
            OutputBalance(id, v) if (id as usize) < pool.strips() => {
                (pool.strip_output(id as usize), EngineOp::Balance(v))
            }
            OutputMute(id, v) if (id as usize) < pool.strips() => {
                (pool.strip_output(id as usize), EngineOp::Mute(v))
            }
            BusGain(id, v) if (id as usize) < pool.buses() => {
                (pool.bus(id as usize), EngineOp::Gain(v))
            }
            BusBalance(id, v) if (id as usize) < pool.buses() => {
                (pool.bus(id as usize), EngineOp::Balance(v))
            }
            BusMute(id, v) if (id as usize) < pool.buses() => {
                (pool.bus(id as usize), EngineOp::Mute(v))
            }
            _ => return Err(Code::TargetUnavailable),
        };
        Ok((node, op))
    }
    fn execute(
        pool: &Pool,
        command: Command,
        boundary: &mut Boundary<'_, EngineNode, LoopPlanner>,
    ) -> Result<(), Code> {
        if !command.valid() {
            return Err(Code::InvalidValue);
        }
        if let Command::Grab(id, _) | Command::GrabGesture(id, _) = command {
            let resolve = |id| {
                boundary
                    .planner
                    .resolver()
                    .id_at_slot(id)
                    .ok_or(Code::TargetUnavailable)
            };
            let target = resolve(id)?;
            if id as usize >= pool.loops()
                || !boundary
                    .processors
                    .get(pool.fader(id as usize))
                    .is_some_and(EngineNode::immediate_playback_supported)
            {
                return Err(Code::Unsupported);
            }
            let grab = match command {
                Command::Grab(_, value) => {
                    use shoop_rt_protocol::immediate::GrabMode;
                    ImmediateGrab::Window {
                        sync: value.sync.map(resolve).transpose()?,
                        window: shoop_loops::grab::WindowSpec {
                            reverse_start_cycle: value.reverse_start_cycle,
                            cycles_length: value.cycles_length,
                            go_to_cycle: value.go_to_cycle,
                            continue_recording: value.mode == GrabMode::Recording,
                        },
                        mode: match value.mode {
                            GrabMode::Stopped => shoop_loops::Mode::Stopped,
                            GrabMode::Playing => shoop_loops::Mode::Playing,
                            GrabMode::Recording => shoop_loops::Mode::Recording,
                        },
                    }
                }
                Command::GrabGesture(_, value) => ImmediateGrab::Gesture {
                    sync: resolve(value.sync)?,
                    target: value.target.map(resolve).transpose()?,
                    spec: shoop_loops::grab::GestureSpec {
                        synced: true,
                        cycles: value.cycles,
                        play_after_record: value.play_after_record,
                    },
                },
                _ => unreachable!(),
            };
            return boundary
                .planner
                .grab_immediate(target, grab, boundary.frame, &mut boundary.resources)
                .map_err(|error| match error {
                    rt_runtime::Error::InvalidOffset => Code::HistoryUnavailable,
                    rt_runtime::Error::Capacity | rt_runtime::Error::Exhausted => {
                        Code::CaptureUnavailable
                    }
                    rt_runtime::Error::InvalidParameter => Code::InvalidValue,
                    rt_runtime::Error::InvalidId => Code::TargetUnavailable,
                    _ => Code::CannotApply,
                });
        }
        if let Some((id, op)) = loop_command(command) {
            let target = boundary
                .planner
                .resolver()
                .id_at_slot(id)
                .ok_or(Code::TargetUnavailable)?;
            if (id as usize) >= pool.loops()
                || boundary
                    .processors
                    .get(pool.playback(id as usize))
                    .is_none()
            {
                return Err(Code::TargetUnavailable);
            }
            if matches!(op, Immediate::Playback(_) | Immediate::Record)
                && !boundary
                    .processors
                    .get(pool.fader(id as usize))
                    .is_some_and(EngineNode::immediate_playback_supported)
            {
                return Err(Code::Unsupported);
            }
            boundary
                .planner
                .apply_immediate_capture(target, op, boundary.frame, &mut boundary.resources)
                .map_err(|error| match error {
                    rt_runtime::Error::InvalidId => Code::TargetUnavailable,
                    rt_runtime::Error::Capacity | rt_runtime::Error::Exhausted => {
                        Code::CaptureUnavailable
                    }
                    _ => Code::CannotApply,
                })
        } else {
            let (node, op) = Self::node(pool, command)?;
            boundary
                .processors
                .get_mut(node)
                .ok_or(Code::TargetUnavailable)?
                .apply_signal(op, boundary.graph)
                .map_err(|_| Code::Unsupported)
        }
    }
}
impl Handler<EngineNode, LoopPlanner> for Render {
    fn begin_callback(&mut self, frame: rt_runtime::Frame) {
        self.rx.publish_frame(frame);
        self.rx.begin_callback();
    }
    fn end_callback(&mut self, frame: rt_runtime::Frame) {
        self.rx.publish_frame(frame);
    }
    fn apply(&mut self, mut boundary: Boundary<'_, EngineNode, LoopPlanner>) {
        while let Some(group) = self.rx.pop() {
            let header = group.header;
            let code = if (header.graph(), header.topology())
                != (boundary.graph.0, boundary.planner.resolver().epoch().0)
            {
                Code::StaleConfiguration
            } else if header
                .deadline()
                .is_some_and(|deadline| boundary.frame.0 > deadline)
            {
                Code::DeadlineMissed
            } else if !boundary.available {
                Code::CannotApply
            } else {
                Code::Applied
            };
            group.respond(header.complete(boundary.frame.0, code), |record| {
                let outcome = if code == Code::Applied {
                    Self::execute(&self.pool, record.command(), &mut boundary)
                        .err()
                        .unwrap_or(Code::Applied)
                } else {
                    code
                };
                record.complete(outcome)
            });
        }
    }
}
