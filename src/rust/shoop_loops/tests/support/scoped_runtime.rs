use super::*;

pub(crate) struct CallLog {
    count: AtomicUsize,
    entries: Box<[(std::sync::atomic::AtomicU64, AtomicUsize); 512]>,
}
impl CallLog {
    fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            entries: Box::new(std::array::from_fn(|_| {
                (std::sync::atomic::AtomicU64::new(0), AtomicUsize::new(0))
            })),
        }
    }
    fn record(&self, frame: Frame, frames: usize) {
        let index = self.count.fetch_add(1, Ordering::Relaxed);
        if let Some((at, width)) = self.entries.get(index) {
            at.store(frame.0, Ordering::Relaxed);
            width.store(frames, Ordering::Relaxed);
        }
    }
    pub(crate) fn load(&self, order: Ordering) -> usize {
        self.count.load(order)
    }
    pub(crate) fn snapshot(&self) -> Vec<(u64, usize)> {
        self.entries
            .iter()
            .take(self.load(Ordering::Relaxed).min(self.entries.len()))
            .map(|(at, width)| (at.load(Ordering::Relaxed), width.load(Ordering::Relaxed)))
            .collect()
    }
}

pub(crate) fn id() -> LoopId {
    LoopId::from_parts(0, 1).unwrap()
}
pub(crate) fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
pub(crate) fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
pub(crate) fn upload(
    store: &mut ResourceStore,
    format: Format,
    frames: u64,
    bytes: &[u8],
    midi: bool,
) -> media::Source {
    let resource = store.create().unwrap();
    let description = Description {
        format,
        frames,
        bytes: bytes.len(),
    };
    let token = if midi {
        store
            .begin_indexed_midi_upload(resource, Revision(0), description, &MidiState::default())
            .unwrap()
    } else {
        store
            .begin_upload(resource, Revision(0), description)
            .unwrap()
    };
    for (page, chunk) in bytes.chunks(256).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 256, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    media::Source {
        snapshot,
        index: store.read_index(snapshot.id).unwrap(),
    }
}
pub(crate) struct Fx {
    calls: Arc<CallLog>,
    origin: u64,
    fail: bool,
    overflow: bool,
}
pub(crate) enum Node {
    Loop(CallbackNode),
    Fx(Fx),
    FailingCapture(CallbackNode, u8),
}
impl Processor<CallbackPlanner> for Node {
    fn cadence(&self) -> Cadence {
        Cadence::Callback
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(p) | Self::FailingCapture(p, _) => p.ports(),
            Self::Fx(_) => PortCounts {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_inputs: 1,
                midi_outputs: 1,
            },
        }
    }
    fn planning(&self) -> ConsumerAccess {
        match self {
            Self::Loop(p) | Self::FailingCapture(p, _) => p.planning(),
            Self::Fx(_) => ConsumerAccess::default(),
        }
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter > 1 {
            return Err(Error::InvalidParameter);
        }
        match self {
            Self::Fx(p) => {
                if parameter == 0 {
                    p.fail = value != 0.0;
                } else {
                    p.overflow = value != 0.0;
                }
                Ok(())
            }
            Self::FailingCapture(_, fail) => {
                *fail = if value == 0.0 { 0 } else { parameter as u8 + 1 };
                Ok(())
            }
            _ => Err(Error::InvalidParameter),
        }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, CallbackPlanner>) -> Result<(), Error> {
        match self {
            Self::Loop(p) => p.process(b),
            Self::FailingCapture(p, fail) => {
                if *fail == 2 {
                    *fail = 0;
                    return Ok(());
                }
                p.process(b)?;
                if core::mem::take(fail) != 0 {
                    Err(Error::Incomplete)
                } else {
                    Ok(())
                }
            }
            Self::Fx(p) => p.process(b),
        }
    }
}
pub(crate) type Runtime = GraphRuntime<Node, CallbackPlanner>;
pub(crate) type Control = rt_runtime::executor::Control<Node, CallbackPlanner>;
pub(crate) fn setup(span_limit: usize) -> (Control, Runtime, Arc<CallLog>) {
    setup_at(span_limit, 0)
}
pub(crate) fn setup_at(span_limit: usize, origin: u64) -> (Control, Runtime, Arc<CallLog>) {
    setup_details(span_limit, origin, 4)
}
pub(crate) fn setup_details(
    span_limit: usize,
    origin: u64,
    length: u64,
) -> (Control, Runtime, Arc<CallLog>) {
    setup_profile(span_limit, origin, length, 1)
}
pub(crate) fn setup_two_owners() -> (Control, Runtime, Arc<CallLog>) {
    setup_profile(32, 0, 4, 2)
}
fn setup_profile(
    span_limit: usize,
    origin: u64,
    length: u64,
    owners: u32,
) -> (Control, Runtime, Arc<CallLog>) {
    let (config, mut resources, topology, imports) = parts_owners(length, owners);
    let mut prepared = CallbackPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: planner(),
            normal_requests: 4,
            media_slots: 8,
            export_slots: 4,
        },
        (0..owners)
            .map(|owner| Binding {
                target: LoopId::from_parts(owner, 1).unwrap(),
                playback: node(if owner == 0 { 0 } else { 3 }),
                capture: node(if owner == 0 { 2 } else { 4 }),
                channels: 2,
                midi: true,
                gain: if owner == 0 { 1.0 } else { 0.5 },
                smoothing: Smoothing::None,
                midi_events: 1024,
                midi_bytes: 8192,
                packet_bytes: 8192,
            })
            .collect(),
        imports,
        span_limit,
    )
    .unwrap();
    prepared
        .planner
        .prepare_frame_origin(Frame(origin))
        .unwrap();
    let bindings = prepared.planner.bindings().to_vec();
    let calls = Arc::new(CallLog::new());
    let mut nodes: Vec<_> = prepared
        .nodes
        .into_iter()
        .map(|(id, p)| {
            PreparedNode::new(
                id,
                if id == node(2) {
                    Node::FailingCapture(p, 0)
                } else {
                    Node::Loop(p)
                },
            )
        })
        .collect();
    nodes.push(PreparedNode::new(
        node(1),
        Node::Fx(Fx {
            calls: calls.clone(),
            origin,
            fail: false,
            overflow: false,
        }),
    ));
    let mut graph = GraphDescription {
        nodes: (0..owners * 2 + 1).map(node).collect(),
        connections: Vec::new(),
    };
    for (kind, ports) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
        for port in 0..ports {
            for (source, target) in (0..owners)
                .flat_map(|owner| {
                    [
                        (node(if owner == 0 { 0 } else { 3 }), Some(node(1))),
                        (node(1), Some(node(if owner == 0 { 2 } else { 4 }))),
                    ]
                })
                .chain(core::iter::once((node(1), None)))
            {
                graph.connections.push(Connection {
                    source: Endpoint::node(source, kind, port),
                    destination: target.map_or(Endpoint::host(kind, port), |id| {
                        Endpoint::node(id, kind, port)
                    }),
                });
            }
        }
    }
    let (control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        prepared.planner,
        resources,
        &graph,
        &bindings,
    )
    .unwrap();
    runtime.prepare_frame_origin(Frame(origin)).unwrap();
    (control, runtime, calls)
}
pub(crate) fn command(control: &mut Control, operation: callback::Operation) {
    assert!(control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(0),
                action: Action::Planner {
                    planner: planner(),
                    operation
                },
                payload: Box::new([])
            }
        )
        .is_ok());
}
pub(crate) fn render(
    runtime: &mut Runtime,
    frames: usize,
) -> Result<(Vec<f32>, Vec<f32>, Vec<(u64, Vec<u8>)>), Error> {
    let at = runtime.next_frame();
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    let mut midi = [MidiBuffer::new(4096, 16384).unwrap()];
    support::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: at,
                frames,
                inputs: &[],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut midi,
            },
        )
    })?;
    let midi = (0..midi[0].len())
        .map(|i| {
            let (offset, data) = midi[0].get(i).unwrap();
            (at.0 + offset as u64, data.to_vec())
        })
        .collect();
    Ok((left, right, midi))
}

