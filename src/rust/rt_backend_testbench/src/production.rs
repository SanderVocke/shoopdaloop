//! Assembly and observation only: graph, timing, processing and media are production.
use crate::{spec as wire, validation::Checked};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Control, GraphRuntime, PreparedNode},
    graph::{self, GraphDescription, PortCounts, PortType},
    processor::{OperationContext, ProcessBlock, Processor},
    resource::{self, ResourceStore},
    transport::Lane,
    Config, Frame, NodeId, PlannerId, Quantum, Sample, Q824,
};
use shoop_loops::{
    self as loops,
    audio::Smoothing,
    runtime::{self as domain, LoopNode, LoopPlanner},
};
pub use shoop_rt_engine::dsp::GainOperation;
use shoop_rt_engine::dsp::GainProcessor;
use std::collections::BTreeMap;

fn checked<T, E: std::fmt::Debug>(v: std::result::Result<T, E>) -> Checked<T> {
    v.map_err(|e| format!("{e:?}"))
}
pub fn node_id(id: wire::Identity) -> Checked<NodeId> {
    checked(NodeId::from_parts(id.slot, id.generation))
}
pub fn loop_id(id: wire::Identity) -> Checked<loops::LoopId> {
    checked(loops::LoopId::from_parts(id.slot, id.generation))
}
fn raw<S: OracleSample>(value: i32) -> S {
    S::from_raw(value)
}

pub trait OracleSample: Sample {
    const NUMERIC: wire::Numeric;
    fn from_raw(value: i32) -> Self;
    fn samples(channels: Vec<Vec<Self>>) -> wire::Samples;
    fn begin_quantum(_frame: u64) {}
    fn end_quantum() {}
}
impl OracleSample for Q824 {
    const NUMERIC: wire::Numeric = wire::Numeric::Q824;
    fn from_raw(value: i32) -> Self {
        Q824(value)
    }
    fn samples(channels: Vec<Vec<Self>>) -> wire::Samples {
        wire::Samples::Q824(
            channels
                .into_iter()
                .map(|c| c.into_iter().map(|v| v.0).collect())
                .collect(),
        )
    }
}
impl OracleSample for f32 {
    const NUMERIC: wire::Numeric = wire::Numeric::Float;
    fn from_raw(value: i32) -> Self {
        Q824(value).to_f32()
    }
    fn samples(channels: Vec<Vec<Self>>) -> wire::Samples {
        wire::Samples::Float(channels)
    }
}

pub enum Node<S: Sample> {
    Loop(LoopNode<S>),
    Gain(GainProcessor<S>),
}
impl<S: Sample> Processor<LoopPlanner<S>, GainOperation<S>, S> for Node<S> {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(node) => node.ports(),
            Self::Gain(node) => PortCounts {
                audio_inputs: node.channels(),
                audio_outputs: node.channels(),
                ..Default::default()
            },
        }
    }
    fn admit(
        &self,
        id: NodeId,
        planner: &LoopPlanner<S>,
    ) -> std::result::Result<(), rt_runtime::Error> {
        match self {
            Self::Loop(node) => node.admit(id, planner),
            _ => Ok(()),
        }
    }
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner<S>, S>,
    ) -> std::result::Result<(), rt_runtime::Error> {
        match self {
            Self::Loop(node) => node.process(block),
            Self::Gain(node) => node.process(block),
        }
    }
    fn operation(
        &mut self,
        operation: GainOperation<S>,
        _: OperationContext<'_, S>,
    ) -> std::result::Result<(), rt_runtime::Error> {
        match self {
            Self::Gain(node) => {
                node.apply(operation);
                Ok(())
            }
            _ => Err(rt_runtime::Error::InvalidEvent),
        }
    }
}
pub type Runtime<S> = GraphRuntime<Node<S>, LoopPlanner<S>, GainOperation<S>, S>;
pub type Controller<S> = Control<Node<S>, LoopPlanner<S>, GainOperation<S>, S>;
pub struct Prepared<S: OracleSample> {
    pub runtime: Runtime<S>,
    pub control: Controller<S>,
    pub client: domain::LoopClient,
    pub ordered: graph::PlanDescriptor,
}

