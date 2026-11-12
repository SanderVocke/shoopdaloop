//! A replayable queue producer and differential observer; neither compiles hardware images.
use crate::{
    SimResult, Simulator,
    mono::{self, Native},
};
use rt_fpga_adapter::looping::{Controller, LoopAdapter, Operation, Prepared, Request};
use rt_runtime::{
    Epoch, Frame, Q824,
    graph::{self, Catalog},
    resource::{self, Cursor},
    schedule::{Action as Scheduled, Execution},
    transport::{Lane, Ticket},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shoop_loops::{
    self as loops, media,
    runtime::{Cut, Operation as LoopOperation, Reply, Selection},
};
use std::collections::BTreeMap;
fn queues() -> usize {
    32
}
fn receipts() -> usize {
    16
}
fn chunks() -> u32 {
    128
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub format_version: u32,
    pub name: String,
    #[serde(default = "queues")]
    pub command_slots: usize,
    #[serde(default = "receipts")]
    pub receipt_slots: usize,
    #[serde(default = "chunks")]
    pub chunk_limit: u32,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub input: Vec<i32>,
    pub commands: Vec<Command>,
    #[serde(default)]
    pub hold_results: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    #[serde(default)]
    pub at: Option<u64>,
    #[serde(default)]
    pub recovery: bool,
    #[serde(flatten)]
    pub action: Action,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Record,
    Stop,
    Play,
    State,
    Gain {
        target: i32,
    },
    Export {
        #[serde(default)]
        playback: bool,
    },
    Cursor {
        export: String,
    },
    Read {
        cursor: String,
        bytes: usize,
    },
    Release {
        export: String,
    },
    Cancel {
        request: String,
    },
    CancelTicket {
        request: String,
    },
}
#[derive(Debug, Serialize)]
pub struct FrameOutput {
    pub frame: u64,
    pub samples: Vec<i32>,
    pub state: String,
    pub events: Vec<String>,
    pub submissions: Vec<Value>,
    pub completions: Vec<Value>,
    pub executions: Vec<Value>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub format_version: u32,
    pub name: String,
    pub frames: Vec<FrameOutput>,
    pub compilations: u64,
    pub gain_writes: u64,
    pub instructions: usize,
    pub state_words: usize,
    pub image_bytes: usize,
    pub compile_micros: u128,
    pub configure_micros: u128,
    pub clocks: u64,
    pub read_bytes: usize,
}
struct Producer {
    tickets: BTreeMap<String, Ticket>,
    commands: BTreeMap<u64, Command>,
    exports: BTreeMap<String, media::ExportId>,
    cursors: BTreeMap<String, Cursor>,
}
impl Producer {
    fn new() -> Self {
        Self {
            tickets: BTreeMap::new(),
            commands: BTreeMap::new(),
            exports: BTreeMap::new(),
            cursors: BTreeMap::new(),
        }
    }
    fn send(
        &mut self,
        control: &mut Controller,
        client: &loops::runtime::LoopClient,
        command: Command,
        frame: u64,
    ) -> SimResult<Value> {
        if let Action::CancelTicket { request } = &command.action {
            self.tickets
                .get(request)
                .ok_or("unknown cancellation ticket")?
                .cancel();
            return Ok(json!({"name":command.name,"ticket_cancelled":request}));
        }
        let operation = match &command.action {
            Action::Record => Some(LoopOperation::Transition {
                target: mono::loop_id(),
                timing: loops::Timing::At,
                transition: loops::Transition::Record {
                    monitor_previous: false,
                },
            }),
            Action::Stop => Some(LoopOperation::Transition {
                target: mono::loop_id(),
                timing: loops::Timing::At,
                transition: loops::Transition::Stop,
            }),
            Action::Play => Some(LoopOperation::Transition {
                target: mono::loop_id(),
                timing: loops::Timing::At,
                transition: loops::Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            }),
            Action::State => Some(LoopOperation::State {
                target: mono::loop_id(),
            }),
            Action::Export { playback } => Some(LoopOperation::Export {
                target: mono::loop_id(),
                selection: if *playback {
                    Selection::Playback
                } else {
                    Selection::Latest
                },
            }),
            Action::Cursor { export } => Some(LoopOperation::Cursor {
                export: *self.exports.get(export).ok_or("unknown export")?,
                midi: false,
            }),
            Action::Release { export } => Some(LoopOperation::Release {
                export: *self.exports.get(export).ok_or("unknown export")?,
            }),
            Action::Cancel { request } => Some(LoopOperation::Cancel {
                sequence: self
                    .tickets
                    .get(request)
                    .ok_or("unknown request sequence")?
                    .sequence(),
            }),
            _ => None,
        };
        let at = Frame(command.at.unwrap_or(frame));
        let request = if let Some(op) = operation {
            client.command(at, op)
        } else {
            match &command.action {
                Action::Gain { target } => Request::Schedule {
                    at,
                    action: Scheduled::Processor {
                        node: mono::node_id(2),
                        operation: Operation {
                            target: Q824(*target),
                            duration: 0,
                        },
                    },
                    payload: Box::new([]),
                },
                Action::Read { cursor, bytes } => Request::resource(resource::Request::Read {
                    cursor: *self.cursors.get(cursor).ok_or("unknown cursor")?,
                    bytes: vec![0xa5; *bytes].into_boxed_slice(),
                }),
                _ => return Err("unsupported replay command".into()),
            }
        };
        let lane = if command.recovery {
            Lane::Recovery
        } else {
            Lane::Normal
        };
        match control.submit(lane, request) {
            Ok(ticket) => {
                let sequence = ticket.sequence().0;
                let output = json!({"name":command.name,"sequence":sequence});
                self.tickets.insert(command.name.clone(), ticket);
                self.commands.insert(sequence, command);
                Ok(output)
            }
            Err(error) => {
                let mut value = json!({"name":command.name,"refused":format!("{:?}",error.error)});
                if let Request::Resource(request) = error.value {
                    if let resource::Request::Read { bytes, .. } = request.into_request() {
                        value["returned_buffer"] = json!(bytes.as_ref());
                    }
                }
                Ok(value)
            }
        }
    }
    fn collect(
        &mut self,
        control: &mut Controller,
    ) -> SimResult<(
        Vec<Value>,
        Vec<Execution<LoopOperation<Q824>, Reply<Q824>, Operation, ()>>,
    )> {
        let mut completions = vec![];
        while let Some(completion) = control.try_completion() {
            let name = self
                .commands
                .get(&completion.sequence.0)
                .map(|c| c.name.as_str())
                .unwrap_or("_initial_commit");
            let mut value = json!({"name":name,"sequence":completion.sequence.0,"lane":format!("{:?}",completion.lane),"outcome":format!("{:?}",completion.outcome),"frame":completion.frame.0,"epoch":completion.epoch.0,"retired":completion.retired.is_some()});
            if let Request::Resource(request) = completion.request {
                value["reply"] = json!(format!("{:?}", request.reply()));
                if let Some(resource::Reply::Read { written, eof }) = request.reply() {
                    value["written"] = json!(written);
                    value["eof"] = json!(eof);
                }
                if let resource::Request::Read { cursor, bytes } = request.into_request() {
                    let command = self
                        .commands
                        .get(&completion.sequence.0)
                        .ok_or("unknown read receipt")?;
                    let Action::Read { cursor: name, .. } = &command.action else {
                        return Err("read receipt kind".into());
                    };
                    self.cursors.insert(name.clone(), cursor);
                    value["bytes"] = json!(bytes.as_ref());
                    value["position"] = json!(cursor.position());
                }
            }
            completions.push(value);
        }
        let mut executions = vec![];
        while let Some(execution) = control.try_execution() {
            let name = &self
                .commands
                .get(&execution.sequence.0)
                .ok_or("unknown execution sequence")?
                .name;
            match execution.operation_result {
                Some(Reply::Export { id, .. }) => {
                    self.exports.insert(name.clone(), id);
                }
                Some(Reply::Cursor(cursor)) => {
                    self.cursors.insert(name.clone(), cursor);
                }
                _ => {}
            }
            executions.push(execution);
        }
        Ok((completions, executions))
    }
}
#[derive(Default)]
struct Compare {
    media: Vec<(media::MediaId, media::MediaId)>,
    exports: Vec<(media::ExportId, media::ExportId)>,
    snapshots: Vec<(resource::SnapshotId, resource::SnapshotId)>,
    resources: Vec<(rt_runtime::ResourceId, rt_runtime::ResourceId)>,
    indexes: Vec<(resource::ReadIndex, resource::ReadIndex)>,
}
fn equal<T: PartialEq + std::fmt::Debug>(a: &T, b: &T, what: &str) -> SimResult<()> {
    if a != b {
        return Err(format!("{what}: native {a:?}, hardware {b:?}").into());
    }
    Ok(())
}
fn pair<T: PartialEq + Copy + std::fmt::Debug>(
    pairs: &mut Vec<(T, T)>,
    a: T,
    b: T,
) -> SimResult<usize> {
    if let Some(index) = pairs.iter().position(|p| p.0 == a || p.1 == b) {
        equal(&pairs[index], &(a, b), "handle correspondence")?;
        return Ok(index);
    }
    pairs.push((a, b));
    Ok(pairs.len() - 1)
}
impl Compare {
    fn media(&mut self, a: Option<media::MediaId>, b: Option<media::MediaId>) -> SimResult<Value> {
        match (a, b) {
            (Some(a), Some(b)) => Ok(json!(pair(&mut self.media, a, b)?)),
            (None, None) => Ok(Value::Null),
            _ => Err("media presence differs".into()),
        }
    }
    fn cut(&mut self, a: Cut<Q824>, b: Cut<Q824>) -> SimResult<Value> {
        equal(&a.state, &b.state, "query state")?;
        equal(
            &(
                a.raw_audio_frames,
                a.raw_midi_frames,
                a.windows,
                a.timing_revision,
                a.frame,
                a.graph_epoch,
                a.topology_epoch,
                a.gain,
                a.balance,
                a.smoothing,
                a.capture_failure,
                a.midi_overflows,
            ),
            &(
                b.raw_audio_frames,
                b.raw_midi_frames,
                b.windows,
                b.timing_revision,
                b.frame,
                b.graph_epoch,
                b.topology_epoch,
                b.gain,
                b.balance,
                b.smoothing,
                b.capture_failure,
                b.midi_overflows,
            ),
            "query cut",
        )?;
        Ok(
            json!({"frame":a.frame.0,"graph_epoch":a.graph_epoch.0,"topology_epoch":a.topology_epoch.0,"state":format!("{:?}",a.state),"raw_audio_frames":a.raw_audio_frames,"playback":self.media(a.playback,b.playback)?,"latest":self.media(a.latest,b.latest)?}),
        )
    }
    fn reply(&mut self, a: Option<Reply<Q824>>, b: Option<Reply<Q824>>) -> SimResult<Value> {
        Ok(match (a, b) {
            (None, None) => Value::Null,
            (Some(Reply::State(a)), Some(Reply::State(b))) => json!({"state":self.cut(a,b)?}),
            (Some(Reply::Transition(a)), Some(Reply::Transition(b))) => {
                equal(&a, &b, "transition completion")?;
                json!({"transition":format!("{a:?}")})
            }
            (
                Some(Reply::Export {
                    id: a,
                    cut: ca,
                    media: ma,
                }),
                Some(Reply::Export {
                    id: b,
                    cut: cb,
                    media: mb,
                }),
            ) => {
                let id = pair(&mut self.exports, a, b)?;
                self.media(Some(ma.media), Some(mb.media))?;
                equal(
                    &(
                        ma.requested_cut,
                        ma.view.windows,
                        ma.view.description,
                        ma.view.auxiliary,
                        ma.view.midi,
                    ),
                    &(
                        mb.requested_cut,
                        mb.view.windows,
                        mb.view.description,
                        mb.view.auxiliary,
                        mb.view.midi,
                    ),
                    "export metadata",
                )?;
                match (ma.view.audio, mb.view.audio) {
                    (Some(a), Some(b)) => {
                        pair(&mut self.snapshots, a.snapshot.id, b.snapshot.id)?;
                        pair(
                            &mut self.resources,
                            a.snapshot.resource,
                            b.snapshot.resource,
                        )?;
                        pair(&mut self.indexes, a.index, b.index)?;
                        equal(
                            &(
                                a.snapshot.revision,
                                a.snapshot.description,
                                a.snapshot.cut_frame,
                                a.snapshot.capture,
                            ),
                            &(
                                b.snapshot.revision,
                                b.snapshot.description,
                                b.snapshot.cut_frame,
                                b.snapshot.capture,
                            ),
                            "source metadata",
                        )?;
                    }
                    (None, None) => {}
                    _ => return Err("export source presence differs".into()),
                }
                json!({"export":id,"frames":ma.view.description.frames,"revision":ma.view.description.revision.0,"cut":self.cut(ca,cb)?})
            }
            (Some(Reply::Cursor(a)), Some(Reply::Cursor(b))) => {
                equal(&a.position(), &b.position(), "cursor position")?;
                json!({"cursor_position":a.position()})
            }
            (Some(a @ (Reply::Done | Reply::Failed(_) | Reply::Cancelled { .. })), Some(b)) => {
                equal(&a, &b, "domain reply")?;
                json!({"reply":format!("{a:?}")})
            }
            replies => return Err(format!("reply kinds differ {replies:?}").into()),
        })
    }
    fn executions(
        &mut self,
        a: Vec<Execution<LoopOperation<Q824>, Reply<Q824>, Operation, ()>>,
        b: Vec<Execution<LoopOperation<Q824>, Reply<Q824>, Operation, ()>>,
        producer: &Producer,
    ) -> SimResult<Vec<Value>> {
        equal(&a.len(), &b.len(), "execution count")?;
        a.into_iter().zip(b).map(|(a,b)| {
            equal(&(a.sequence,a.lane,a.requested_frame,a.delivered_frame,a.actual_frame,a.result,a.processor_result),&(b.sequence,b.lane,b.requested_frame,b.delivered_frame,b.actual_frame,b.result,b.processor_result),"execution receipt")?;
            Ok(json!({"name":producer.commands.get(&a.sequence.0).ok_or("unknown execution")?.name,"sequence":a.sequence.0,"requested":a.requested_frame.0,"delivered":a.delivered_frame.map(|v|v.0),"terminal":a.actual_frame.0,"result":format!("{:?}",a.result),"reply":self.reply(a.operation_result,b.operation_result)?}))
        }).collect()
    }
}
pub fn run(trace: &Trace, sim: Simulator<'_>) -> SimResult<Report> {
    if trace.format_version != 1
        || trace.name.is_empty()
        || trace.command_slots == 0
        || trace.receipt_slots == 0
        || trace.chunk_limit == 0
        || trace.chunk_limit > 128
        || trace.steps.is_empty()
        || trace.steps.iter().any(|s| s.input.len() != 32)
    {
        return Err("invalid mono trace profile".into());
    }
    let mut limits = mono::config().limits();
    limits.command_slots = trace.command_slots;
    limits.scheduled_events = trace.receipt_slots;
    let config = limits.prepare()?;
    let mut native = Native::with_config(config)?;
    let p = mono::prepare_with(config)?;
    let plan = graph::compile(
        &Catalog::new(
            config,
            &p.nodes.iter().map(Prepared::spec).collect::<Vec<_>>(),
        )?,
        &mono::graph(),
    )?;
    let mut startup = p.startup;
    startup.max_chunks = trace.chunk_limit;
    let (mut control, mut hardware) = LoopAdapter::prepare(config, startup, p.nodes, sim)?;
    control
        .submit(Lane::Normal, Request::commit(Epoch(0), plan))
        .map_err(|e| e.error)?;
    let mut a = Producer::new();
    let mut b = Producer::new();
    let mut compare = Compare::default();
    let mut frames = vec![];
    let mut read_bytes = 0;
    for (index, step) in trace.steps.iter().enumerate() {
        let frame = index as u64 * 32;
        let mut commands = step.commands.clone();
        commands.push(Command {
            name: format!("_state-{index}"),
            at: None,
            recovery: false,
            action: Action::State,
        });
        let mut submissions = vec![];
        for command in commands {
            if a.tickets.contains_key(&command.name) {
                return Err("duplicate command name".into());
            }
            let sent = a.send(&mut native.control, &native.client, command.clone(), frame)?;
            let peer = b.send(&mut control, &native.client, command, frame)?;
            equal(&sent, &peer, "submission")?;
            submissions.push(sent);
        }
        let expected = native.step(&step.input)?;
        let actual = hardware.process(&[step.input.clone()]).map_err(|e| {
            format!(
                "quantum {index}, hardware {e}: {:?}",
                hardware.device().last_error
            )
        })?;
        equal(
            &actual,
            &vec![expected.clone()],
            &format!("audio quantum {index}"),
        )?;
        equal(
            &hardware.state(),
            &native
                .runtime
                .planner()
                .resolver()
                .state(mono::loop_id())
                .map_err(|e| format!("{e:?}"))?,
            "post-quantum state",
        )?;
        equal(
            &hardware.events(),
            &native.runtime.planner().resolver().events(),
            "events",
        )?;
        equal(
            &(hardware.frame(), hardware.epoch()),
            &(native.runtime.next_frame(), native.runtime.epoch()),
            "clock/epoch",
        )?;
        let (completions, executions) = if step.hold_results {
            (vec![], vec![])
        } else {
            let (ac, ae) = a.collect(&mut native.control)?;
            let (bc, be) = b.collect(&mut control)?;
            equal(&ac, &bc, &format!("completions quantum {index}"))?;
            for c in &ac {
                if let Some(written) = c.get("written").and_then(Value::as_u64) {
                    read_bytes += written as usize;
                }
            }
            (
                ac,
                compare
                    .executions(ae, be, &a)
                    .map_err(|e| format!("quantum {index}: {e}"))?,
            )
        };
        frames.push(FrameOutput {
            frame,
            samples: expected,
            state: format!("{:?}", hardware.state()),
            events: hardware.events().iter().map(|e| format!("{e:?}")).collect(),
            submissions,
            completions,
            executions,
        });
    }
    if trace.steps.last().is_some_and(|s| s.hold_results)
        || native.control.outstanding() != 0
        || control.outstanding() != 0
    {
        return Err("trace must collect its final results".into());
    }
    if hardware.metrics.compilations != 1 {
        return Err("loop operations recompiled the program".into());
    }
    let m = &hardware.metrics;
    Ok(Report {
        format_version: 1,
        name: trace.name.clone(),
        frames,
        compilations: m.compilations,
        gain_writes: m.gain_writes,
        instructions: m.instructions,
        state_words: m.state_words,
        image_bytes: m.image_bytes,
        compile_micros: m.compile_time.as_micros(),
        configure_micros: m.configuration_time.as_micros(),
        clocks: hardware.device().cycles,
        read_bytes,
    })
}
