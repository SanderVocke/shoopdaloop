use crate::assembly::{LoopControl, TestNode};
use crate::{export_wire::*, scenario::Mix, wire};
use rt_runtime::{
    executor::{Command, Retired},
    resource::{self, Cursor},
    schedule::Execution,
    transport::{Completion, Lane, Outcome, Ticket},
    Error, Frame, Sequence,
};
use shoop_loops::{
    media::ExportId,
    runtime::{Failure, LoopPlanner, Operation, Reply, Selection},
};

type Returned = Completion<Command<TestNode<Mix>, LoopPlanner>, Retired<TestNode<Mix>>>;
struct Lease {
    id: ExportId,
    cursors: [Option<Cursor>; 2],
}
struct Pending {
    request: Request,
    ticket: Option<Ticket>,
    admission: Option<Response>,
    terminal: Option<Response>,
    sent: bool,
    effective: u64,
}
impl Pending {
    fn response(&self, phase: u8, status: u32, observed: u64) -> Response {
        let mut response = Response::new(
            self.request,
            phase,
            status,
            self.ticket.as_ref().map_or(0, |t| t.sequence().0),
            observed,
        );
        put64(&mut response.0, 48, self.effective);
        response
    }
}
pub(crate) struct Exports {
    pub(crate) resource_bytes: u32,
    session: u64,
    last: u64,
    generations: [u32; BANKS],
    leases: [Option<Lease>; BANKS],
    pending: Option<Pending>,
    buffer: Option<Box<[u8]>>,
}
impl Exports {
    pub(crate) fn new(session: u64, resource_bytes: u32) -> Self {
        Self {
            resource_bytes,
            session,
            last: 0,
            generations: [1; BANKS],
            leases: std::array::from_fn(|_| None),
            pending: None,
            buffer: Some(vec![0; 64].into_boxed_slice()),
        }
    }
    pub(crate) fn retained(&self) -> usize {
        self.leases.iter().filter(|l| l.is_some()).count()
    }
    pub(crate) fn outstanding(&self) -> usize {
        usize::from(self.pending.is_some())
    }
    pub(crate) fn matches(&self, sequence: Sequence) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.ticket.as_ref().is_some_and(|t| t.sequence() == sequence))
    }
    pub(crate) fn ingest(
        &mut self,
        control: &mut LoopControl<Mix>,
        bytes: &[u8],
        now: u64,
    ) -> Result<(), wire::Error> {
        let request = Request::decode(bytes)?;
        if request.session != self.session {
            return Err(wire::Error::Session);
        }
        if request.sequence <= self.last {
            return Err(wire::Error::Sequence);
        }
        if self.pending.is_some() {
            return Err(wire::Error::Busy);
        }
        let bank = request.bank as usize;
        if self.generations[bank] != request.generation {
            return Err(wire::Error::Identity);
        }
        if (request.kind == EXPORT) == self.leases[bank].is_some() {
            return Err(wire::Error::State);
        }
        let mut pending = Pending {
            request,
            ticket: None,
            admission: None,
            terminal: None,
            sent: false,
            effective: request.at,
        };
        let submitted = if matches!(request.kind, AUDIO_READ | MIDI_READ) {
            let stream = usize::from(request.kind == MIDI_READ);
            let cursor =
                self.leases[bank].as_ref().unwrap().cursors[stream].ok_or(wire::Error::State)?;
            if cursor.position() as u64 != request.position {
                return Err(wire::Error::State);
            }
            let mut bytes = self.buffer.take().ok_or(wire::Error::State)?;
            bytes.fill(0);
            control.read(cursor, bytes)
        } else {
            let operation = match request.kind {
                EXPORT => Operation::Export {
                    target: request.target.unwrap(),
                    selection: match request.selection {
                        0 => Selection::Latest,
                        1 => Selection::Playback,
                        _ => Selection::Recording,
                    },
                },
                AUDIO_CURSOR | MIDI_CURSOR => Operation::Cursor {
                    export: self.leases[bank].as_ref().unwrap().id,
                    midi: request.kind == MIDI_CURSOR,
                },
                INITIAL => Operation::MidiInitial {
                    export: self.leases[bank].as_ref().unwrap().id,
                    channel: request.channel,
                },
                RELEASE => Operation::Release {
                    export: self.leases[bank].as_ref().unwrap().id,
                },
                _ => unreachable!(),
            };
            control.submit(Lane::Normal, Frame(request.at), operation)
        };
        match submitted {
            Ok(ticket) => {
                if request.cancelled {
                    ticket.cancel();
                }
                pending.ticket = Some(ticket);
            }
            Err(refusal) => {
                let read = matches!(request.kind, AUDIO_READ | MIDI_READ);
                let mut response = pending.response(if read { 3 } else { 1 }, REJECTED, now);
                put32(&mut response.0, 72, wire::runtime_error(refusal.error));
                if read {
                    match refusal.value {
                        Command::Resource(command) => match command.into_request() {
                            resource::Request::Read { bytes, .. } => self.buffer = Some(bytes),
                            _ => unreachable!(),
                        },
                        _ => unreachable!(),
                    }
                    pending.terminal = Some(response);
                } else {
                    pending.admission = Some(response);
                    drop(refusal.value);
                }
            }
        }
        self.last = request.sequence;
        self.pending = Some(pending);
        Ok(())
    }
    pub(crate) fn completion(&mut self, completion: Returned) -> Result<(), Returned> {
        let Some(pending) = self.pending.as_mut() else {
            return Err(completion);
        };
        if pending.ticket.as_ref().map(|t| t.sequence()) != Some(completion.sequence)
            || completion.retired.is_some()
        {
            return Err(completion);
        }
        let request = pending.request;
        if matches!(request.kind, AUDIO_READ | MIDI_READ) {
            let (cursor, bytes, reply) = match &completion.request {
                Command::Resource(command) => match command.request() {
                    resource::Request::Read { cursor, bytes }
                        if bytes.len() == 64 && self.buffer.is_none() =>
                    {
                        (*cursor, bytes, command.reply())
                    }
                    _ => return Err(completion),
                },
                _ => return Err(completion),
            };
            let mut response = pending.response(3, REJECTED, completion.frame.0);
            match completion.outcome {
                Outcome::Applied => {
                    let Some(resource::Reply::Read { written, eof }) = reply else {
                        return Err(completion);
                    };
                    if written > bytes.len()
                        || cursor.position() as u64
                            != request
                                .position
                                .checked_add(written as u64)
                                .unwrap_or(u64::MAX)
                    {
                        return Err(completion);
                    }
                    put32(&mut response.0, 12, APPLIED);
                    put32(&mut response.0, 8, if eof { 4 } else { 0 });
                    put32(&mut response.0, 96, written as u32);
                    put32(&mut response.0, 100, written as u32);
                    response.0[DATA..DATA + written].copy_from_slice(&bytes[..written]);
                }
                Outcome::Cancelled => put32(&mut response.0, 12, CANCELLED),
                Outcome::Rejected(error) => put32(&mut response.0, 72, wire::runtime_error(error)),
                _ => return Err(completion),
            }
            if pending.terminal.is_some() {
                return Err(completion);
            }
            let Command::Resource(command) = completion.request else {
                unreachable!()
            };
            let resource::Request::Read { cursor, bytes } = command.into_request() else {
                unreachable!()
            };
            self.buffer = Some(bytes);
            self.leases[request.bank as usize].as_mut().unwrap().cursors
                [usize::from(request.kind == MIDI_READ)] = Some(cursor);
            pending.terminal = Some(response);
        } else {
            if pending.admission.is_some()
                || pending.sent
                || !matches!(&completion.request, Command::Schedule { payload, .. } if payload.is_empty())
            {
                return Err(completion);
            }
            let mut response = pending.response(1, REJECTED, completion.frame.0);
            match completion.outcome {
                Outcome::Scheduled {
                    effective_frame, ..
                } => {
                    pending.effective = effective_frame.0;
                    put64(&mut response.0, 48, effective_frame.0);
                    put32(&mut response.0, 12, ADMITTED);
                }
                Outcome::Cancelled => put32(&mut response.0, 12, CANCELLED),
                Outcome::Rejected(error) => put32(&mut response.0, 72, wire::runtime_error(error)),
                _ => return Err(completion),
            }
            pending.admission = Some(response);
            drop(completion);
        }
        Ok(())
    }
    pub(crate) fn execution(
        &mut self,
        execution: Execution<Operation, Reply>,
    ) -> Result<(), Error> {
        let pending = self.pending.as_mut().ok_or(Error::InvalidState)?;
        if pending.terminal.is_some()
            || pending.ticket.as_ref().map(|t| t.sequence()) != Some(execution.sequence)
        {
            return Err(Error::InvalidState);
        }
        let request = pending.request;
        let bank = request.bank as usize;
        let mut response = pending.response(2, FAILED, execution.actual_frame.0);
        if let Some(frame) = execution.delivered_frame {
            put32(&mut response.0, 8, 1);
            put64(&mut response.0, 56, frame.0);
        }
        match execution.result {
            Err(error) => put32(&mut response.0, 72, wire::runtime_error(error)),
            Ok(()) => match execution.operation_result {
                Some(Reply::Failed(error)) => put32(
                    &mut response.0,
                    72,
                    match error {
                        Failure::Timeline(e) => wire::loop_error(e),
                        Failure::Runtime(e) => 512 + wire::runtime_error(e),
                    },
                ),
                Some(Reply::Export { id, cut, media }) if request.kind == EXPORT => {
                    if self.leases[bank].is_some() || media.requested_cut != cut.frame {
                        return Err(Error::InvalidState);
                    }
                    let b = &mut response.0;
                    let flags = get32(b, 8) | 2;
                    put32(b, 8, flags);
                    put64(b, 104, cut.frame.0);
                    put64(b, 112, cut.graph_epoch.0);
                    put64(b, 120, cut.topology_epoch.0);
                    let d = media.view.description;
                    put32(b, 128, d.owner.slot());
                    put32(b, 132, d.owner.generation());
                    put64(b, 136, d.revision.0);
                    put64(b, 144, d.frames);
                    put32(b, 152, d.channels);
                    b[156] = u8::from(d.midi)
                        | (u8::from(media.view.audio.is_some()) << 1)
                        | (u8::from(media.view.midi.is_some()) << 2);
                    b[158] = match cut.state.mode {
                        shoop_loops::Mode::Stopped => 0,
                        shoop_loops::Mode::Playing => 1,
                        shoop_loops::Mode::Recording => 2,
                    };
                    for (stream, source) in
                        [media.view.audio, media.view.midi].into_iter().enumerate()
                    {
                        if let Some(source) = source {
                            put64(
                                b,
                                160 + stream * 8,
                                source.snapshot.description.bytes as u64,
                            );
                            put64(b, 176 + stream * 8, source.snapshot.cut_frame.0);
                            put64(b, 224 + stream * 8, source.snapshot.revision.0);
                            if let Some(capture) = source.snapshot.capture {
                                put64(b, 192 + stream * 16, capture.start_frame.0);
                                put64(b, 200 + stream * 16, capture.next_frame.0);
                                b[157] |= u8::from(capture.active) << stream;
                                b[157] |= u8::from(capture.incomplete.is_some()) << (stream + 2);
                            }
                        }
                    }
                    put64(b, 240, cut.state.position);
                    put64(b, 248, cut.state.cycles);
                    self.leases[bank] = Some(Lease {
                        id,
                        cursors: [None; 2],
                    });
                    put32(b, 12, APPLIED);
                }
                Some(Reply::Cursor(cursor))
                    if matches!(request.kind, AUDIO_CURSOR | MIDI_CURSOR) =>
                {
                    self.leases[bank]
                        .as_mut()
                        .ok_or(Error::InvalidState)?
                        .cursors[usize::from(request.kind == MIDI_CURSOR)] = Some(cursor);
                    put32(&mut response.0, 12, APPLIED);
                }
                Some(Reply::MidiInitial {
                    channel,
                    state,
                    system_reset_seen,
                }) if request.kind == INITIAL && channel == request.channel => {
                    encode_initial(
                        &mut response.0[DATA..DATA + INITIAL_BYTES],
                        channel,
                        state,
                        system_reset_seen,
                    );
                    put32(&mut response.0, 100, INITIAL_BYTES as u32);
                    put32(&mut response.0, 12, APPLIED);
                }
                Some(Reply::Done) if request.kind == RELEASE => {
                    self.leases[bank] = None;
                    self.generations[bank] = self.generations[bank].checked_add(1).unwrap_or(0);
                    put32(&mut response.0, 84, self.generations[bank]);
                    put32(&mut response.0, 12, APPLIED);
                }
                _ => return Err(Error::InvalidState),
            },
        }
        pending.terminal = Some(response);
        Ok(())
    }
    pub(crate) fn next_response(&mut self) -> Result<Option<[u8; RESPONSE_BYTES]>, wire::Error> {
        let Some(pending) = self.pending.as_mut() else {
            return Ok(None);
        };
        if let Some(response) = pending.admission.take() {
            Response::decode(&response.0)?;
            if get32(&response.0, 12) == ADMITTED {
                pending.sent = true;
            } else {
                self.pending = None;
            }
            return Ok(Some(response.0));
        }
        if let Some(response) = pending
            .terminal
            .filter(|_| pending.sent || matches!(pending.request.kind, AUDIO_READ | MIDI_READ))
        {
            Response::decode(&response.0)?;
            self.pending = None;
            return Ok(Some(response.0));
        }
        Ok(None)
    }
}
fn encode_initial(
    bytes: &mut [u8],
    channel: u8,
    state: rt_runtime::midi_state::ChannelState,
    reset: bool,
) {
    bytes.fill(0);
    bytes[0] = channel;
    bytes[1] = u8::from(reset);
    bytes[2] = state.program().unwrap_or(255);
    let bank = state.program_bank();
    bytes[3] = bank[0].unwrap_or(255);
    bytes[4] = bank[1].unwrap_or(255);
    bytes[5] = state.pressure().unwrap_or(255);
    bytes[6] = u8::from(state.reset_controllers_seen());
    bytes[7] = match state.parameter_family() {
        None => 0,
        Some(rt_runtime::midi_state::ParameterFamily::Rpn) => 1,
        Some(rt_runtime::midi_state::ParameterFamily::Nrpn) => 2,
    };
    bytes[8] = u8::from(state.parameter_history_untracked());
    bytes[10..12].copy_from_slice(&state.pitch_bend().to_le_bytes());
    for key in 0..128u8 {
        let note = state.note(key).unwrap();
        let at = 16 + key as usize * 4;
        bytes[at] = note.velocity();
        bytes[at + 1] = u8::from(note.held()) | (u8::from(note.sostenuto()) << 1);
        bytes[at + 2] = note.pressure().unwrap_or(255);
        bytes[528 + key as usize] = state.controller(key).unwrap_or(255);
    }
}