pub fn prepare<S: OracleSample>(scenario: &wire::Scenario) -> Checked<Prepared<S>> {
    scenario.validate()?;
    let q = checked(Quantum::new(scenario.quantum as usize))?;
    let slots = scenario
        .loops
        .iter()
        .map(|l| l.id.slot as usize + 1)
        .max()
        .unwrap_or(1);
    let requests = scenario.request_capacity as usize;
    let node_slots = scenario
        .nodes
        .iter()
        .map(|n| n.id.slot as usize + 1)
        .max()
        .unwrap_or(1);
    let channels = scenario
        .loops
        .iter()
        .map(|l| l.channels as usize)
        .max()
        .unwrap_or(1);
    let sample_count = scenario
        .steps
        .len()
        .checked_mul(scenario.quantum as usize)
        .and_then(|v| v.checked_mul(channels))
        .and_then(|v| v.checked_mul(scenario.loops.len().max(1)))
        .ok_or("capture capacity overflow")?;
    let media_samples: usize = scenario
        .media
        .iter()
        .flat_map(|m| &m.channels)
        .map(Vec::len)
        .sum();
    let config = checked(
        Config {
            max_frames: scenario.quantum as usize,
            audio_inputs: scenario.input_ports as usize,
            audio_outputs: scenario.output_ports as usize,
            midi_inputs: 0,
            midi_outputs: 0,
            max_nodes: node_slots,
            max_ports: scenario
                .nodes
                .iter()
                .map(|n| match n.operation {
                    wire::NodeKind::Gain { channels, .. } => 2 * channels as usize,
                    wire::NodeKind::Playback { target } | wire::NodeKind::Capture { target } => {
                        scenario
                            .loops
                            .iter()
                            .find(|l| l.id == target)
                            .unwrap()
                            .channels as usize
                    }
                })
                .sum::<usize>()
                .max(1)
                + scenario.input_ports as usize
                + scenario.output_ports as usize,
            max_edges: scenario.connections.len().max(1),
            resource_slots: (scenario.loops.len() * 12 + scenario.media.len() * 4).max(32),
            resource_pages: ((sample_count + media_samples) * 4 / 128 + slots * 16).max(64),
            page_bytes: 128,
            scheduled_events: requests,
            budgets: rt_runtime::Budgets {
                commands: 256,
                events: requests.max(1),
                publications: 256,
                ..Default::default()
            },
            command_slots: 256,
            ..Default::default()
        }
        .prepare(),
    )?;
    let mut resources = checked(ResourceStore::<S>::with_capture_groups(
        config,
        q,
        None,
        resource::CaptureGroupConfig {
            groups: (2 * scenario.loops.len()).max(1),
            members_per_group: 1,
            staging_bytes_per_group: channels * scenario.quantum as usize * 4,
        },
    ))?;
    let topology = topology(scenario, slots, requests + 1)?;
    let bindings = scenario
        .loops
        .iter()
        .map(|l| {
            let playback = scenario
                .nodes
                .iter()
                .find(|n| matches!(n.operation,wire::NodeKind::Playback { target } if target==l.id))
                .unwrap();
            let capture = scenario
                .nodes
                .iter()
                .find(|n| matches!(n.operation,wire::NodeKind::Capture { target } if target==l.id))
                .unwrap();
            Ok(domain::Binding {
                target: loop_id(l.id)?,
                playback: node_id(playback.id)?,
                capture: node_id(capture.id)?,
                channels: l.channels as usize,
                auxiliary_channels: 0,
                dry_wet: false,
                midi: false,
                gain: S::ONE,
                smoothing: Smoothing::None,
                midi_events: 1,
                midi_bytes: 4,
                packet_bytes: 4,
            })
        })
        .collect::<Checked<Vec<_>>>()?;
    let mut imports = Vec::new();
    for l in &scenario.loops {
        let Some(content) = &l.content else { continue };
        let media = scenario
            .media
            .iter()
            .find(|m| m.name == content.media)
            .unwrap();
        let start = content.start_chunks as usize * scenario.chunk_frames as usize;
        let frames = content.length_chunks * scenario.chunk_frames as u64;
        let bytes: Vec<u8> = (0..frames as usize)
            .flat_map(|i| {
                media
                    .channels
                    .iter()
                    .flat_map(move |c| raw::<S>(c[start + i]).to_le_bytes())
            })
            .collect();
        let resource = checked(resources.create())?;
        let upload = checked(resources.begin_upload(
            resource,
            rt_runtime::Revision(0),
            resource::Description {
                format: resource::Format::Audio {
                    channels: l.channels as u32,
                },
                frames,
                bytes: bytes.len(),
            },
        ))?;
        for (index, page) in bytes.chunks(128).enumerate() {
            resources.begin_callback();
            checked(resources.write_upload(upload, index * 128, page))?;
        }
        checked(resources.finish_upload(upload))?;
        checked(resources.commit(upload))?;
        let snapshot = checked(resources.snapshot(resource, Frame(0)))?;
        let index = checked(resources.read_index(snapshot.id))?;
        imports.push(loops::media::Input {
            description: loops::media::Description {
                owner: loop_id(l.id)?,
                revision: rt_runtime::Revision(0),
                frames,
                channels: l.channels as u32,
                auxiliary_channels: 0,
                midi: false,
            },
            audio: Some(loops::media::Source { snapshot, index }),
            auxiliary: None,
            midi: None,
        });
    }
    let domain::Prepared {
        mut planner,
        nodes,
        client,
    } = checked(LoopPlanner::prepare(
        config,
        q,
        topology,
        &mut resources,
        domain::Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: requests,
            media_slots: (slots * 8).max(8),
            export_slots: (scenario
                .steps
                .iter()
                .flat_map(|s| &s.actions)
                .filter(|a| matches!(a, wire::Action::Export { .. }))
                .count()
                + 1)
            .max(4),
        },
        bindings,
        imports,
        domain::Options::default(),
    ))?;
    for l in &scenario.loops {
        checked(planner.prepare_repeat_sync(loop_id(l.id)?, l.repeat_sync))?;
    }
    let mut nodes: Vec<_> = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, Node::Loop(node)))
        .collect();
    for node in &scenario.nodes {
        if let wire::NodeKind::Gain { channels, initial } = node.operation {
            nodes.push(PreparedNode::new(
                node_id(node.id)?,
                Node::Gain(GainProcessor::new(
                    channels as usize,
                    scenario.quantum as usize,
                    raw::<S>(initial),
                )),
            ));
        }
    }
    let graph = signal_graph(scenario)?;
    let (mut control, runtime) = checked(Runtime::prepare_with_initial_graph(
        config, q, nodes, planner, resources, &graph,
    ))?;
    checked(control.prepare_submissions())?;
    let ordered = checked(graph::compile(runtime.catalog(), &graph))?
        .descriptor()
        .clone();
    Ok(Prepared {
        runtime,
        control,
        client,
        ordered,
    })
}
pub fn topology(
    scenario: &wire::Scenario,
    slots: usize,
    requests: usize,
) -> Checked<loops::PreparedTopology> {
    checked(loops::compile(
        loops::Limits {
            loop_slots: slots,
            pending_requests: requests,
            journal_events: slots * 2 + requests,
            ..Default::default()
        },
        &loops::TopologyDescription {
            loops: scenario
                .loops
                .iter()
                .map(|l| {
                    Ok(loops::LoopSpec {
                        id: loop_id(l.id)?,
                        kind: loops::LoopKind::Primitive,
                        length: l
                            .content
                            .as_ref()
                            .map_or(0, |c| c.length_chunks * u64::from(scenario.chunk_frames)),
                        sync_source: l.sync_source.map(loop_id).transpose()?,
                    })
                })
                .collect::<Checked<_>>()?,
            ..Default::default()
        },
    ))
}
pub fn signal_graph(scenario: &wire::Scenario) -> Checked<GraphDescription> {
    fn endpoint(ep: wire::Endpoint) -> Checked<graph::Endpoint> {
        Ok(match ep.node {
            Some(id) => graph::Endpoint::node(node_id(id)?, PortType::Audio, ep.port as u32),
            None => graph::Endpoint::host(PortType::Audio, ep.port as u32),
        })
    }
    Ok(GraphDescription {
        nodes: scenario
            .nodes
            .iter()
            .map(|n| node_id(n.id))
            .collect::<Checked<_>>()?,
        connections: scenario
            .connections
            .iter()
            .map(|c| {
                Ok(graph::Connection {
                    source: endpoint(c.source)?,
                    destination: endpoint(c.destination)?,
                })
            })
            .collect::<Checked<_>>()?,
    })
}

