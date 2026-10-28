use crate::{
    export_wire::{get32, put32, put64},
    scenario::Mix,
    staging_wire::*,
    wire,
};
use rt_runtime::{
    executor::{Command, Retired},
    resource::{self, Cursor, Snapshot, UploadId},
    schedule::Execution,
    transport::{Completion, Lane, Outcome, Ticket},
    Error, Frame, ResourceId, Revision, Sequence,
};
use shoop_loop_engine::{
    runtime::{
        Controller, Failure, LoopPlanner, Node, Operation, Reply, RoleSourceId, RoleSourceStatus,
    },
    topology::staging::{StageId, Status as ProgramStatus},
    LoopId, Timing, Transition,
};
type Returned = Completion<Command<Node<Mix>, LoopPlanner>, Retired<Node<Mix>>>;
struct ResourceBank {
    generation: u32,
    resource: Option<ResourceId>,
    upload: Option<UploadId>,
    snapshot: Option<Snapshot>,
    cursor: Option<Cursor>,
    source: Option<RoleSourceId>,
}
impl ResourceBank {
    fn flags(&self) -> u32 {
        u32::from(self.resource.is_some())
            | u32::from(self.upload.is_some()) * 2
            | u32::from(self.snapshot.is_some()) * 4
            | u32::from(self.source.is_some()) * 8
    }
}
struct ProgramBank {
    generation: u32,
    id: Option<StageId>,
}
struct Pending {
    request: Request,
    ticket: Option<Ticket>,
    effective: u64,
    admission: Option<Response>,
    terminal: Option<Response>,
    sent: bool,
}
pub(crate) struct Staging {
    session: u64,
    last: u64,
    resources: [ResourceBank; BANKS],
    programs: [ProgramBank; BANKS],
    catalog: [Option<shoop_loop_engine::media::MediaId>; BANKS],
    buffer: Option<Box<[u8]>>,
    pending: Option<Pending>,
}
impl Staging {
    pub(crate) fn new(session: u64) -> Self {
        Self {
            session,
            last: 0,
            resources: std::array::from_fn(|_| ResourceBank {
                generation: 1,
                resource: None,
                upload: None,
                snapshot: None,
                cursor: None,
                source: None,
            }),
            programs: std::array::from_fn(|_| ProgramBank {
                generation: 1,
                id: None,
            }),
            catalog: [None; BANKS],
            buffer: Some(vec![0; 64].into_boxed_slice()),
            pending: None,
        }
    }
    pub(crate) fn set_catalog(
        &mut self,
        catalog: [Option<shoop_loop_engine::media::MediaId>; BANKS],
    ) {
        self.catalog = catalog;
    }
    pub(crate) fn outstanding(&self) -> usize {
        usize::from(self.pending.is_some())
    }
    pub(crate) fn retained(&self) -> usize {
        self.resources
            .iter()
            .map(|b| b.flags().count_ones() as usize)
            .sum::<usize>()
            + self.programs.iter().filter(|b| b.id.is_some()).count()
    }
    pub(crate) fn matches(&self, sequence: Sequence) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.ticket.as_ref().is_some_and(|t| t.sequence() == sequence))
    }
    fn flags(&self, r: Request) -> u32 {
        if program(r.kind) {
            u32::from(self.programs[r.bank as usize].id.is_some()) * 256
        } else {
            self.resources[r.bank as usize].flags()
        }
    }
    fn generation(&self, r: Request) -> u32 {
        if program(r.kind) {
            self.programs[r.bank as usize].generation
        } else {
            self.resources[r.bank as usize].generation
        }
    }
    fn response(&self, phase: u8, status: u32, observed: u64) -> Response {
        let p = self.pending.as_ref().unwrap();
        let mut b = Response::new(
            p.request,
            phase,
            status,
            p.ticket.as_ref().map_or(0, |t| t.sequence().0),
            p.effective,
            observed,
        );
        put32(&mut b.0, 132, self.flags(p.request));
        put32(&mut b.0, 80, self.generation(p.request));
        b
    }
    fn operation(&self, r: Request) -> Result<Operation, wire::Error> {
        let bank = r.bank as usize;
        let stage = || self.programs[bank].id.ok_or(wire::Error::State);
        let source = || self.resources[bank].source.ok_or(wire::Error::State);
        Ok(match r.kind {
            SOURCE => {
                if self.resources[bank].source.is_some() {
                    return Err(wire::Error::Busy);
                }
                Operation::RoleSnapshot {
                    resource: self.resources[bank].resource.ok_or(wire::Error::State)?,
                }
            }
            VALIDATE_SOURCE => Operation::RoleSourceValidate { id: source()? },
            RELEASE_SOURCE => Operation::RoleSourceRelease { id: source()? },
            PROGRAM_BEGIN => {
                if self.programs[bank].id.is_some() {
                    return Err(wire::Error::Busy);
                }
                Operation::TopologyBegin { slot: bank }
            }
            APPEND => Operation::TopologyAppend {
                id: stage()?,
                first_record: offset(r.args[0])?,
            },
            VALIDATE => Operation::TopologyValidate { id: stage()? },
            STATUS => Operation::TopologyStatus { id: stage()? },
            ACTIVATE => Operation::TopologyActivate {
                id: stage()?,
                graph_epoch: rt_runtime::Epoch(r.args[0]),
                topology_epoch: rt_runtime::Epoch(r.args[1]),
            },
            RECYCLE => Operation::TopologyRecycle { id: stage()? },
            CANCEL => Operation::TopologyCancel { id: stage()? },
            SELECT => Operation::Select {
                target: LoopId::from_parts(r.args[0] as u32, (r.args[0] >> 32) as u32)
                    .map_err(|_| wire::Error::Identity)?,
                timing: Timing::At,
                media: *self
                    .catalog
                    .get(usize::try_from(r.args[1]).map_err(|_| wire::Error::Slot)?)
                    .ok_or(wire::Error::Slot)?
                    .as_ref()
                    .ok_or(wire::Error::State)?,
                preserve_position: false,
            },
            RECORD_AFTER => Operation::Transition {
                target: LoopId::from_parts(r.args[0] as u32, (r.args[0] >> 32) as u32)
                    .map_err(|_| wire::Error::Identity)?,
                timing: Timing::AfterTicks {
                    source: LoopId::from_parts(r.args[1] as u32, (r.args[1] >> 32) as u32)
                        .map_err(|_| wire::Error::Identity)?,
                    skip: r.args[2],
                },
                transition: Transition::Record {
                    monitor_previous: false,
                },
            },
            QUERY_STATE => Operation::State {
                target: LoopId::from_parts(r.args[0] as u32, (r.args[0] >> 32) as u32)
                    .map_err(|_| wire::Error::Identity)?,
            },
            PLAY | STOP | SEEK_ITERATION => Operation::Transition {
                target: LoopId::from_parts(r.args[0] as u32, (r.args[0] >> 32) as u32)
                    .map_err(|_| wire::Error::Identity)?,
                timing: Timing::At,
                transition: if r.kind == SEEK_ITERATION {
                    Transition::SeekIteration {
                        iteration: r.args[1],
                        retrigger: false,
                    }
                } else if r.kind == PLAY {
                    Transition::Play {
                        position: 0,
                        retrigger: false,
                    }
                } else {
                    Transition::Stop
                },
            },
            _ => return Err(wire::Error::Operation),
        })
    }
    fn resource_request(&mut self, r: Request) -> Result<resource::Request, wire::Error> {
        let bank = &self.resources[r.bank as usize];
        let id = || bank.resource.ok_or(wire::Error::State);
        let upload = || bank.upload.ok_or(wire::Error::State);
        let snapshot = || bank.snapshot.ok_or(wire::Error::State);
        Ok(match r.kind {
            CREATE => {
                if bank.resource.is_some() {
                    return Err(wire::Error::Busy);
                }
                resource::Request::Create
            }
            BEGIN => {
                if bank.upload.is_some() {
                    return Err(wire::Error::Busy);
                }
                resource::Request::BeginUpload {
                    resource: id()?,
                    expected_revision: Revision(r.args[0]),
                    description: resource::Description {
                        format: if r.data[0] == 1 {
                            resource::Format::Audio {
                                channels: get32(&r.data, 4),
                            }
                        } else {
                            resource::Format::Midi
                        },
                        frames: r.args[1],
                        bytes: offset(r.args[2])?,
                    },
                }
            }
            WRITE => {
                let upload = upload()?;
                let offset = offset(r.args[0])?;
                let mut bytes = self.buffer.take().ok_or(wire::Error::State)?;
                bytes.copy_from_slice(&r.data);
                resource::Request::Write {
                    upload,
                    offset,
                    bytes,
                }
            }
            FINISH => resource::Request::Finish { upload: upload()? },
            COMMIT => resource::Request::Commit { upload: upload()? },
            CANCEL_UPLOAD => resource::Request::CancelUpload { upload: upload()? },
            SNAPSHOT => {
                if bank.snapshot.is_some() {
                    return Err(wire::Error::Busy);
                }
                resource::Request::Snapshot { resource: id()? }
            }
            CURSOR => resource::Request::Cursor {
                snapshot: snapshot()?.id,
            },
            READ => {
                let cursor = bank.cursor.ok_or(wire::Error::State)?;
                if cursor.position() as u64 != r.args[0] {
                    return Err(wire::Error::State);
                }
                let mut bytes = self.buffer.take().ok_or(wire::Error::State)?;
                bytes.fill(0);
                resource::Request::Read { cursor, bytes }
            }
            RELEASE => resource::Request::ReleaseSnapshot {
                snapshot: snapshot()?.id,
            },
            REMOVE => {
                if bank.snapshot.is_some() || bank.upload.is_some() || bank.source.is_some() {
                    return Err(wire::Error::Busy);
                }
                resource::Request::Remove { resource: id()? }
            }
            _ => return Err(wire::Error::Operation),
        })
    }
    pub(crate) fn ingest(
        &mut self,
        control: &mut Controller<Mix>,
        bytes: &[u8],
        now: u64,
    ) -> Result<(), wire::Error> {
        let r = Request::decode(bytes)?;
        if r.session != self.session {
            return Err(wire::Error::Session);
        }
        if r.sequence <= self.last {
            return Err(wire::Error::Sequence);
        }
        if self.pending.is_some() {
            return Err(wire::Error::Busy);
        }
        if r.generation != self.generation(r) {
            return Err(wire::Error::Identity);
        }
        let scalar = if resource(r.kind) {
            None
        } else {
            Some(self.operation(r)?)
        };
        let submitted = if let Some(operation) = scalar {
            if matches!(r.kind, PROGRAM_BEGIN | APPEND) {
                let mut payload = self.buffer.take().ok_or(wire::Error::State)?;
                payload.copy_from_slice(&r.data);
                control.submit_payload(Lane::Normal, Frame(r.at), operation, payload)
            } else {
                control.submit(Lane::Normal, Frame(r.at), operation)
            }
        } else {
            let request = self.resource_request(r)?;
            control.resource(Lane::Normal, request)
        };
        self.pending = Some(Pending {
            request: r,
            ticket: None,
            effective: r.at,
            admission: None,
            terminal: None,
            sent: false,
        });
        match submitted {
            Ok(ticket) => {
                if r.cancelled {
                    ticket.cancel();
                }
                self.pending.as_mut().unwrap().ticket = Some(ticket);
            }
            Err(refusal) => {
                let mut response =
                    self.response(if resource(r.kind) { 3 } else { 1 }, REJECTED, now);
                put32(&mut response.0, 68, wire::runtime_error(refusal.error));
                match refusal.value {
                    Command::Resource(command) => match command.into_request() {
                        resource::Request::Write { bytes, .. }
                        | resource::Request::Read { bytes, .. } => {
                            if r.kind == WRITE {
                                response.0[DATA..DATA + 64].copy_from_slice(&bytes);
                                put32(&mut response.0, 128, 64);
                            }
                            self.buffer = Some(bytes);
                        }
                        _ => {}
                    },
                    Command::Schedule { payload, .. } => {
                        if owning(r.kind) {
                            response.0[DATA..DATA + 64].copy_from_slice(&payload);
                            put32(&mut response.0, 128, 64);
                            self.buffer = Some(payload);
                        } else {
                            drop(payload);
                        }
                    }
                    _ => unreachable!(),
                }
                if resource(r.kind) {
                    self.pending.as_mut().unwrap().terminal = Some(response);
                } else {
                    self.pending.as_mut().unwrap().admission = Some(response);
                }
            }
        }
        self.last = r.sequence;
        Ok(())
    }
    pub(crate) fn completion(&mut self, completion: Returned) -> Result<(), Returned> {
        let Some(p) = self.pending.as_ref() else {
            return Err(completion);
        };
        let r = p.request;
        if p.ticket.as_ref().map(|t| t.sequence()) != Some(completion.sequence)
            || completion.retired.is_some()
            || p.admission.is_some()
            || p.sent
            || p.terminal.is_some()
        {
            return Err(completion);
        }
        if resource(r.kind) {
            let Command::Resource(command) = &completion.request else {
                return Err(completion);
            };
            let payload = match command.request() {
                resource::Request::Write { bytes, .. } | resource::Request::Read { bytes, .. } => {
                    Some(bytes.as_ref())
                }
                _ => None,
            };
            if owning(r.kind) != (payload.is_some())
                || payload.is_some_and(|b| b.len() != 64 || self.buffer.is_some())
            {
                return Err(completion);
            }
            let mut response = self.response(3, REJECTED, completion.frame.0);
            match completion.outcome {
                Outcome::Applied => {
                    let bank = &mut self.resources[r.bank as usize];
                    let Some(reply) = command.reply() else {
                        return Err(completion);
                    };
                    match (r.kind, reply) {
                        (CREATE, resource::Reply::Created(id)) => bank.resource = Some(id),
                        (BEGIN, resource::Reply::Upload(id)) => bank.upload = Some(id),
                        (COMMIT, resource::Reply::Revision(revision)) => {
                            bank.upload = None;
                            put64(&mut response.0, 88, revision.0);
                        }
                        (SNAPSHOT, resource::Reply::Snapshot(snapshot)) => {
                            bank.snapshot = Some(snapshot);
                            put64(&mut response.0, 88, snapshot.revision.0);
                            put64(&mut response.0, 96, snapshot.description.bytes as u64);
                            put64(&mut response.0, 104, snapshot.description.frames);
                            put64(
                                &mut response.0,
                                112,
                                match snapshot.description.format {
                                    resource::Format::Audio { channels } => u64::from(channels),
                                    resource::Format::Midi => 0,
                                },
                            );
                            put64(&mut response.0, 120, snapshot.cut_frame.0);
                        }
                        (CURSOR, resource::Reply::Cursor(cursor)) => {
                            bank.cursor = Some(cursor);
                            put64(&mut response.0, 88, cursor.position() as u64);
                        }
                        (READ, resource::Reply::Read { written, eof }) => {
                            let resource::Request::Read { cursor, bytes } = command.request()
                            else {
                                return Err(completion);
                            };
                            if written > 64
                                || Some(cursor.position() as u64)
                                    != r.args[0].checked_add(written as u64)
                            {
                                return Err(completion);
                            }
                            response.0[DATA..DATA + written].copy_from_slice(&bytes[..written]);
                            put32(&mut response.0, 128, written as u32);
                            put32(&mut response.0, 84, if eof { 2 } else { 0 });
                            put64(&mut response.0, 88, cursor.position() as u64);
                            bank.cursor = Some(*cursor);
                        }
                        (WRITE | FINISH, resource::Reply::Done) => {}
                        (CANCEL_UPLOAD, resource::Reply::Done) => bank.upload = None,
                        (RELEASE, resource::Reply::Done) => {
                            bank.snapshot = None;
                            bank.cursor = None;
                        }
                        (REMOVE, resource::Reply::Done) => {
                            bank.resource = None;
                            bank.generation = bank.generation.checked_add(1).unwrap_or(0);
                        }
                        _ => return Err(completion),
                    }
                    put32(&mut response.0, 64, APPLIED);
                }
                Outcome::Cancelled => put32(&mut response.0, 64, CANCELLED),
                Outcome::Rejected(error) => put32(&mut response.0, 68, wire::runtime_error(error)),
                _ => return Err(completion),
            }
            if r.kind == WRITE {
                response.0[DATA..DATA + 64].copy_from_slice(payload.unwrap());
                put32(&mut response.0, 128, 64);
            }
            let Command::Resource(command) = completion.request else {
                unreachable!()
            };
            match command.into_request() {
                resource::Request::Read { bytes, .. } | resource::Request::Write { bytes, .. } => {
                    self.buffer = Some(bytes)
                }
                _ => {}
            }
            put32(&mut response.0, 132, self.flags(r));
            put32(&mut response.0, 80, self.generation(r));
            self.pending.as_mut().unwrap().terminal = Some(response);
        } else {
            let expected = owning(r.kind);
            if !matches!(&completion.request,Command::Schedule{payload,..}if payload.len()==if expected{64}else{0})
                || expected && self.buffer.is_some()
            {
                return Err(completion);
            }
            let mut response = self.response(1, REJECTED, completion.frame.0);
            match completion.outcome {
                Outcome::Scheduled {
                    effective_frame, ..
                } => {
                    self.pending.as_mut().unwrap().effective = effective_frame.0;
                    put64(&mut response.0, 40, effective_frame.0);
                    put32(&mut response.0, 64, ADMITTED);
                }
                Outcome::Cancelled => put32(&mut response.0, 64, CANCELLED),
                Outcome::Rejected(error) => put32(&mut response.0, 68, wire::runtime_error(error)),
                _ => return Err(completion),
            }
            let Command::Schedule { payload, .. } = completion.request else {
                unreachable!()
            };
            if expected {
                response.0[DATA..DATA + 64].copy_from_slice(&payload);
                put32(&mut response.0, 128, 64);
                self.buffer = Some(payload);
            } else {
                drop(payload);
            }
            self.pending.as_mut().unwrap().admission = Some(response);
        }
        Ok(())
    }
    pub(crate) fn execution(
        &mut self,
        execution: Execution<Operation, Reply>,
    ) -> Result<(), Error> {
        let p = self.pending.as_ref().ok_or(Error::InvalidState)?;
        if p.ticket.as_ref().map(|t| t.sequence()) != Some(execution.sequence)
            || p.terminal.is_some()
        {
            return Err(Error::InvalidState);
        }
        let r = p.request;
        let index = r.bank as usize;
        let mut response = self.response(2, FAILED, execution.actual_frame.0);
        if let Some(frame) = execution.delivered_frame {
            put32(&mut response.0, 84, 1);
            put64(&mut response.0, 48, frame.0);
        }
        match execution.result {
            Err(error) => put32(&mut response.0, 68, wire::runtime_error(error)),
            Ok(()) => match execution.operation_result {
                Some(Reply::Failed(error)) => put32(
                    &mut response.0,
                    68,
                    match error {
                        Failure::Runtime(e) => 512 + wire::runtime_error(e),
                        Failure::Timeline(e) => wire::loop_error(e),
                    },
                ),
                Some(reply) => {
                    match (r.kind, reply) {
                        (
                            SOURCE,
                            Reply::RoleSource {
                                id,
                                snapshot,
                                status,
                            },
                        ) => {
                            self.resources[index].source = Some(id);
                            put64(&mut response.0, 104, snapshot.revision.0);
                            source_status(&mut response, status);
                        }
                        (VALIDATE_SOURCE, Reply::RoleSourceStatus { status, .. }) => {
                            source_status(&mut response, status)
                        }
                        (RELEASE_SOURCE, Reply::RoleSourceReleased { .. }) => {
                            self.resources[index].source = None
                        }
                        (PROGRAM_BEGIN, Reply::TopologyStatus { id, status }) => {
                            self.programs[index].id = Some(id);
                            program_status(&mut response, status);
                        }
                        (APPEND | STATUS, Reply::TopologyStatus { status, .. }) => {
                            program_status(&mut response, status)
                        }
                        (VALIDATE, Reply::TopologyProgress { progress, .. }) => {
                            put64(&mut response.0, 88, u64::from(progress.ready));
                            put64(&mut response.0, 96, progress.total_steps);
                            put64(&mut response.0, 104, progress.used as u64);
                        }
                        (
                            ACTIVATE,
                            Reply::TopologyActivated {
                                graph_epoch,
                                topology_epoch,
                                ..
                            },
                        ) => {
                            put64(&mut response.0, 88, graph_epoch.0);
                            put64(&mut response.0, 96, topology_epoch.0);
                        }
                        (
                            RECYCLE | CANCEL,
                            Reply::TopologyStatus {
                                status: ProgramStatus::Idle,
                                ..
                            },
                        ) => {
                            self.programs[index].id = None;
                            self.programs[index].generation =
                                self.programs[index].generation.checked_add(1).unwrap_or(0);
                        }
                        (QUERY_STATE, Reply::State(cut)) => {
                            put64(&mut response.0, 88, cut.frame.0);
                            put64(&mut response.0, 96, cut.state.capture_frames);
                            put64(&mut response.0, 104, cut.state.capture_revision.0);
                            let mode = match cut.state.mode {
                                shoop_loop_engine::Mode::Stopped => 0,
                                shoop_loop_engine::Mode::Playing => 1,
                                shoop_loop_engine::Mode::Recording => 2,
                            };
                            put64(
                                &mut response.0,
                                112,
                                mode | u64::from(cut.state.capture_incomplete) * 4
                                    | u64::from(cut.state.monitor_previous) * 8,
                            );
                            put64(
                                &mut response.0,
                                120,
                                cut.capture_failure
                                    .map_or(0, |e| u64::from(wire::runtime_error(e))),
                            );
                        }
                        (
                            PLAY | STOP | SEEK_ITERATION | SELECT | RECORD_AFTER,
                            Reply::Transition(completion),
                        ) => match completion.outcome {
                            shoop_loop_engine::Outcome::Applied { frame } => {
                                put64(&mut response.0, 88, frame.0)
                            }
                            _ => return Err(Error::InvalidState),
                        },
                        _ => return Err(Error::InvalidState),
                    }
                    put32(&mut response.0, 64, APPLIED);
                }
                None => return Err(Error::InvalidState),
            },
        }
        put32(&mut response.0, 132, self.flags(r));
        put32(&mut response.0, 80, self.generation(r));
        self.pending.as_mut().unwrap().terminal = Some(response);
        Ok(())
    }
    pub(crate) fn next_response(&mut self) -> Result<Option<[u8; RESPONSE_BYTES]>, wire::Error> {
        let Some(p) = self.pending.as_mut() else {
            return Ok(None);
        };
        if let Some(response) = p.admission.take() {
            Response::decode(&response.0)?;
            if get32(&response.0, 64) == ADMITTED {
                p.sent = true;
            } else {
                self.pending = None;
            }
            return Ok(Some(response.0));
        }
        if let Some(response) = p.terminal.filter(|_| p.sent || resource(p.request.kind)) {
            Response::decode(&response.0)?;
            self.pending = None;
            return Ok(Some(response.0));
        }
        Ok(None)
    }
}
fn offset(value: u64) -> Result<usize, wire::Error> {
    if value > i32::MAX as u64 {
        return Err(wire::Error::Operation);
    }
    usize::try_from(value).map_err(|_| wire::Error::Operation)
}
fn source_status(response: &mut Response, status: RoleSourceStatus) {
    let (state, value) = match status {
        RoleSourceStatus::Validating { packets } => (0, packets as u64),
        RoleSourceStatus::Ready { max_packet } => (1, max_packet as u64),
        RoleSourceStatus::Failed(error) => (2, u64::from(wire::runtime_error(error))),
    };
    put64(&mut response.0, 88, state);
    put64(&mut response.0, 96, value);
}
fn program_status(response: &mut Response, status: ProgramStatus) {
    let (state, a, b) = match status {
        ProgramStatus::Idle => (0, 0, 0),
        ProgramStatus::Receiving { received, total } => (1, received as u64, total as u64),
        ProgramStatus::Validating => (2, 0, 0),
        ProgramStatus::Ready => (3, 0, 0),
        ProgramStatus::Taken => (4, 0, 0),
        ProgramStatus::Failed(error) => (5, u64::from(wire::loop_error(error)), 0),
    };
    put64(&mut response.0, 88, state);
    put64(&mut response.0, 96, a);
    put64(&mut response.0, 104, b);
}
