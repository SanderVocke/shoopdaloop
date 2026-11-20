use super::*;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use shoop_graph::{PortId, PortRole, SignalType};
use validate::channels;

type Node = usize;
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
enum Step {
    Alias(Node),
    Silence,
    Input(u32),
    Playback(Identity, u32),
    Gain(u32, Node),
    Mix(Node, Node),
    Smoothing(Identity, u32, Node),
    Capture(Identity, u32, Node),
    Output(u32, Node),
}
impl Step {
    fn sources(&self) -> Vec<Node> {
        match *self {
            Self::Alias(a)
            | Self::Gain(_, a)
            | Self::Smoothing(_, _, a)
            | Self::Capture(_, _, a)
            | Self::Output(_, a) => vec![a],
            Self::Mix(a, b) => vec![a, b],
            _ => vec![],
        }
    }
    fn remap(&mut self, mut map: impl FnMut(Node) -> Node) {
        match self {
            Self::Alias(a)
            | Self::Gain(_, a)
            | Self::Smoothing(_, _, a)
            | Self::Capture(_, _, a)
            | Self::Output(_, a) => *a = map(*a),
            Self::Mix(a, b) => {
                *a = map(*a);
                *b = map(*b);
            }
            _ => {}
        }
    }
    fn produces(&self) -> bool {
        !matches!(self, Self::Output(..) | Self::Capture(..))
    }
}
struct Plan<'a> {
    graph: &'a Graph,
    steps: Vec<Step>,
    sources: BTreeMap<PortId, Node>,
    positions: BTreeMap<(Owner, PortRole, u32), PortId>,
    state: Vec<InitialValue>,
    keys: Vec<StateBinding>,
    state_index: BTreeMap<StateKey, u32>,
    roots: Vec<Node>,
    common: BTreeMap<Step, Node>,
    meters: BTreeMap<Identity, Node>,
    bindings: Vec<LoopBinding>,
    views: u32,
    histories: u32,
}
impl<'a> Plan<'a> {
    fn new(graph: &'a Graph) -> Self {
        let mut plan = Self {
            graph,
            steps: vec![Step::Silence],
            sources: BTreeMap::new(),
            positions: BTreeMap::new(),
            state: vec![],
            keys: vec![],
            state_index: BTreeMap::new(),
            roots: vec![],
            common: BTreeMap::from([(Step::Silence, 0)]),
            meters: BTreeMap::new(),
            bindings: vec![],
            views: 0,
            histories: 0,
        };
        for p in graph
            .ports
            .iter()
            .filter(|p| p.id.signal == SignalType::Audio)
        {
            plan.positions
                .insert((p.id.owner, p.id.role, p.channel), p.id);
            if p.id.is_source() {
                let step = if p.id.owner == Owner::Host {
                    Step::Input(p.channel)
                } else {
                    Step::Silence
                };
                let n = if p.id.owner == Owner::Host {
                    plan.add(step)
                } else {
                    let n = plan.steps.len();
                    plan.steps.push(step);
                    n
                };
                plan.sources.insert(p.id, n);
            }
        }
        plan
    }
    fn add(&mut self, step: Step) -> Node {
        let common = step.produces() && !matches!(step, Step::Alias(_));
        if common {
            if let Some(&id) = self.common.get(&step) {
                return id;
            }
        }
        let id = self.steps.len();
        if common {
            self.common.insert(step.clone(), id);
        }
        self.steps.push(step);
        id
    }
    fn sum(&mut self, values: impl IntoIterator<Item = Node>) -> Node {
        let mut it = values.into_iter();
        let Some(mut result) = it.next() else {
            return 0;
        };
        for value in it {
            result = self.add(Step::Mix(result, value));
        }
        result
    }
    fn input(&mut self, owner: Owner, channel: u32) -> Node {
        let values = self
            .positions
            .get(&(
                owner,
                if owner == Owner::Host {
                    PortRole::Output
                } else {
                    PortRole::Input
                },
                channel,
            ))
            .map(|id| {
                self.graph
                    .connections
                    .iter()
                    .filter(|e| e.destination == *id)
                    .map(|e| self.sources[&e.source])
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.sum(values)
    }
    fn output(&mut self, owner: Owner, channel: u32, value: Node) {
        if let Some(id) = self.positions.get(&(owner, PortRole::Output, channel)) {
            self.steps[self.sources[id]] = Step::Alias(value);
        }
    }
    fn state(&mut self, key: StateKey, value: InitialValue) -> u32 {
        if let Some(&address) = self.state_index.get(&key) {
            return address;
        }
        let address = self.state.len() as u32;
        self.state.push(value);
        self.keys.push(StateBinding { key, address });
        self.state_index.insert(key, address);
        address
    }
    fn coefficient(
        &mut self,
        owner: Owner,
        channel: u32,
        source: Node,
        gain: f32,
    ) -> Result<Node, CompileError> {
        if !gain.is_finite() || !(0.0..128.0).contains(&gain) {
            return Err(CompileError::InvalidProgram);
        }
        if gain == 1.0 {
            return Ok(source);
        }
        let value = (f64::from(gain) * f64::from(1 << 24)) as i32;
        let address = self.state(
            StateKey {
                owner,
                role: StateRole::Coefficient(value),
                channel,
            },
            ramp(value),
        );
        Ok(self.add(Step::Gain(address, source)))
    }
    fn connected_channels(&self, owner: Owner) -> BTreeSet<u32> {
        self.positions
            .keys()
            .filter(|(o, _, _)| *o == owner)
            .map(|(_, _, c)| *c)
            .collect()
    }
    fn build(&mut self) -> Result<(), CompileError> {
        let g = self.graph;
        let mut inputs = BTreeMap::new();
        for s in &g.strips {
            let count = channels(s.layout)?;
            let mut used = self.connected_channels(Owner::Strip(s.id));
            if g.loops.iter().any(|l| l.strip == Some(s.id)) {
                used.extend(0..count);
            }
            for channel in used {
                let source = self.input(Owner::Strip(s.id), channel);
                let gain = db(s.controls.input_gain_db)
                    * balance(s.controls.input_balance, channel, count);
                let source = self.coefficient(Owner::Strip(s.id), channel, source, gain)?;
                inputs.insert((s.id, channel), source);
            }
        }
        let mut playback = BTreeMap::new();
        for l in &g.loops {
            let owner = Owner::Loop(l.id);
            let gain = self.state(
                StateKey {
                    owner,
                    role: StateRole::Gain,
                    channel: 0,
                },
                ramp((f64::from(l.gain) * f64::from(1 << 24)) as i32),
            );
            let count = channels(l.layout)?;
            let mut first_view = None;
            for channel in 0..count {
                let view = self.state(
                    StateKey {
                        owner,
                        role: StateRole::View,
                        channel,
                    },
                    InitialValue::MediaView(self.views),
                );
                self.views += 1;
                first_view.get_or_insert(view);
                let raw = self.add(Step::Playback(l.id, view));
                let value = self.add(Step::Gain(gain, raw));
                let mut value =
                    self.coefficient(owner, channel, value, balance(l.balance, channel, count))?;
                if l.boundary_smoothing_frames != 0 {
                    let control = self.state(
                        StateKey {
                            owner,
                            role: StateRole::Smoothing(l.boundary_smoothing_frames),
                            channel,
                        },
                        InitialValue::Smoothing {
                            destination: 0,
                            history: self.histories,
                            duration: l.boundary_smoothing_frames,
                        },
                    );
                    self.histories += 1;
                    value = self.add(Step::Smoothing(l.id, control, value));
                }
                playback.insert((l.id, channel), value);
                self.output(owner, channel, value);
                if channel == 0 {
                    self.meters.insert(l.id, value);
                    self.roots.push(value);
                }
                let source = if self
                    .positions
                    .contains_key(&(owner, PortRole::Input, channel))
                {
                    self.input(owner, channel)
                } else {
                    l.strip.map_or(0, |strip| inputs[&(strip, channel)])
                };
                let capture = self.add(Step::Capture(l.id, view, source));
                self.roots.push(capture);
            }
            self.bindings.push(LoopBinding {
                id: l.id,
                gain: Some(gain),
                view: first_view,
                meter: None,
            });
        }
        for s in &g.strips {
            let count = channels(s.layout)?;
            let mut used = self.connected_channels(Owner::Strip(s.id));
            if g.loops.iter().any(|l| l.strip == Some(s.id)) {
                used.extend(0..count);
            }
            for channel in used {
                let mut values = g
                    .loops
                    .iter()
                    .filter(|l| l.strip == Some(s.id))
                    .map(|l| playback[&(l.id, channel)])
                    .collect::<Vec<_>>();
                if s.controls.input_monitoring {
                    values.push(inputs[&(s.id, channel)]);
                }
                let source = self.sum(values);
                let gain = if s.controls.output_muted {
                    0.0
                } else {
                    db(s.controls.output_gain_db)
                        * balance(s.controls.output_balance, channel, count)
                };
                let value = self.coefficient(Owner::Strip(s.id), channel, source, gain)?;
                self.output(Owner::Strip(s.id), channel, value);
            }
        }
        for b in &g.buses {
            for channel in self.connected_channels(Owner::Bus(b.id)) {
                let source = self.input(Owner::Bus(b.id), channel);
                let gain = if b.muted {
                    0.0
                } else {
                    db(b.gain_db) * balance(b.balance, channel, b.channels)
                };
                let value = self.coefficient(Owner::Bus(b.id), channel, source, gain)?;
                self.output(Owner::Bus(b.id), channel, value);
            }
        }
        for p in g.ports.iter().filter(|p| {
            p.id.owner == Owner::Host
                && p.id.signal == SignalType::Audio
                && p.id.role == PortRole::Output
        }) {
            let input = self.input(Owner::Host, p.channel);
            let root = self.add(Step::Output(p.channel, input));
            self.roots.push(root);
        }
        if self.roots.is_empty() {
            self.roots.push(0);
        }
        Ok(())
    }
    fn resolve(&self, mut node: Node) -> Result<Node, CompileError> {
        for _ in 0..=self.steps.len() {
            match self.steps[node] {
                Step::Alias(next) => node = next,
                _ => return Ok(node),
            }
        }
        Err(CompileError::InvalidProgram)
    }
    fn emit(
        mut self,
        context: CompileContext,
        chunk_frames: u32,
        sync_pass: Vec<Identity>,
    ) -> Result<LogicalStateProgram, CompileError> {
        let resolved = (0..self.steps.len())
            .map(|n| self.resolve(n))
            .collect::<Result<Vec<_>, _>>()?;
        for step in &mut self.steps {
            step.remap(|n| resolved[n]);
        }
        let mut live = BTreeSet::new();
        let mut todo = self.roots.iter().map(|&r| resolved[r]).collect::<Vec<_>>();
        while let Some(n) = todo.pop() {
            if live.insert(n) {
                todo.extend(self.steps[n].sources());
            }
        }
        let mut users = vec![vec![]; self.steps.len()];
        let mut remaining = vec![0; self.steps.len()];
        for &n in &live {
            for source in self.steps[n].sources() {
                users[source].push(n);
                remaining[n] += 1;
            }
        }
        let mut ready = live
            .iter()
            .copied()
            .filter(|&n| remaining[n] == 0)
            .collect::<BTreeSet<_>>();
        let mut buffers = vec![None; self.steps.len()];
        let mut depth = 0u32;
        let mut work_pass = Vec::new();
        let mut jobs = 0u32;
        let mut emitted = 0;
        let mut input_ports = 0;
        let mut output_ports = 0;
        while let Some(n) = ready.pop_first() {
            let step = &self.steps[n];
            let destination = if step.produces() {
                let buffer = depth;
                depth += 1;
                buffers[n] = Some(buffer);
                buffer
            } else {
                0
            };
            let source = |n: Node| buffers[n].unwrap();
            let op = match *step {
                Step::Alias(_) => unreachable!(),
                Step::Silence => Operation::Silence { destination },
                Step::Input(port) => {
                    input_ports = input_ports.max(
                        port.checked_add(1)
                            .ok_or(CompileError::Capacity(Resource::Inputs))?,
                    );
                    Operation::Input { port, destination }
                }
                Step::Playback(loop_id, view) => Operation::Playback {
                    loop_id,
                    view,
                    destination,
                },
                Step::Gain(ramp, a) => Operation::Gain {
                    ramp,
                    source: source(a),
                    destination,
                },
                Step::Mix(a, b) => Operation::Mix {
                    left: source(a),
                    right: source(b),
                    destination,
                },
                Step::Smoothing(loop_id, control, a) => {
                    let InitialValue::Smoothing {
                        destination: ref mut out,
                        ..
                    } = self.state[control as usize]
                    else {
                        unreachable!()
                    };
                    *out = destination;
                    Operation::Smoothing {
                        loop_id,
                        control,
                        source: source(a),
                    }
                }
                Step::Capture(loop_id, view, a) => Operation::Capture {
                    loop_id,
                    view,
                    source: source(a),
                },
                Step::Output(port, a) => {
                    output_ports = output_ports.max(
                        port.checked_add(1)
                            .ok_or(CompileError::Capacity(Resource::Outputs))?,
                    );
                    Operation::Output {
                        source: source(a),
                        port,
                    }
                }
            };
            jobs = jobs
                .checked_add(if matches!(op, Operation::Capture { .. }) {
                    2
                } else {
                    1
                })
                .ok_or(CompileError::Capacity(Resource::Jobs))?;
            work_pass.push(op);
            for &user in &users[n] {
                remaining[user] -= 1;
                if remaining[user] == 0 {
                    ready.insert(user);
                }
            }
            emitted += 1;
        }
        if emitted != live.len() {
            return Err(CompileError::InvalidProgram);
        }
        for binding in &mut self.bindings {
            binding.meter = self
                .meters
                .get(&binding.id)
                .map(|&n| buffers[resolved[n]].unwrap());
        }
        let program = LogicalStateProgram {
            quantum: context.quantum,
            chunk_frames,
            sync_pass,
            work_pass,
            initial_state: self.state,
            state_bindings: self.keys,
            loops: self
                .graph
                .loops
                .iter()
                .map(|l| Loop {
                    id: l.id,
                    length: 0,
                    sync_source: l.sync_source,
                    repeat_sync: l.repeat_sync,
                })
                .collect(),
            bindings: self.bindings,
            jobs_per_quantum: jobs,
            resources: Resources {
                queue_capacity: jobs,
                buffers: depth,
                history_slots: self.histories,
                input_ports,
                output_ports,
                events: (4 * self.graph.loops.len()) as u32,
            },
        };
        program.validate_logical(context.limits)?;
        Ok(program)
    }
}
fn ramp(value: i32) -> InitialValue {
    InitialValue::Ramp(Ramp {
        start: value,
        target: value,
        duration: 0,
        elapsed: 0,
    })
}
fn db(value: f32) -> f32 {
    libm::powf(10.0, value / 20.0)
}
fn balance(value: f32, channel: u32, count: u32) -> f32 {
    if count != 2 {
        1.0
    } else if channel == 0 {
        1.0 - value.max(0.0)
    } else {
        1.0 + value.min(0.0)
    }
}
pub(super) fn lower(
    graph: &Graph,
    context: CompileContext,
    chunk_frames: u32,
) -> Result<LogicalStateProgram, CompileError> {
    let mut sync_pass = Vec::new();
    while sync_pass.len() < graph.loops.len() {
        let next = graph
            .loops
            .iter()
            .filter(|l| !sync_pass.contains(&l.id))
            .find(|l| {
                l.sync_source
                    .is_none_or(|s| s == l.id || sync_pass.contains(&s))
            })
            .ok_or(CompileError::InvalidProgram)?;
        sync_pass.push(next.id);
    }
    let mut plan = Plan::new(graph);
    plan.build()?;
    plan.emit(context, chunk_frames, sync_pass)
}