fn transition(value: wire::Transition) -> loops::Transition {
    match value {
        wire::Transition::Stop => loops::Transition::Stop,
        wire::Transition::Record => loops::Transition::Record {
            monitor_previous: false,
        },
        wire::Transition::Play {
            position,
            retrigger,
        } => loops::Transition::Play {
            position,
            retrigger,
        },
        wire::Transition::Seek {
            position,
            retrigger,
        } => loops::Transition::Seek {
            position,
            retrigger,
        },
    }
}
fn timing(value: wire::Timing) -> Checked<loops::Timing> {
    Ok(match value {
        wire::Timing::At => loops::Timing::At,
        wire::Timing::AfterTicks { source, skip } => loops::Timing::AfterTicks {
            source: loop_id(source)?,
            skip,
        },
    })
}
fn selection(value: wire::Selection) -> domain::Selection {
    match value {
        wire::Selection::Playback => domain::Selection::Playback,
        wire::Selection::Latest => domain::Selection::Latest,
        wire::Selection::Recording => domain::Selection::Recording,
    }
}
fn state(value: loops::State) -> wire::LoopState {
    wire::LoopState {
        id: wire::Identity {
            slot: value.id.slot(),
            generation: value.id.generation(),
        },
        mode: format!("{:?}", value.mode).to_lowercase(),
        content_length: value.content_length,
        position: value.position,
        capture_frames: value.capture_frames,
        cycles: value.cycles,
        content_revision: value.content_revision.0,
        capture_revision: value.capture_revision.0,
        last_pulse: value.last_pulse.map(|f| f.0),
        repeat_sync: value.repeat_sync,
    }
}
fn error_name(error: impl std::fmt::Debug) -> String {
    let source = format!("{error:?}");
    let mut result = String::new();
    for (i, c) in source.chars().enumerate() {
        if c.is_uppercase() && i != 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}
fn outcome(value: loops::Outcome) -> String {
    match value {
        loops::Outcome::Applied { .. } => "applied".into(),
        loops::Outcome::Superseded { .. } => "superseded".into(),
        loops::Outcome::Cancelled { .. } => "cancelled".into(),
        loops::Outcome::Failed { reason, .. } => format!("failed:{}", error_name(reason)),
    }
}
struct Observer<S: OracleSample> {
    names: BTreeMap<u64, String>,
    sequences: BTreeMap<String, rt_runtime::Sequence>,
    exports: Vec<(String, loops::media::Export)>,
    _sample: std::marker::PhantomData<S>,
}
impl<S: OracleSample> Observer<S> {
    fn collect(&mut self, control: &mut Controller<S>) -> Checked<Vec<wire::ActionResult>> {
        let mut values = Vec::new();
        while let Some(execution) = control.try_execution() {
            let name = self
                .names
                .get(&execution.sequence.0)
                .ok_or("unknown execution identity")?
                .clone();
            let result = match execution.result {
                Err(error) => format!("rejected:{error:?}"),
                Ok(()) => match execution.operation_result {
                    Some(domain::Reply::Transition(c)) => outcome(c.outcome),
                    Some(domain::Reply::Failed(error)) => format!(
                        "failed:{}",
                        match error {
                            domain::Failure::Timeline(e) => error_name(e),
                            domain::Failure::Runtime(e) => error_name(e),
                        }
                    ),
                    Some(domain::Reply::Export { media, .. }) => {
                        self.exports.push((name.clone(), media));
                        "applied".into()
                    }
                    Some(domain::Reply::Cancelled { .. }) => {
                        if matches!(
                            execution.action,
                            rt_runtime::schedule::Action::Planner {
                                operation: domain::Operation::Cancel { .. },
                                ..
                            }
                        ) {
                            "applied".into()
                        } else {
                            "cancelled".into()
                        }
                    }
                    Some(domain::Reply::Done) | None => "applied".into(),
                    Some(reply) => return Err(format!("unexpected production reply {reply:?}")),
                },
            };
            values.push(wire::ActionResult {
                name,
                requested: execution.requested_frame.0,
                delivered: execution.delivered_frame.map(|f| f.0),
                terminal: execution.actual_frame.0,
                outcome: result,
            });
        }
        Ok(values)
    }
}
pub fn run<S: OracleSample>(scenario: &wire::Scenario) -> Checked<wire::Result> {
    run_monitored::<S, _>(scenario, &mut Direct)
}

pub trait ProcessMonitor {
    fn process<T>(&mut self, run: impl FnOnce() -> T) -> T;
}
struct Direct;
impl ProcessMonitor for Direct {
    fn process<T>(&mut self, run: impl FnOnce() -> T) -> T {
        run()
    }
}

pub fn run_monitored<S: OracleSample, M: ProcessMonitor>(
    scenario: &wire::Scenario,
    monitor: &mut M,
) -> Checked<wire::Result> {
    let Prepared {
        mut runtime,
        mut control,
        client,
        ..
    } = prepare::<S>(scenario)?;
    let mut observer = Observer::<S> {
        names: BTreeMap::new(),
        sequences: BTreeMap::new(),
        exports: Vec::new(),
        _sample: std::marker::PhantomData,
    };
    let mut result = wire::Result {
        format_version: wire::FORMAT_VERSION,
        name: scenario.name.clone(),
        numeric: S::NUMERIC,
        quanta: Vec::new(),
        media: Vec::new(),
    };
    for step in &scenario.steps {
        let mut admissions = Vec::new();
        while let Some(completion) = control.try_completion() {
            if let rt_runtime::transport::Outcome::Rejected(error) = completion.outcome {
                let name = observer
                    .names
                    .get(&completion.sequence.0)
                    .ok_or("unknown rejected command")?
                    .clone();
                let requested = match completion.request {
                    rt_runtime::executor::Command::Schedule { at, .. } => at.0,
                    _ => return Err("unexpected admission refusal".into()),
                };
                admissions.push(wire::ActionResult {
                    name,
                    requested,
                    delivered: None,
                    terminal: completion.frame.0,
                    outcome: format!("rejected:{}", error_name(error)),
                });
            }
        }
        let mut outcomes = if step.collect_completed {
            observer.collect(&mut control)?
        } else {
            Vec::new()
        };
        outcomes.extend(admissions);
        let frame = runtime.next_frame();
        for action in &step.actions {
            if let wire::Action::Export {
                target, selection, ..
            } = action
            {
                let state = checked(runtime.planner().resolver().state(loop_id(*target)?))?;
                if (state.mode == loops::Mode::Recording)
                    != matches!(selection, wire::Selection::Recording)
                {
                    return Err("unsupported export cut: use recording selection while recording, and playback/latest at a completed-take boundary".into());
                }
                if state.mode != loops::Mode::Recording && state.content_length == 0 {
                    return Err("unsupported export cut: no published content".into());
                }
            }
            let (name, command) = match action {
                wire::Action::Transition {
                    name,
                    target,
                    at,
                    timing: when,
                    operation,
                } => (
                    name,
                    client.command(
                        Frame(*at),
                        domain::Operation::Transition {
                            target: loop_id(*target)?,
                            timing: timing(*when)?,
                            transition: transition(*operation),
                        },
                    ),
                ),
                wire::Action::Gain {
                    name,
                    node,
                    at,
                    target,
                    duration,
                } => (
                    name,
                    rt_runtime::executor::Command::Schedule {
                        at: Frame(*at),
                        action: rt_runtime::schedule::Action::Processor {
                            node: node_id(*node)?,
                            operation: GainOperation {
                                target: raw::<S>(*target),
                                duration: *duration,
                            },
                        },
                        payload: Box::new([]),
                    },
                ),
                wire::Action::Cancel { name, request } => {
                    let sequence = *observer
                        .sequences
                        .get(request)
                        .ok_or("cancel refers to unknown action")?;
                    (
                        name,
                        client.command(frame, domain::Operation::Cancel { sequence }),
                    )
                }
                wire::Action::Export {
                    name,
                    target,
                    selection: which,
                } => (
                    name,
                    client.command(
                        frame,
                        domain::Operation::Export {
                            target: loop_id(*target)?,
                            selection: selection(*which),
                        },
                    ),
                ),
            };
            match control.submit_prepared(Lane::Normal, command) {
                Ok(ticket) => {
                    observer.names.insert(ticket.sequence().0, name.clone());
                    observer.sequences.insert(name.clone(), ticket.sequence());
                }
                Err(error) => outcomes.push(wire::ActionResult {
                    name: name.clone(),
                    requested: frame.0,
                    delivered: None,
                    terminal: frame.0,
                    outcome: format!("rejected:{:?}", error.error),
                }),
            }
        }
        let inputs: Vec<Vec<S>> = step
            .inputs
            .iter()
            .map(|c| c.iter().map(|&v| raw::<S>(v)).collect())
            .collect();
        let mut outputs =
            vec![vec![S::ZERO; scenario.quantum as usize]; scenario.output_ports as usize];
        let input_refs = inputs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let mut output_refs = outputs
            .iter_mut()
            .map(Vec::as_mut_slice)
            .collect::<Vec<_>>();
        S::begin_quantum(frame.0);
        checked(monitor.process(|| {
            runtime.process(
                AudioBlock {
                    frame,
                    frames: scenario.quantum as usize,
                    inputs: &input_refs,
                    outputs: &mut output_refs,
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        }))?;
        S::end_quantum();
        let states = scenario
            .loops
            .iter()
            .map(|l| checked(runtime.planner().resolver().state(loop_id(l.id)?)).map(state))
            .collect::<Checked<_>>()?;
        let events = runtime
            .planner()
            .resolver()
            .events()
            .iter()
            .map(|event| {
                let kind = match event.kind {
                    loops::EventKind::Transition(value) => {
                        wire::EventKind::Transition(match value {
                            loops::Transition::Stop => wire::Transition::Stop,
                            loops::Transition::Record {
                                monitor_previous: false,
                            } => wire::Transition::Record,
                            loops::Transition::Play {
                                position,
                                retrigger,
                            } => wire::Transition::Play {
                                position,
                                retrigger,
                            },
                            loops::Transition::Seek {
                                position,
                                retrigger,
                            } => wire::Transition::Seek {
                                position,
                                retrigger,
                            },
                            _ => return Err("transition event outside the hardware profile".into()),
                        })
                    }
                    loops::EventKind::Wrap => wire::EventKind::Wrap,
                    loops::EventKind::Pulse => wire::EventKind::Pulse,
                    _ => return Err("boundary event outside the hardware profile".into()),
                };
                Ok(wire::Event {
                    frame: event.frame.0,
                    target: wire::Identity {
                        slot: event.target.slot(),
                        generation: event.target.generation(),
                    },
                    kind,
                })
            })
            .collect::<Checked<_>>()?;
        let mut pending = runtime
            .planner()
            .resolver()
            .pending_transitions()
            .map(|p| {
                Ok(wire::PendingRequest {
                    name: observer
                        .names
                        .get(&p.request.sequence.0)
                        .ok_or("unknown pending action")?
                        .clone(),
                    requested: p.request.at.0,
                    effective: p.effective.0,
                    remaining_ticks: p.remaining_ticks,
                })
            })
            .collect::<Checked<Vec<_>>>()?;
        pending.sort_by(|a, b| a.name.cmp(&b.name));
        outcomes.sort_by(|a, b| a.name.cmp(&b.name));
        result.quanta.push(wire::QuantumResult {
            frame: frame.0,
            next_frame: runtime.next_frame().0,
            outputs: S::samples(outputs),
            states,
            outcomes,
            events,
            pending,
        });
    }
    // Final media reads occur off-render. Retained exports continue owning their
    // snapshots through later recordings and are read through production codecs.
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    for l in &scenario.loops {
        let target = loop_id(l.id)?;
        if checked(planner.resolver().state(target))?.mode == loops::Mode::Recording {
            continue;
        }
        let media = checked(planner.media_ledger().latest(target))?
            .or(checked(planner.media_ledger().playback(target))?);
        if let Some(id) = media {
            let view = checked(planner.media_ledger().view(id))?;
            if let Some(source) = view.audio {
                result.media.push(read_media::<S>(
                    format!("loop:{}:{}", l.id.slot, l.id.generation),
                    source,
                    &mut resources,
                )?);
            }
        }
    }
    // A final collection is an explicit host observation after the final quantum;
    // it cannot affect any processing/admission in the scenario.
    let mut final_outcomes = observer.collect(&mut control)?;
    while let Some(completion) = control.try_completion() {
        if let rt_runtime::transport::Outcome::Rejected(error) = completion.outcome {
            let requested = match completion.request {
                rt_runtime::executor::Command::Schedule { at, .. } => at.0,
                _ => return Err("unexpected final admission refusal".into()),
            };
            final_outcomes.push(wire::ActionResult {
                name: observer.names[&completion.sequence.0].clone(),
                requested,
                delivered: None,
                terminal: completion.frame.0,
                outcome: format!("rejected:{}", error_name(error)),
            });
        }
    }
    if let Some(last) = result.quanta.last_mut() {
        last.outcomes.extend(final_outcomes);
        last.outcomes.sort_by(|a, b| a.name.cmp(&b.name));
    }
    for (name, export) in observer.exports {
        if let Some(source) = export.view.audio {
            result
                .media
                .push(read_media::<S>(name, source, &mut resources)?);
        }
    }
    checked(planner.cleanup(&mut resources))?;
    result.media.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}
fn read_media<S: OracleSample>(
    name: String,
    source: loops::media::Source,
    resources: &mut ResourceStore<S>,
) -> Checked<wire::MediaResult> {
    let resource::Format::Audio { channels } = source.snapshot.description.format else {
        return Err("non-audio media in hardware profile".into());
    };
    let frames = source.snapshot.description.frames;
    let mut bytes = vec![0; source.snapshot.description.bytes];
    let mut cursor = checked(resources.render_io().cursor_at(source.index, 0))?;
    let mut copied = 0;
    while copied < bytes.len() {
        let count = checked(
            resources
                .render_io()
                .read(&mut cursor, &mut bytes[copied..]),
        )?;
        if count == 0 {
            return Err("incomplete media export".into());
        }
        copied += count;
    }
    let mut planar = vec![Vec::with_capacity(frames as usize); channels as usize];
    for (i, bytes) in bytes.chunks_exact(4).enumerate() {
        planar[i % channels as usize].push(S::from_le_bytes(bytes.try_into().unwrap()));
    }
    Ok(wire::MediaResult {
        name,
        frames,
        samples: S::samples(planar),
    })
}
