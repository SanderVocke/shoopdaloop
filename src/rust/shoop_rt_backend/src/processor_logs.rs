use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use anyhow::{anyhow, bail, ensure, Result};
use shoop_app_api::FxGenerationLogState;
use shoop_backend::{BackendMutationKind, BackendSnapshot};
use shoop_rt_protocol::{
    Outcome, ProcessorLogCommand as Command, ProcessorLogPage, Request, PROCESSOR_LOG_GENERATIONS,
    PROCESSOR_LOG_STREAM_BYTES,
};

pub(crate) use crate::processor_controls::Target;
use crate::{processor_controls::Owner, requests::Purpose, EngineClock, RtBackend};

pub(crate) enum Reply {
    Outcome(Outcome),
    Page(ProcessorLogPage),
}

impl Owner {
    fn log_command(self, command: Command) -> Request {
        Request::ProcessorLogs {
            target: self.target,
            slot: self.slot,
            worker_generation: self.generation,
            command,
        }
    }
}

#[derive(Default)]
pub(crate) struct Logs {
    reading: Option<Read>,
    visible: Option<(Target, Owner, Arc<[FxGenerationLogState]>)>,
}

impl Logs {
    pub fn clear_confirmed(&mut self, target: Target) {
        if self.visible.as_ref().is_some_and(|entry| entry.0 == target) {
            self.visible = None;
        }
        if let Some(read) = self.reading.as_mut().filter(|read| read.target == target) {
            read.discard = true;
        }
    }
}

struct Read {
    target: Target,
    owner: Owner,
    snapshot: u64,
    generations: u8,
    index: u8,
    stderr: bool,
    generation: u64,
    dropped: [u64; 2],
    totals: [Option<u32>; 2],
    streams: [Vec<u8>; 2],
    logs: Vec<FxGenerationLogState>,
    discard: bool,
}

impl Read {
    fn new(target: Target, owner: Owner) -> Self {
        Self {
            target,
            owner,
            snapshot: 0,
            generations: 0,
            index: 0,
            stderr: false,
            generation: 0,
            dropped: [0; 2],
            totals: [None; 2],
            streams: Default::default(),
            logs: Vec::new(),
            discard: false,
        }
    }

    fn next(&self) -> Command {
        if self.discard || self.index >= self.generations {
            Command::Close {
                snapshot: self.snapshot,
            }
        } else {
            Command::Read {
                snapshot: self.snapshot,
                generation_index: self.index,
                stderr: self.stderr,
                offset: self.streams[usize::from(self.stderr)].len() as u32,
            }
        }
    }

    fn accept(&mut self, command: Command, reply: Reply) -> Result<Option<Command>> {
        match (command, reply) {
            (
                Command::Open,
                Reply::Outcome(Outcome::ProcessorLogs {
                    snapshot,
                    generations,
                }),
            ) => {
                ensure!(
                    snapshot != 0 && usize::from(generations) <= PROCESSOR_LOG_GENERATIONS,
                    "invalid Carla log snapshot"
                );
                self.snapshot = snapshot;
                self.generations = generations;
            }
            (Command::Read { .. }, Reply::Page(page)) => {
                let stream = usize::from(self.stderr);
                let bytes = page
                    .bytes
                    .get(..usize::from(page.len))
                    .ok_or_else(|| anyhow!("invalid Carla log page length"))?;
                ensure!(
                    page.generation != 0
                        && page.generation <= self.owner.generation
                        && (self.generation == 0 || page.generation == self.generation)
                        && page.offset as usize == self.streams[stream].len()
                        && page.total as usize <= PROCESSOR_LOG_STREAM_BYTES
                        && (page.offset as usize + bytes.len()) <= page.total as usize
                        && (!bytes.is_empty() || page.offset == page.total)
                        && self.totals[stream].is_none_or(
                            |total| total == page.total && self.dropped[stream] == page.dropped
                        ),
                    "invalid Carla log page"
                );
                self.generation = page.generation;
                self.dropped[stream] = page.dropped;
                self.totals[stream] = Some(page.total);
                self.streams[stream].extend_from_slice(bytes);
                if self.streams[stream].len() == page.total as usize {
                    if self.stderr {
                        let [stdout, stderr] = core::mem::take(&mut self.streams);
                        self.logs.push(FxGenerationLogState {
                            generation: self.generation,
                            stdout: Arc::from(String::from_utf8_lossy(&stdout).into_owned()),
                            stderr: Arc::from(String::from_utf8_lossy(&stderr).into_owned()),
                            dropped_stdout_bytes: self.dropped[0],
                            dropped_stderr_bytes: self.dropped[1],
                        });
                        self.index += 1;
                        self.generation = 0;
                        self.totals = [None; 2];
                    }
                    self.stderr = !self.stderr;
                }
            }
            (Command::Close { .. }, Reply::Outcome(Outcome::Done)) => return Ok(None),
            (_, Reply::Outcome(Outcome::Refused(reason))) => {
                bail!("{}", crate::incoming::refusal_text(reason))
            }
            _ => bail!("unexpected Carla log response"),
        }
        Ok(Some(self.next()))
    }
}