fn parts() -> (
    rt_runtime::PreparedConfig,
    ResourceStore,
    PreparedTopology,
    Vec<media::Input>,
) {
    parts_length(4)
}
pub(crate) fn parts_length(
    length: u64,
) -> (
    rt_runtime::PreparedConfig,
    ResourceStore,
    PreparedTopology,
    Vec<media::Input>,
) {
    parts_owners(length, 1)
}
fn parts_owners(
    length: u64,
    owners: u32,
) -> (
    rt_runtime::PreparedConfig,
    ResourceStore,
    PreparedTopology,
    Vec<media::Input>,
) {
    let config = Config {
        max_frames: 32,
        audio_inputs: 0,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 1,
        max_nodes: (owners * 2 + 1) as usize,
        max_edges: (owners * 8 + 4) as usize,
        max_ports: (owners * 12 + 4) as usize,
        midi_events_per_port: 4096,
        midi_bytes_per_port: 16384,
        resource_slots: 16,
        resource_pages: 128,
        page_bytes: 256,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 8,
            checkpoints_per_version: 16,
            replay_events: 4096,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 32768,
        },
    )
    .unwrap();
    let audio: Vec<_> = (1..=length)
        .flat_map(|i| [i as f32, i as f32 * 10.0])
        .flat_map(f32::to_le_bytes)
        .collect();
    let audio = upload(
        &mut resources,
        Format::Audio { channels: 2 },
        length,
        &audio,
        false,
    );
    let mut packet = [0; 32];
    let packet_len = encode_midi_record(u64::from(length > 1), &[0xf8], &mut packet).unwrap();
    let midi = upload(
        &mut resources,
        Format::Midi,
        length,
        &packet[..packet_len],
        true,
    );
    let topology = compile(
        Limits {
            loop_slots: owners as usize,
            pending_requests: 8,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: (0..owners)
                .map(|owner| LoopSpec {
                    id: LoopId::from_parts(owner, 1).unwrap(),
                    kind: LoopKind::Primitive,
                    length,
                    sync_source: Some(id()),
                })
                .collect(),
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let mut imports = vec![media::Input {
        description: media::Description {
            owner: id(),
            revision: Revision(0),
            frames: length,
            channels: 2,
            midi: true,
        },
        audio: Some(audio),
        midi: Some(midi),
    }];
    for owner in 1..owners {
        let audio_snapshot = resources
            .snapshot(audio.snapshot.resource, Frame(0))
            .unwrap();
        let midi_snapshot = resources
            .snapshot(midi.snapshot.resource, Frame(0))
            .unwrap();
        let audio = media::Source {
            snapshot: audio_snapshot,
            index: resources.read_index(audio_snapshot.id).unwrap(),
        };
        let midi = media::Source {
            snapshot: midi_snapshot,
            index: resources.read_index(midi_snapshot.id).unwrap(),
        };
        imports.push(media::Input {
            description: media::Description {
                owner: LoopId::from_parts(owner, 1).unwrap(),
                revision: Revision(0),
                frames: length,
                channels: 2,
                midi: true,
            },
            audio: Some(audio),
            midi: Some(midi),
        });
    }
    (config, resources, topology, imports)
}

pub(crate) fn play(control: &mut Control) {
    command(
        control,
        callback::Operation::Transition {
            target: id(),
            at: Frame(0),
            timing: Timing::At,
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
    );
}
pub(crate) fn schedule(
    control: &mut Control,
    at: Frame,
    action: Action<callback::Operation>,
    payload: &[u8],
) {
    assert!(control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at,
                action,
                payload: payload.into()
            }
        )
        .is_ok());
}

