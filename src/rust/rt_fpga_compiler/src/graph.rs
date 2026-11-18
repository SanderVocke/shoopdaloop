//! Target-independent graph input, with incremental FPGA lowering.
use crate::*;
pub use shoop_graph;

#[derive(Clone, Copy, Debug)]
pub struct LoopDescription {
    pub id: Identity,
    pub source: Option<u32>,
    pub input: bool,
    pub output: bool,
    pub gain: f32,
    pub repeat_sync: bool,
}
pub struct MonoGraphFixture {
    pub quantum: u32,
    pub loops: Vec<LoopDescription>,
}
impl MonoGraphFixture {
    pub fn compile(&self) -> Result<LogicalStateProgram, CompileError> {
        self.compile_with_limits(ProgramLimits::default())
    }
    pub fn compile_with_limits(
        &self,
        limits: ProgramLimits,
    ) -> Result<LogicalStateProgram, CompileError> {
        compile(
            &self.to_graph(),
            CompileContext {
                quantum: self.quantum,
                limits,
            },
        )
    }
    fn emit(&self, limits: ProgramLimits) -> Result<LogicalStateProgram, CompileError> {
        if self.loops.is_empty() {
            return Err(CompileError::InvalidProgram);
        }
        let n =
            u32::try_from(self.loops.len()).map_err(|_| CompileError::Capacity(Resource::Loops))?;
        // These are the minimum instructions for any routing of these loops.
        if u64::from(n) * 4 + 4 > u64::from(limits.instructions) {
            return Err(CompileError::Capacity(Resource::Instructions));
        }
        let outputs = self.loops.iter().filter(|l| l.output).count() as u32;
        let jobs = n
            .checked_mul(4)
            .and_then(|v| v.checked_add(outputs))
            .and_then(|v| v.checked_add(4))
            .ok_or(CompileError::Capacity(Resource::Jobs))?;
        let buffers = n
            .checked_mul(2)
            .and_then(|v| v.checked_add(outputs))
            .and_then(|v| v.checked_add(2))
            .ok_or(CompileError::Capacity(Resource::Buffers))?;
        let mut mixes = 0;
        let mut sum = 1;
        let mut loops = Vec::new();
        let mut bindings = Vec::new();
        let mut initial_state = Vec::new();
        let mut audio_pass = vec![
            Operation::Input {
                port: 0,
                destination: 0,
            },
            Operation::Silence { destination: 1 },
        ];
        for (i, l) in self.loops.iter().enumerate() {
            if !l.gain.is_finite() || !(0.0..=1.0).contains(&l.gain) {
                return Err(CompileError::InvalidProgram);
            }
            let source = l
                .source
                .map(|slot| {
                    self.loops
                        .iter()
                        .find(|s| s.id.slot == slot)
                        .map(|s| s.id)
                        .ok_or(CompileError::InvalidProgram)
                })
                .transpose()?;
            loops.push(Loop {
                id: l.id,
                length: 0,
                sync_source: source,
                repeat_sync: l.repeat_sync,
            });
            initial_state.push(InitialValue::Ramp(Ramp {
                start: (f64::from(l.gain) * f64::from(1 << 24)) as i32,
                target: (f64::from(l.gain) * f64::from(1 << 24)) as i32,
                duration: 0,
                elapsed: 0,
            }));
            let view = n + i as u32;
            let playback = 2 + 2 * i as u32;
            let meter = playback + 1;
            audio_pass.extend([
                Operation::Playback {
                    loop_id: l.id,
                    view,
                    destination: playback,
                },
                Operation::Gain {
                    ramp: i as u32,
                    source: playback,
                    destination: meter,
                },
                Operation::Capture {
                    loop_id: l.id,
                    view,
                    source: u32::from(!l.input),
                },
            ]);
            if l.output {
                let destination = 2 + 2 * n + mixes;
                mixes += 1;
                audio_pass.push(Operation::Mix {
                    left: sum,
                    right: meter,
                    destination,
                });
                sum = destination;
            }
            bindings.push(LoopBinding {
                id: l.id,
                gain: Some(i as u32),
                view: Some(view),
                meter: Some(meter),
            });
        }
        initial_state.extend((0..n).map(InitialValue::MediaView));
        audio_pass.extend([
            Operation::Output {
                source: sum,
                port: 0,
            },
            Operation::Output {
                source: sum,
                port: 1,
            },
        ]);
        let mut sync_pass = Vec::new();
        while sync_pass.len() < loops.len() {
            let next = loops
                .iter()
                .filter(|l| !sync_pass.contains(&l.id))
                .filter(|l| {
                    l.sync_source
                        .is_none_or(|s| s == l.id || sync_pass.contains(&s))
                })
                .min_by_key(|l| (l.id.slot, l.id.generation));
            let Some(next) = next else {
                return Err(CompileError::InvalidProgram);
            };
            sync_pass.push(next.id);
        }
        let program = LogicalStateProgram {
            quantum: self.quantum,
            chunk_frames: 1,
            sync_pass,
            audio_pass,
            initial_state,
            loops,
            bindings,
            jobs_per_quantum: jobs,
            resources: Resources {
                queue_capacity: jobs,
                buffers,
                history_slots: 0,
                input_ports: 1,
                output_ports: 2,
                events: 4 * n,
            },
        };
        program.validate_logical(limits)?;
        Ok(program)
    }
}