impl<C: shoop_rt_transport::carrier::Carrier, T: EngineClock> RtBackend<C, T> {
    fn log_owner(&self, target: Target) -> Option<Owner> {
        self.hosted_owner(target)
            .filter(|owner| owner.generation != 0)
    }

    pub(crate) fn request_processor_logs(&mut self, target: Target) -> Result<()> {
        self.ready()?;
        ensure!(
            self.capabilities
                .contains(shoop_rt_protocol::Capabilities::PROCESSOR_LOGS),
            "Carla process logs are unavailable on this host"
        );
        let owner = self
            .log_owner(target)
            .ok_or_else(|| anyhow!("Carla worker generation is not confirmed"))?;
        if let Some(read) = &self.processor_logs.reading {
            ensure!(
                read.target == target && read.owner == owner,
                "another processor's logs are being read"
            );
            return Ok(());
        }
        self.require_mutation_capacity(1)?;
        self.processor_logs.reading = Some(Read::new(target, owner));
        self.requests
            .push_internal(owner.log_command(Command::Open), Purpose::ProcessorLogs);
        Ok(())
    }

    pub(crate) fn processor_logs_answered(&mut self, request: Request, reply: Reply) {
        let Request::ProcessorLogs { command, .. } = request else {
            return;
        };
        if !self.processor_logs.reading.as_ref().is_some_and(|read| {
            request == read.owner.log_command(command)
                && (read.snapshot == 0) == matches!(command, Command::Open)
        }) {
            return;
        }
        let Some(mut read) = self.processor_logs.reading.take() else {
            return;
        };
        read.discard |= self.log_owner(read.target) != Some(read.owner);
        match read.accept(command, reply) {
            Ok(Some(next)) => {
                self.requests
                    .push_internal(read.owner.log_command(next), Purpose::ProcessorLogs);
                self.processor_logs.reading = Some(read);
            }
            Ok(None) => {
                if !read.discard {
                    self.processor_logs.visible = Some((read.target, read.owner, read.logs.into()));
                }
            }
            Err(error) => {
                if read.snapshot != 0 && !matches!(command, Command::Close { .. }) {
                    self.requests.push_internal(
                        read.owner.log_command(Command::Close {
                            snapshot: read.snapshot,
                        }),
                        Purpose::ProcessorLogs,
                    );
                }
                if !read.discard {
                    let (kind, id) = match read.target {
                        Target::Track(id) => (BackendMutationKind::TrackFxControl, id.raw()),
                        Target::Bus(id) => (BackendMutationKind::BusFxControl, id.raw()),
                    };
                    self.failure(kind, Some(id), None, error.to_string());
                }
            }
        }
    }

    pub(crate) fn display_processor_logs(&self, state: &mut BackendSnapshot) {
        let Some((target, owner, logs)) = &self.processor_logs.visible else {
            return;
        };
        if self.log_owner(*target) != Some(*owner) {
            return;
        }
        let fx = match target {
            Target::Track(id) => state.tracks.get_mut(id).and_then(|track| track.fx.as_mut()),
            Target::Bus(id) => state
                .mixer
                .buses
                .get_mut(id)
                .and_then(|bus| bus.fx.as_mut()),
        };
        if let Some(fx) = fx {
            fx.logs = Arc::clone(logs);
        }
    }
}