impl<L: rt_runtime::planner::Planner> Processor<L> for Fx {
    fn cadence(&self) -> Cadence {
        Cadence::Callback
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, L>) -> Result<(), Error> {
        self.calls.record(b.frame, b.frames);
        if core::mem::take(&mut self.fail) {
            return Err(Error::Capacity);
        }
        for channel in 0..2 {
            let (input, output) = b.audio.pair(channel, channel).unwrap();
            for (input, output) in input.iter().zip(output) {
                *output = *input * 3.0;
            }
        }
        if core::mem::take(&mut self.overflow) {
            for _ in 0..4097 {
                b.midi_outputs[0].push(0, b.frames, &[0xf8])?;
            }
        }
        // Clock packets pass through; the algorithm produces one held note.
        for offset in 0..b.frames {
            for event in 0..b.midi_inputs[0].len() {
                let (at, data) = b.midi_inputs[0].get(event).unwrap();
                if at as usize == offset && data == [0xf8] {
                    b.midi_outputs[0].push(at, b.frames, data)?;
                }
            }
            match b.frame.0 - self.origin + offset as u64 {
                2 => b.midi_outputs[0].push(offset as u32, b.frames, &[0x90, 72, 91])?,
                9 => b.midi_outputs[0].push(offset as u32, b.frames, &[0x80, 72, 64])?,
                _ => {}
            }
        }
        Ok(())
    }
}