use alloc::borrow::Cow;
use shoop_graph::{
    BusView, ChannelLayout, Connection, Graph, GraphView, LoopView, Owner, PortId, PortRole,
    PortView, SignalType, StripControls, StripView,
};
#[derive(Clone, Copy, Debug)]
pub struct CompileContext {
    pub quantum: u32,
    pub limits: ProgramLimits,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Feature {
    Midi,
    Channels,
    Processor,
    Composite,
    StripControls,
    LoopControls,
    BusControls,
    Buses,
    Routing,
}
fn unsupported(object: Owner, feature: Feature) -> CompileError {
    CompileError::Unsupported { object, feature }
}

pub fn compile(
    graph: &impl GraphView,
    context: CompileContext,
) -> Result<LogicalStateProgram, CompileError> {
    let strips = graph.strips().collect::<Vec<_>>();
    let loops = graph.loops().collect::<Vec<_>>();
    let buses = graph.buses().collect::<Vec<_>>();
    let ports = graph.ports().collect::<Vec<_>>();
    let edges = graph.connections().collect::<Vec<_>>();
    let valid_id = |id: Identity| id.generation != 0;
    if strips
        .iter()
        .enumerate()
        .any(|(i, s)| !valid_id(s.id) || strips[..i].iter().any(|p| p.id.slot == s.id.slot))
        || loops
            .iter()
            .enumerate()
            .any(|(i, l)| !valid_id(l.id) || loops[..i].iter().any(|p| p.id.slot == l.id.slot))
        || buses
            .iter()
            .enumerate()
            .any(|(i, b)| !valid_id(b.id) || buses[..i].iter().any(|p| p.id.slot == b.id.slot))
    {
        return Err(CompileError::InvalidProgram);
    }
    let owner_exists = |owner| match owner {
        Owner::Strip(id) => strips.iter().any(|s| s.id == id),
        Owner::Loop(id) => loops.iter().any(|l| l.id == id),
        Owner::Bus(id) => buses.iter().any(|b| b.id == id),
        Owner::Host | Owner::Global => true,
    };
    if ports
        .iter()
        .enumerate()
        .any(|(i, p)| !owner_exists(p.id.owner) || ports[..i].iter().any(|q| q.id == p.id))
        || edges.iter().enumerate().any(|(i, e)| {
            !ports.iter().any(|p| p.id == e.source)
                || !ports.iter().any(|p| p.id == e.destination)
                || !e.source.is_source()
                || e.destination.is_source()
                || e.source.signal != e.destination.signal
                || edges[..i].contains(e)
        })
        || loops.iter().any(|l| {
            l.strip.is_some_and(|s| !strips.iter().any(|p| p.id == s))
                || l.sync_source
                    .is_some_and(|s| !loops.iter().any(|p| p.id == s))
        })
    {
        return Err(CompileError::InvalidProgram);
    }
    for s in &strips {
        let owner = Owner::Strip(s.id);
        if s.processor.is_some() {
            return Err(unsupported(owner, Feature::Processor));
        }
        match s.layout {
            ChannelLayout::Direct { midi: true, .. }
            | ChannelLayout::DryWet { dry_midi: true, .. } => {
                return Err(unsupported(owner, Feature::Midi))
            }
            ChannelLayout::Direct {
                audio: 1,
                midi: false,
            } => {}
            _ => return Err(unsupported(owner, Feature::Channels)),
        }
        if s.controls != StripControls::default() || s.default_dry_playback {
            return Err(unsupported(owner, Feature::StripControls));
        }
    }
    let Some(master) = buses.iter().find(|b| b.master) else {
        return Err(CompileError::InvalidProgram);
    };
    if buses.len() != 1 {
        return Err(unsupported(Owner::Bus(master.id), Feature::Buses));
    }
    if master.channels != 2 {
        return Err(unsupported(Owner::Bus(master.id), Feature::Channels));
    }
    if master.processor.is_some() {
        return Err(unsupported(Owner::Bus(master.id), Feature::Processor));
    }
    if master.gain_db != 0. || master.balance != 0. || master.muted {
        return Err(unsupported(Owner::Bus(master.id), Feature::BusControls));
    }
    let port = |owner, role, index| PortId {
        owner,
        signal: SignalType::Audio,
        role,
        index,
    };
    let host_port = |role, index| {
        ports
            .iter()
            .find(|p| {
                p.id.owner == Owner::Host
                    && p.id.signal == SignalType::Audio
                    && p.id.role == role
                    && p.channel == index
            })
            .map(|p| p.id)
            .unwrap_or_else(|| port(Owner::Host, role, u32::MAX))
    };
    let master_output = |i| Connection {
        source: port(Owner::Bus(master.id), PortRole::Output, i),
        destination: host_port(PortRole::Output, i),
    };
    if !(0..2).all(|i| edges.contains(&master_output(i))) {
        return Err(unsupported(Owner::Bus(master.id), Feature::Routing));
    }
    for e in &edges {
        if e.source.signal == SignalType::Midi {
            return Err(unsupported(e.destination.owner, Feature::Midi));
        }
        let allowed = match (e.source.owner, e.destination.owner) {
            (Owner::Host, Owner::Strip(_)) => {
                e.source == host_port(PortRole::Input, 0)
                    && e.destination.role == PortRole::Input
                    && e.destination.index == 0
            }
            (Owner::Strip(_), Owner::Bus(id)) => {
                id == master.id
                    && e.source.role == PortRole::Output
                    && e.source.index == 0
                    && e.destination.role == PortRole::Input
                    && e.destination.index < 2
            }
            (Owner::Bus(_), Owner::Host) => (0..2).any(|i| *e == master_output(i)),
            _ => false,
        };
        if !allowed {
            return Err(unsupported(e.destination.owner, Feature::Routing));
        }
    }
    let mut lowered = Vec::new();
    for l in loops {
        let owner = Owner::Loop(l.id);
        if l.composite.is_some() || l.strip.is_none() {
            return Err(unsupported(owner, Feature::Composite));
        }
        if l.layout
            != (ChannelLayout::Direct {
                audio: 1,
                midi: false,
            })
        {
            return Err(unsupported(owner, Feature::Channels));
        }
        if l.balance != 0. || l.boundary_smoothing_frames != 0 {
            return Err(unsupported(owner, Feature::LoopControls));
        }
        let strip = l.strip.unwrap();
        let routed = (0..2)
            .filter(|&i| {
                edges.contains(&Connection {
                    source: port(Owner::Strip(strip), PortRole::Output, 0),
                    destination: port(Owner::Bus(master.id), PortRole::Input, i),
                })
            })
            .count();
        if routed != 0 && routed != 2 {
            return Err(unsupported(owner, Feature::Routing));
        }
        lowered.push(LoopDescription {
            id: l.id,
            source: l.sync_source.map(|s| s.slot),
            input: edges.contains(&Connection {
                source: host_port(PortRole::Input, 0),
                destination: port(Owner::Strip(strip), PortRole::Input, 0),
            }),
            output: routed == 2,
            gain: l.gain,
            repeat_sync: l.repeat_sync,
        });
    }
    MonoGraphFixture {
        quantum: context.quantum,
        loops: lowered,
    }
    .emit(context.limits)
}

impl MonoGraphFixture {
    /// Full graph fixtures for component benches, using one independent strip per loop.
    pub fn to_graph(&self) -> Graph {
        let mut graph = Graph::default();
        let master = Identity {
            slot: 0,
            generation: 1,
        };
        graph.buses.push(BusView {
            id: master,
            name: Cow::Borrowed("Master"),
            channels: 2,
            master: true,
            gain_db: 0.,
            balance: 0.,
            muted: false,
            processor: None,
        });
        let mut add_port = |owner, role, index| {
            let id = PortId {
                owner,
                signal: SignalType::Audio,
                role,
                index,
            };
            graph.ports.push(PortView {
                id,
                channel: index,
                name: Cow::Borrowed(""),
            });
            id
        };
        let input = add_port(Owner::Host, PortRole::Input, 0);
        let mut bus_inputs = Vec::new();
        for i in 0..2 {
            let source = add_port(Owner::Bus(master), PortRole::Output, i);
            let destination = add_port(Owner::Host, PortRole::Output, i);
            graph.connections.push(Connection {
                source,
                destination,
            });
            bus_inputs.push(add_port(Owner::Bus(master), PortRole::Input, i));
        }
        for l in &self.loops {
            let layout = ChannelLayout::Direct {
                audio: 1,
                midi: false,
            };
            graph.strips.push(StripView {
                id: l.id,
                name: Cow::Borrowed("fixture"),
                layout,
                controls: StripControls::default(),
                default_dry_playback: false,
                processor: None,
            });
            graph.loops.push(LoopView {
                id: l.id,
                strip: Some(l.id),
                layout,
                gain: l.gain,
                balance: 0.,
                boundary_smoothing_frames: 0,
                sync_source: l.source.map(|slot| {
                    self.loops.iter().find(|s| s.id.slot == slot).map_or(
                        Identity {
                            slot,
                            generation: 1,
                        },
                        |s| s.id,
                    )
                }),
                repeat_sync: l.repeat_sync,
                composite: None,
            });
            let capture = add_port(Owner::Strip(l.id), PortRole::Input, 0);
            let playback = add_port(Owner::Strip(l.id), PortRole::Output, 0);
            if l.input {
                graph.connections.push(Connection {
                    source: input,
                    destination: capture,
                });
            }
            if l.output {
                for &destination in &bus_inputs {
                    graph.connections.push(Connection {
                        source: playback,
                        destination,
                    });
                }
            }
        }
        graph
    }
}
