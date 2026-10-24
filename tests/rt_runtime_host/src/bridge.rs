use crate::{allocator_canaries, config, guarded, node::Node, node_id, wire::*, QUANTUM};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{Connection, Endpoint, PortType, PreparedPlan};
use rt_runtime::resource::*;
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Epoch, Error, Frame, NodeId, ResourceId, Revision, Sequence};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const PLAN_POOL: usize = 7;
type Returned = rt_runtime::transport::Completion<Command<Node>, Retired<Node>>;
struct Stage {
    generation: u32,
    steps: usize,
    edges: usize,
    epoch: Epoch,
    plan: Option<PreparedPlan>,
}
#[derive(Clone, Copy)]
struct Export {
    generation: u32,
    cursor: Option<Cursor>,
    busy: bool,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Bridge {
    control: Control<Node>,
    runtime: GraphRuntime<Node>,
    packets: Box<[Packet; SLOTS]>,
    headers: [Header; SLOTS],
    pending: [Option<Sequence>; SLOTS],
    emergency: Option<Returned>,
    ready: u32,
    last_sequence: u64,
    stream: u64,
    buffers: Vec<Vec<Option<Box<[u8]>>>>,
    plans: Vec<PreparedPlan>,
    stages: [Stage; 2],
    stage_pending: [Option<usize>; SLOTS],
    exports: [Export; 16],
    export_pending: [Option<usize>; SLOTS],
    export_new: [bool; SLOTS],
    output: Box<[f32; QUANTUM]>,
    inputs: [MidiBuffer; 1],
    outputs: [MidiBuffer; 1],
    recovery: MidiBuffer,
    parked: Option<Node>,
    error: u32,
    max_copy: usize,
    max_steps: usize,
    node_frames: u64,
    sample_hash: u32,
    midi_hash: u32,
    midi_events: u64,
    callbacks: u64,
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Bridge {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(stream: u64) -> Self {
        assert!(stream != 0);
        assert!(allocator_canaries());
        let config = config();
        let mut store = ResourceStore::new(config).expect("resource preparation");
        let playback = store.create().unwrap();
        let audio_capture = store.create().unwrap();
        let midi_capture = store.create().unwrap();
        let upload = store
            .begin_upload(
                playback,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 1 },
                    frames: 16,
                    bytes: 64,
                },
            )
            .unwrap();
        let mut initial = [0u8; 64];
        for sample in initial.chunks_exact_mut(4) {
            sample.copy_from_slice(&1f32.to_le_bytes());
        }
        store.begin_callback();
        store.write_upload(upload, 0, &initial).unwrap();
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        let snapshot = store.snapshot(playback, Frame(0)).unwrap();
        let cursor = store.cursor(snapshot.id).unwrap();
        let node = Node {
            playback,
            audio_capture,
            midi_capture,
            snapshot,
            cursor,
            frames: 0,
            gain: 1.0,
        };
        let (mut control, runtime) = GraphRuntime::prepare_with_resources(
            config,
            vec![PreparedNode::new(node_id(), node)],
            store,
        )
        .unwrap();
        control.prepare_submissions().unwrap();
        let mut plans = Vec::with_capacity(PLAN_POOL);
        for _ in 0..PLAN_POOL {
            plans.push(PreparedPlan::with_capacity(config).unwrap());
        }
        let buffers = (0..SLOTS)
            .map(|_| {
                (0..=64)
                    .map(|len| {
                        let mut bytes = vec![0; len].into_boxed_slice();
                        for byte in &mut bytes {
                            *std::hint::black_box(byte) = 0;
                        }
                        Some(bytes)
                    })
                    .collect()
            })
            .collect();
        Self {
            control,
            runtime,
            packets: Box::new([Packet::default(); SLOTS]),
            headers: [Header::default(); SLOTS],
            pending: [None; SLOTS],
            emergency: None,
            ready: 0,
            last_sequence: 0,
            stream,
            buffers,
            plans,
            stages: std::array::from_fn(|_| Stage {
                generation: 1,
                steps: 0,
                edges: 0,
                epoch: Epoch(0),
                plan: None,
            }),
            stage_pending: [None; SLOTS],
            exports: [Export {
                generation: 1,
                cursor: None,
                busy: false,
            }; 16],
            export_pending: [None; SLOTS],
            export_new: [false; SLOTS],
            output: Box::new([0.; QUANTUM]),
            inputs: [MidiBuffer::new(8, 64).unwrap()],
            outputs: [MidiBuffer::new(8, 64).unwrap()],
            recovery: MidiBuffer::new(32, 96).unwrap(),
            parked: None,
            error: 0,
            max_copy: 0,
            max_steps: 0,
            node_frames: 0,
            sample_hash: 2166136261,
            midi_hash: 2166136261,
            midi_events: 0,
            callbacks: 0,
        }
    }
    pub fn frame_ptr(&mut self, slot: u32) -> usize {
        self.packets
            .get_mut(slot as usize)
            .map_or(0, |packet| packet.bytes.as_mut_ptr() as usize)
    }
    pub fn output_ptr(&self) -> usize {
        self.output.as_ptr() as usize
    }
    pub fn reply_len(&self, slot: u32) -> u32 {
        self.packets
            .get(slot as usize)
            .map_or(0, |packet| packet.len as u32)
    }
    pub fn ingest(&mut self, slot: u32, length: u32) -> u32 {
        guarded(|| {
            if slot as usize >= SLOTS {
                return error_code(Error::InvalidId);
            }
            if self.occupied(slot as usize) {
                return error_code(Error::Busy);
            }
            self.packets[slot as usize].len = length as usize;
            self.ingest_inner(slot as usize);
            0
        })
    }
    /// The only Rust audio entry point. Scalar bindings do not allocate slice adapters.
    pub fn render(&mut self) -> u32 {
        guarded(|| {
            if self.error == 0 {
                if let Err(error) = self.render_inner() {
                    self.error = error_code(error);
                }
            }
            let ready = self.ready;
            self.ready = 0;
            ready | if self.error == 0 { 0 } else { 0x80000000 }
        })
    }
    pub fn next_frame(&self) -> u64 {
        self.runtime.next_frame().0
    }
    pub fn error(&self) -> u32 {
        self.error
    }
    pub fn marker(&self) -> u32 {
        crate::marker()
    }
    pub fn sample_hash(&self) -> u32 {
        self.sample_hash
    }
    pub fn midi_hash(&self) -> u32 {
        self.midi_hash
    }
    pub fn midi_events(&self) -> u64 {
        self.midi_events
    }
    pub fn callbacks(&self) -> u64 {
        self.callbacks
    }
    pub fn max_copy(&self) -> u32 {
        self.max_copy as u32
    }
    pub fn max_steps(&self) -> u32 {
        self.max_steps as u32
    }
}
impl Bridge {
    pub fn load(&mut self, slot: usize, packet: Packet) -> Result<(), Error> {
        guarded(|| {
            if slot >= SLOTS {
                return Err(Error::InvalidId);
            }
            if self.occupied(slot) {
                return Err(Error::Busy);
            }
            self.packets[slot] = packet;
            self.ingest_inner(slot);
            Ok(())
        })
    }
    pub fn packet(&self, slot: usize) -> Packet {
        self.packets[slot]
    }
    pub fn output(&self) -> &[f32; QUANTUM] {
        &self.output
    }
    fn occupied(&self, slot: usize) -> bool {
        self.pending[slot].is_some() || self.ready & (1 << slot) != 0
    }
    fn reply(
        &mut self,
        slot: usize,
        status: u32,
        error: u32,
        data: &[u8],
        frame: Frame,
        epoch: Epoch,
    ) {
        let header = self.headers[slot];
        let mut packet = Packet::new(header.op | 0x80, slot, header.sequence, epoch.0, frame.0);
        put64(&mut packet.bytes, 16, self.stream);
        let mut body = [0u8; BYTES - HEADER];
        put32(&mut body, 0, status);
        put32(&mut body, 4, error);
        body[8..8 + data.len()].copy_from_slice(data);
        packet.body(&body[..8 + data.len()]);
        self.packets[slot] = packet;
        self.ready |= 1 << slot;
    }
    fn local_reply(&mut self, slot: usize, status: u32, data: &[u8]) {
        self.reply(
            slot,
            status,
            0,
            data,
            self.runtime.next_frame(),
            self.runtime.epoch(),
        );
    }
    fn ingest_inner(&mut self, slot: usize) {
        let packet = self.packets[slot];
        self.headers[slot] = Header {
            op: packet.bytes[6],
            sequence: get64(&packet.bytes, 8).unwrap_or(0),
            ..Header::default()
        };
        let result = (|| {
            let header = packet.header(slot, self.stream)?;
            if header.sequence <= self.last_sequence {
                return Err(Error::InvalidState);
            }
            self.last_sequence = header.sequence;
            self.headers[slot] = header;
            let local = matches!(
                header.op,
                PLAN_BEGIN
                    | PLAN_STEPS
                    | PLAN_EDGES
                    | PLAN_CANCEL
                    | CLOSE_CURSOR
                    | EXECUTION
                    | STATUS
            );
            if slot == NORMAL_SLOTS
                && !matches!(
                    header.op,
                    PLAN_CANCEL
                        | CANCEL_UPLOAD
                        | RELEASE
                        | STOP_CAPTURE
                        | RESET_MIDI
                        | CLOSE_CURSOR
                )
            {
                return Err(Error::InvalidState);
            }
            if local && header.cancelled {
                self.local_reply(slot, CANCELLED, &[]);
                return Ok(());
            }
            let Some(command) = self.decode(slot, header, &packet.bytes[HEADER..packet.len])?
            else {
                return Ok(());
            };
            let lane = if slot == NORMAL_SLOTS {
                Lane::Recovery
            } else {
                Lane::Normal
            };
            match self.control.submit_prepared(lane, command) {
                Ok(ticket) => {
                    if header.cancelled {
                        ticket.cancel();
                    }
                    self.pending[slot] = Some(ticket.sequence());
                    if let Some(index) = self.stage_pending[slot].take() {
                        self.stages[index].generation =
                            self.stages[index].generation.checked_add(1).unwrap_or(0);
                    }
                    drop(ticket);
                    Ok(())
                }
                Err(refusal) => {
                    self.recycle(slot, refusal.value, None);
                    self.cancel_export_reservation(slot);
                    Err(refusal.error)
                }
            }
        })();
        if let Err(error) = result {
            self.reply(
                slot,
                REJECTED,
                error_code(error),
                &[],
                self.runtime.next_frame(),
                self.runtime.epoch(),
            );
        }
    }
    fn stage_index(&self, data: &[u8]) -> Result<usize, Error> {
        let index = get32(data, 0)? as usize;
        let generation = get32(data, 4)?;
        let stage = self.stages.get(index).ok_or(Error::InvalidId)?;
        if stage.generation != generation || stage.plan.is_none() {
            return Err(Error::InvalidId);
        }
        Ok(index)
    }
    fn take_buffer(&mut self, slot: usize, len: usize) -> Result<Box<[u8]>, Error> {
        self.buffers[slot]
            .get_mut(len)
            .ok_or(Error::Capacity)?
            .take()
            .ok_or(Error::Busy)
    }
    fn resource(data: &[u8]) -> Result<ResourceId, Error> {
        ResourceId::from_parts(get32(data, 0)?, get32(data, 4)?)
    }
    fn upload(data: &[u8]) -> Result<UploadId, Error> {
        Ok(UploadId {
            slot: get32(data, 0)?,
            generation: get32(data, 4)?,
        })
    }
    fn snapshot(data: &[u8]) -> Result<SnapshotId, Error> {
        Ok(SnapshotId {
            slot: get32(data, 0)?,
            generation: get32(data, 4)?,
        })
    }
    fn capture(data: &[u8]) -> Result<CaptureId, Error> {
        Ok(CaptureId {
            slot: get32(data, 0)?,
            generation: get32(data, 4)?,
        })
    }
    fn format(value: u32) -> Result<Format, Error> {
        match value {
            0 => Ok(Format::Midi),
            1 => Ok(Format::Audio { channels: 1 }),
            _ => Err(Error::InvalidShape),
        }
    }
    fn export_index(&self, data: &[u8]) -> Result<usize, Error> {
        let index = get32(data, 0)? as usize;
        let generation = get32(data, 4)?;
        let export = self.exports.get(index).ok_or(Error::InvalidId)?;
        if export.generation != generation || export.cursor.is_none() {
            return Err(Error::InvalidId);
        }
        if export.busy {
            return Err(Error::Busy);
        }
        Ok(index)
    }
    fn exact(data: &[u8], expected: usize) -> Result<(), Error> {
        if data.len() == expected {
            Ok(())
        } else {
            Err(Error::InvalidShape)
        }
    }
    fn decode(
        &mut self,
        slot: usize,
        header: Header,
        data: &[u8],
    ) -> Result<Option<Command<Node>>, Error> {
        let request = match header.op {
            PLAN_BEGIN => {
                Self::exact(data, 8)?;
                let steps = get32(data, 0)? as usize;
                let edges = get32(data, 4)? as usize;
                if steps > 1 || edges > 4 {
                    return Err(Error::Capacity);
                }
                let index = self
                    .stages
                    .iter()
                    .position(|stage| stage.generation != 0 && stage.plan.is_none())
                    .ok_or(Error::Capacity)?;
                let mut plan = self.plans.pop().ok_or(Error::Capacity)?;
                if let Err(error) = plan.begin_descriptor() {
                    self.plans.push(plan);
                    return Err(error);
                }
                let generation = self.stages[index].generation;
                self.stages[index] = Stage {
                    generation,
                    steps,
                    edges,
                    epoch: Epoch(header.epoch),
                    plan: Some(plan),
                };
                let mut reply = [0; 8];
                put32(&mut reply, 0, index as u32);
                put32(&mut reply, 4, generation);
                self.local_reply(slot, STAGED, &reply);
                return Ok(None);
            }
            PLAN_STEPS | PLAN_EDGES => {
                let index = self.stage_index(data)?;
                let offset = get32(data, 8)? as usize;
                let count = get32(data, 12)? as usize;
                let width = if header.op == PLAN_STEPS { 12 } else { 24 };
                if count == 0 || count > 4 {
                    return Err(Error::Capacity);
                }
                Self::exact(data, 16 + count * width)?;
                let stage = &mut self.stages[index];
                let plan = stage.plan.as_mut().ok_or(Error::InvalidId)?;
                if header.op == PLAN_STEPS {
                    if count != 1
                        || offset != plan.descriptor().steps.len()
                        || offset + count > stage.steps
                        || !plan.descriptor().connections.is_empty()
                    {
                        return Err(Error::InvalidOffset);
                    }
                    let step = decode_step(&data[16..])?;
                    plan.push_step(step)?;
                } else {
                    if offset != plan.descriptor().connections.len()
                        || offset + count > stage.edges
                        || plan.descriptor().steps.len() != stage.steps
                    {
                        return Err(Error::InvalidOffset);
                    }
                    let endpoint = Endpoint {
                        node: None,
                        kind: PortType::Audio,
                        port: 0,
                    };
                    let mut edges = [Connection {
                        source: endpoint,
                        destination: endpoint,
                    }; 4];
                    for (edge, bytes) in edges[..count].iter_mut().zip(data[16..].chunks_exact(24))
                    {
                        *edge = decode_edge(bytes)?;
                    }
                    for edge in &edges[..count] {
                        plan.push_connection(*edge)?;
                    }
                }
                self.local_reply(slot, STAGED, &[]);
                return Ok(None);
            }
            PLAN_COMMIT => {
                Self::exact(data, 8)?;
                let index = self.stage_index(data)?;
                let stage = &mut self.stages[index];
                let plan = stage.plan.as_ref().ok_or(Error::InvalidId)?;
                if plan.descriptor().steps.len() != stage.steps
                    || plan.descriptor().connections.len() != stage.edges
                {
                    return Err(Error::Incomplete);
                }
                self.stage_pending[slot] = Some(index);
                return Ok(Some(Command::commit(
                    stage.epoch,
                    stage.plan.take().ok_or(Error::InvalidState)?,
                )));
            }
            PLAN_CANCEL => {
                Self::exact(data, 8)?;
                let index = self.stage_index(data)?;
                self.plans
                    .push(self.stages[index].plan.take().ok_or(Error::InvalidState)?);
                self.stages[index].generation =
                    self.stages[index].generation.checked_add(1).unwrap_or(0);
                self.local_reply(slot, STAGED, &[]);
                return Ok(None);
            }
            PARAMETER => {
                Self::exact(data, 16)?;
                let action = Action::Parameter {
                    node: NodeId::from_parts(get32(data, 0)?, get32(data, 4)?)?,
                    parameter: get32(data, 8)?,
                    value: f32::from_bits(get32(data, 12)?),
                };
                let payload = self.take_buffer(slot, 0)?;
                return Ok(Some(Command::Schedule {
                    at: Frame(header.frame),
                    action,
                    payload,
                }));
            }
            MIDI => {
                let node = NodeId::from_parts(get32(data, 0)?, get32(data, 4)?)?;
                let port = get32(data, 8)?;
                let len = get32(data, 12)? as usize;
                if len > 64 {
                    return Err(Error::Capacity);
                }
                Self::exact(data, 16 + len)?;
                let mut payload = self.take_buffer(slot, len)?;
                payload.copy_from_slice(&data[16..]);
                return Ok(Some(Command::Schedule {
                    at: Frame(header.frame),
                    action: Action::Midi { node, port },
                    payload,
                }));
            }
            EXECUTION => {
                Self::exact(data, 0)?;
                if let Some(execution) = self.control.try_execution() {
                    let mut reply = [0; 32];
                    put64(&mut reply, 0, execution.sequence.0);
                    put64(&mut reply, 8, execution.actual_frame.0);
                    put32(&mut reply, 16, execution.result.err().map_or(0, error_code));
                    self.local_reply(slot, EXECUTED, &reply);
                } else {
                    self.local_reply(slot, PENDING, &[]);
                }
                return Ok(None);
            }
            STATUS => {
                Self::exact(data, 0)?;
                let mut reply = [0; 80];
                let usage = self.runtime.resource_usage();
                for (offset, value) in [
                    (0, usage.free_pages),
                    (4, usage.reserved_pages),
                    (8, usage.free_versions),
                    (12, usage.free_snapshots),
                    (16, usage.retired_versions),
                    (20, usage.retired_pages),
                    (24, self.plans.len()),
                    (28, self.control.outstanding()),
                    (
                        32,
                        self.stages
                            .iter()
                            .filter(|stage| stage.plan.is_some())
                            .count(),
                    ),
                    (
                        36,
                        self.exports
                            .iter()
                            .filter(|export| export.cursor.is_some() || export.busy)
                            .count(),
                    ),
                ] {
                    put32(&mut reply, offset, value as u32);
                }
                put64(&mut reply, 40, self.node_frames);
                put32(&mut reply, 48, self.sample_hash);
                put32(&mut reply, 52, self.midi_hash);
                put64(&mut reply, 56, self.midi_events);
                put32(&mut reply, 64, self.max_copy as u32);
                put32(&mut reply, 68, self.max_steps as u32);
                self.local_reply(slot, APPLIED, &reply);
                return Ok(None);
            }
            CREATE => {
                Self::exact(data, 0)?;
                Request::Create
            }
            UPLOAD => {
                Self::exact(data, 32)?;
                Request::BeginUpload {
                    resource: Self::resource(data)?,
                    expected_revision: Revision(get64(data, 8)?),
                    description: Description {
                        format: Self::format(get32(data, 16)?)?,
                        frames: get64(data, 20)?,
                        bytes: get32(data, 28)? as usize,
                    },
                }
            }
            WRITE => {
                let upload = Self::upload(data)?;
                let offset = get32(data, 8)? as usize;
                let len = get32(data, 12)? as usize;
                if len > 64 {
                    return Err(Error::Capacity);
                }
                Self::exact(data, 16 + len)?;
                let mut bytes = self.take_buffer(slot, len)?;
                bytes.copy_from_slice(&data[16..]);
                Request::Write {
                    upload,
                    offset,
                    bytes,
                }
            }
            FINISH | COMMIT | CANCEL_UPLOAD => {
                Self::exact(data, 8)?;
                let upload = Self::upload(data)?;
                match header.op {
                    FINISH => Request::Finish { upload },
                    COMMIT => Request::Commit { upload },
                    _ => Request::CancelUpload { upload },
                }
            }
            SNAPSHOT => {
                Self::exact(data, 8)?;
                Request::Snapshot {
                    resource: Self::resource(data)?,
                }
            }
            CURSOR => {
                Self::exact(data, 8)?;
                let snapshot = Self::snapshot(data)?;
                let index = self
                    .exports
                    .iter()
                    .position(|export| {
                        export.generation != 0 && export.cursor.is_none() && !export.busy
                    })
                    .ok_or(Error::Capacity)?;
                self.exports[index].busy = true;
                self.export_pending[slot] = Some(index);
                self.export_new[slot] = true;
                Request::Cursor { snapshot }
            }
            READ => {
                Self::exact(data, 12)?;
                let index = self.export_index(data)?;
                let len = get32(data, 8)? as usize;
                if len == 0 || len > 64 {
                    return Err(Error::Capacity);
                }
                let cursor = self.exports[index].cursor.ok_or(Error::InvalidId)?;
                let bytes = self.take_buffer(slot, len)?;
                self.exports[index].busy = true;
                self.export_pending[slot] = Some(index);
                self.export_new[slot] = false;
                Request::Read { cursor, bytes }
            }
            CLOSE_CURSOR => {
                Self::exact(data, 8)?;
                let index = self.export_index(data)?;
                self.release_export(index);
                self.local_reply(slot, APPLIED, &[]);
                return Ok(None);
            }
            RELEASE | CURRENTNESS => {
                Self::exact(data, 8)?;
                let snapshot = Self::snapshot(data)?;
                if header.op == RELEASE {
                    Request::ReleaseSnapshot { snapshot }
                } else {
                    Request::Currentness { snapshot }
                }
            }
            INFO => {
                Self::exact(data, 8)?;
                Request::Info {
                    resource: Self::resource(data)?,
                }
            }
            CAPTURE => {
                Self::exact(data, 20)?;
                Request::BeginCapture {
                    resource: Self::resource(data)?,
                    expected_revision: Revision(get64(data, 8)?),
                    format: Self::format(get32(data, 16)?)?,
                }
            }
            STOP_CAPTURE => {
                Self::exact(data, 8)?;
                Request::StopCapture {
                    capture: Self::capture(data)?,
                }
            }
            REMOVE_RESOURCE => {
                Self::exact(data, 8)?;
                Request::Remove {
                    resource: Self::resource(data)?,
                }
            }
            REMOVE_NODE => {
                Self::exact(data, 8)?;
                return Ok(Some(Command::Remove {
                    node: NodeId::from_parts(get32(data, 0)?, get32(data, 4)?)?,
                }));
            }
            RESET_MIDI => {
                Self::exact(data, 0)?;
                return Ok(Some(Command::ResetMidi));
            }
            _ => return Err(Error::InvalidEvent),
        };
        Ok(Some(Command::resource(request)))
    }
    fn release_export(&mut self, index: usize) {
        self.exports[index].cursor = None;
        self.exports[index].busy = false;
        self.exports[index].generation = self.exports[index].generation.checked_add(1).unwrap_or(0);
    }
    fn cancel_export_reservation(&mut self, slot: usize) {
        if let Some(index) = self.export_pending[slot].take() {
            if self.export_new[slot] {
                self.release_export(index);
            } else {
                self.exports[index].busy = false;
            }
        }
        self.export_new[slot] = false;
    }
    fn return_buffer(&mut self, slot: usize, bytes: Box<[u8]>) {
        let len = bytes.len();
        assert!(self.buffers[slot][len].is_none());
        self.buffers[slot][len] = Some(bytes);
    }
    fn recycle(&mut self, slot: usize, command: Command<Node>, retired: Option<Retired<Node>>) {
        match command {
            Command::Commit {
                plan: Some(plan), ..
            } => {
                if let Some(index) = self.stage_pending[slot].take() {
                    self.stages[index].plan = Some(plan);
                } else {
                    self.plans.push(plan);
                }
            }
            Command::Schedule { payload, .. } => self.return_buffer(slot, payload),
            Command::Resource(command) => match command.into_request() {
                Request::Write { bytes, .. } | Request::Read { bytes, .. } => {
                    self.return_buffer(slot, bytes)
                }
                _ => {}
            },
            _ => {}
        }
        match retired {
            Some(Retired::Plan(plan)) => self.plans.push(plan),
            Some(Retired::Processor(node)) => {
                assert!(self.parked.is_none());
                assert_eq!(node.frames, self.node_frames);
                self.parked = Some(node);
            }
            None => {}
        }
        assert!(self.plans.len() <= PLAN_POOL);
    }
    fn describe(description: Description, bytes: &mut [u8], offset: usize) {
        put32(
            bytes,
            offset,
            match description.format {
                Format::Midi => 0,
                Format::Audio { channels } => channels,
            },
        );
        put64(bytes, offset + 4, description.frames);
        put32(bytes, offset + 12, description.bytes as u32);
    }
    fn capture_info(info: Option<CaptureInfo>, bytes: &mut [u8], offset: usize) {
        if let Some(info) = info {
            put32(bytes, offset, 1);
            put32(bytes, offset + 4, u32::from(info.active));
            put32(bytes, offset + 8, info.incomplete.map_or(0, error_code));
            put64(bytes, offset + 12, info.start_frame.0);
            put64(bytes, offset + 20, info.next_frame.0);
        }
    }
    fn completion(&mut self, completion: Returned) -> Result<(), Error> {
        match self.encode_completion(&completion) {
            Ok(slot) => {
                self.recycle(slot, completion.request, completion.retired);
                Ok(())
            }
            Err(error) => {
                self.emergency = Some(completion);
                Err(error)
            }
        }
    }
    fn encode_completion(&mut self, completion: &Returned) -> Result<usize, Error> {
        let slot = self
            .pending
            .iter()
            .position(|pending| *pending == Some(completion.sequence))
            .ok_or(Error::InvalidState)?;
        self.pending[slot] = None;
        let mut data = [0u8; 160];
        let mut len = 0;
        let (status, error) = match completion.outcome {
            Outcome::Applied => (APPLIED, 0),
            Outcome::Cancelled => (CANCELLED, 0),
            Outcome::Rejected(error) => (REJECTED, error_code(error)),
            Outcome::Scheduled {
                effective_frame,
                late_by,
            } => {
                put64(&mut data, 0, completion.sequence.0);
                put64(&mut data, 8, effective_frame.0);
                put64(&mut data, 16, late_by);
                len = 24;
                (SCHEDULED, 0)
            }
        };
        if completion.outcome == Outcome::Applied {
            if let Command::Resource(command) = &completion.request {
                match command.reply().ok_or(Error::InvalidState)? {
                    Reply::Done => {}
                    Reply::Created(id) => {
                        put32(&mut data, 0, id.slot());
                        put32(&mut data, 4, id.generation());
                        len = 8;
                    }
                    Reply::Upload(id) => {
                        put32(&mut data, 0, id.slot);
                        put32(&mut data, 4, id.generation);
                        len = 8;
                    }
                    Reply::Capture(id) => {
                        put32(&mut data, 0, id.slot);
                        put32(&mut data, 4, id.generation);
                        len = 8;
                    }
                    Reply::Revision(revision) => {
                        put64(&mut data, 0, revision.0);
                        len = 8;
                    }
                    Reply::Snapshot(snapshot) => {
                        put32(&mut data, 0, snapshot.id.slot);
                        put32(&mut data, 4, snapshot.id.generation);
                        put64(&mut data, 8, snapshot.revision.0);
                        put64(&mut data, 16, snapshot.cut_frame.0);
                        Self::describe(snapshot.description, &mut data, 24);
                        Self::capture_info(snapshot.capture, &mut data, 40);
                        len = 68;
                    }
                    Reply::Info(info) => {
                        put64(&mut data, 0, info.revision.0);
                        Self::describe(info.description, &mut data, 8);
                        Self::capture_info(info.capture, &mut data, 24);
                        len = 52;
                    }
                    Reply::Currentness(currentness) => {
                        put32(
                            &mut data,
                            0,
                            match currentness {
                                Currentness::Current => 0,
                                Currentness::Stale => 1,
                                Currentness::ResourceRemoved => 2,
                            },
                        );
                        len = 4;
                    }
                    Reply::Cursor(cursor) => {
                        let index = self.export_pending[slot]
                            .take()
                            .ok_or(Error::InvalidState)?;
                        self.exports[index].cursor = Some(cursor);
                        self.exports[index].busy = false;
                        self.export_new[slot] = false;
                        put32(&mut data, 0, index as u32);
                        put32(&mut data, 4, self.exports[index].generation);
                        len = 8;
                    }
                    Reply::Read { written, eof } => {
                        let index = self.export_pending[slot]
                            .take()
                            .ok_or(Error::InvalidState)?;
                        let Request::Read { cursor, bytes } = command.request() else {
                            return Err(Error::InvalidState);
                        };
                        self.exports[index].cursor = Some(*cursor);
                        self.exports[index].busy = false;
                        put32(&mut data, 0, written as u32);
                        put32(&mut data, 4, u32::from(eof));
                        data[8..8 + written].copy_from_slice(&bytes[..written]);
                        len = 8 + written;
                    }
                }
            }
            if let Some(Retired::Processor(node)) = &completion.retired {
                put32(&mut data, 0, node.snapshot.id.slot);
                put32(&mut data, 4, node.snapshot.id.generation);
                put64(&mut data, 8, node.frames);
                len = 16;
            }
        } else {
            self.cancel_export_reservation(slot);
        }
        self.reply(
            slot,
            status,
            error,
            &data[..len],
            completion.frame,
            completion.epoch,
        );
        Ok(slot)
    }
    fn render_inner(&mut self) -> Result<(), Error> {
        let frame = self.runtime.next_frame();
        self.runtime.process(
            AudioBlock {
                frame,
                frames: QUANTUM,
                inputs: &[],
                outputs: &mut [&mut self.output[..]],
            },
            MidiBlock {
                inputs: &self.inputs,
                outputs: &mut self.outputs,
            },
        )?;
        let diagnostics = self.runtime.diagnostics();
        if diagnostics.nodes_processed != 0 {
            self.node_frames += QUANTUM as u64;
        }
        self.max_copy = self.max_copy.max(diagnostics.transfer_bytes);
        self.max_steps = self.max_steps.max(diagnostics.validation_steps);
        if diagnostics.transfer_bytes > 2048 || diagnostics.validation_steps > 512 {
            return Err(Error::Budget);
        }
        for sample in &*self.output {
            self.sample_hash = (self.sample_hash ^ sample.to_bits()).wrapping_mul(16777619);
        }
        self.recovery.clear();
        self.runtime
            .drain_midi_recovery(0, &mut self.recovery, QUANTUM)?;
        for port in self.outputs.iter().chain(std::iter::once(&self.recovery)) {
            for index in 0..port.len() {
                let (offset, bytes) = port.get(index).ok_or(Error::InvalidEvent)?;
                for byte in frame
                    .checked_add(offset as usize)?
                    .0
                    .to_le_bytes()
                    .iter()
                    .chain(bytes)
                {
                    self.midi_hash = (self.midi_hash ^ u32::from(*byte)).wrapping_mul(16777619);
                }
                self.midi_events += 1;
            }
        }
        for _ in 0..SLOTS {
            let Some(completion) = self.control.try_completion() else {
                break;
            };
            self.completion(completion)?;
        }
        self.callbacks += 1;
        Ok(())
    }
}