pub(crate) enum BusNode {
    Loop(shoop_loops::runtime::LoopNode),
    Fx(Fx),
}
impl Processor<shoop_loops::runtime::LoopPlanner> for BusNode {
    fn cadence(&self) -> Cadence {
        match self {
            Self::Loop(p) => p.cadence(),
            Self::Fx(p) => <Fx as Processor<shoop_loops::runtime::LoopPlanner>>::cadence(p),
        }
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(p) => p.ports(),
            Self::Fx(p) => <Fx as Processor<shoop_loops::runtime::LoopPlanner>>::ports(p),
        }
    }
    fn planning(&self) -> ConsumerAccess {
        match self {
            Self::Loop(p) => p.planning(),
            Self::Fx(_) => ConsumerAccess::default(),
        }
    }
    fn process(
        &mut self,
        b: &mut ProcessBlock<'_, shoop_loops::runtime::LoopPlanner>,
    ) -> Result<(), Error> {
        match self {
            Self::Loop(p) => p.process(b),
            Self::Fx(p) => p.process(b),
        }
    }
}
pub(crate) type BusRuntime = GraphRuntime<BusNode, shoop_loops::runtime::LoopPlanner>;
pub(crate) type BusControl =
    rt_runtime::executor::Control<BusNode, shoop_loops::runtime::LoopPlanner>;
pub(crate) fn setup_bus() -> (BusControl, BusRuntime, Arc<CallLog>) {
    let (config, mut resources, topology, imports) = parts();
    let prepared = shoop_loops::runtime::LoopPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: planner(),
            normal_requests: 4,
            media_slots: 8,
            export_slots: 4,
        },
        vec![Binding {
            target: id(),
            playback: node(0),
            capture: node(2),
            channels: 2,
            midi: true,
            gain: 1.0,
            smoothing: Smoothing::None,
            midi_events: 1024,
            midi_bytes: 8192,
            packet_bytes: 8192,
        }],
        imports,
        shoop_loops::runtime::Options::default(),
    )
    .unwrap();
    let bindings = prepared.client.bindings().to_vec();
    let calls = Arc::new(CallLog::new());
    let mut nodes: Vec<_> = prepared
        .nodes
        .into_iter()
        .map(|(id, p)| PreparedNode::new(id, BusNode::Loop(p)))
        .collect();
    nodes.push(PreparedNode::new(
        node(1),
        BusNode::Fx(Fx {
            calls: calls.clone(),
            origin: 0,
            fail: false,
            overflow: false,
        }),
    ));
    let mut graph = GraphDescription {
        nodes: vec![node(0), node(1), node(2)],
        connections: Vec::new(),
    };
    for (kind, count) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
        for port in 0..count {
            for (source, target) in [(node(0), Some(node(1))), (node(1), None)] {
                graph.connections.push(Connection {
                    source: Endpoint::node(source, kind, port),
                    destination: target.map_or(Endpoint::host(kind, port), |id| {
                        Endpoint::node(id, kind, port)
                    }),
                });
            }
        }
    }
    let (control, runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        prepared.planner,
        resources,
        &graph,
        &bindings,
    )
    .unwrap();
    (control, runtime, calls)
}
