//! Read one confirmed Carla checkpoint lease at a time, outside the render thread.
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use anyhow::{anyhow, bail, ensure, Result};
use shoop_rt_protocol::{
    Capabilities, Outcome, ProcessorCheckpointCommand as Command, ProcessorCheckpointPage, Request,
    BULK_CHUNK_BYTES, MAX_PROCESSOR_CHECKPOINT_BYTES,
};
use shoop_rt_transport::carrier::Carrier;

use crate::{
    processor_controls::{processor_kind, track_kind, Owner, Target},
    requests::Purpose,
    EngineClock, RtBackend,
};

pub(crate) enum Reply {
    Outcome(Outcome),
    Page(ProcessorCheckpointPage),
}

impl Owner {
    fn checkpoint_command(self, command: Command) -> Request {
        Request::ProcessorCheckpoint {
            target: self.target,
            slot: self.slot,
            worker_generation: self.generation,
            command,
        }
    }
}

#[derive(Default)]
pub(crate) struct Capture {
    cut: Option<u64>,
    reading: Option<Read>,
    ready: BTreeMap<Target, (Owner, String)>,
    failure: Option<String>,
}

impl Capture {
    pub(crate) fn cancel(&mut self) {
        self.cut = None;
        self.ready.clear();
        self.failure = None;
        if let Some(read) = &mut self.reading {
            read.discard = true;
        }
    }
}

struct Read {
    target: Target,
    owner: Owner,
    snapshot: u64,
    total: u32,
    limit: u64,
    bytes: Vec<u8>,
    discard: bool,
}

impl Read {
    fn accept(&mut self, command: Command, reply: Reply) -> Result<Option<Command>> {
        match (command, reply) {
            (Command::Open, Reply::Outcome(Outcome::ProcessorCheckpoint { snapshot, bytes })) => {
                // Keep the lease ID even if the host's reply exceeds our budget so it can be closed.
                self.snapshot = snapshot;
                self.total = bytes;
                ensure!(
                    snapshot != 0 && bytes != 0 && bytes as usize <= MAX_PROCESSOR_CHECKPOINT_BYTES,
                    "invalid Carla checkpoint lease"
                );
                ensure!(
                    u64::from(bytes) <= self.limit,
                    "Carla checkpoints exceed the remaining host media budget"
                );
                if !self.discard {
                    self.bytes.try_reserve_exact(bytes as usize)?;
                }
            }
            (Command::Read { snapshot, offset }, Reply::Page(page)) => {
                let left = self.total as usize - self.bytes.len();
                ensure!(
                    page.snapshot == snapshot
                        && page.offset == offset
                        && page.total == self.total
                        && offset as usize == self.bytes.len()
                        && left != 0
                        && usize::from(page.len) == left.min(BULK_CHUNK_BYTES),
                    "invalid Carla checkpoint page"
                );
                if !self.discard {
                    self.bytes
                        .extend_from_slice(&page.bytes[..usize::from(page.len)]);
                }
            }
            (Command::Close { .. }, Reply::Outcome(Outcome::Done)) => return Ok(None),
            (_, Reply::Outcome(Outcome::Refused(reason))) => {
                bail!(
                    "Carla checkpoint: {}",
                    crate::incoming::refusal_text(reason)
                )
            }
            _ => bail!("unexpected Carla checkpoint response"),
        }
        Ok(Some(
            if self.discard || self.bytes.len() == self.total as usize {
                Command::Close {
                    snapshot: self.snapshot,
                }
            } else {
                Command::Read {
                    snapshot: self.snapshot,
                    offset: self.bytes.len() as u32,
                }
            },
        ))
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn capture_processor_checkpoints(
        &mut self,
        limit: u64,
    ) -> Result<Option<BTreeMap<Target, String>>> {
        if self.processor_checkpoints.cut != Some(self.snapshots.capture_after) {
            self.processor_checkpoints.cancel();
            self.processor_checkpoints.cut = Some(self.snapshots.capture_after);
        }
        if self.processor_checkpoints.reading.is_some() {
            return Ok(None);
        }
        if let Some(error) = self.processor_checkpoints.failure.take() {
            self.processor_checkpoints.cancel();
            bail!(error);
        }
        let tracks = self
            .mirror
            .tracks
            .iter()
            .filter(|(_, track)| {
                track.confirmed
                    && !track.removing
                    && track_kind(track).is_some_and(|kind| kind.is_carla())
            })
            .map(|(id, _)| Target::Track(*id));
        let buses = self
            .mirror
            .buses
            .iter()
            .filter(|(_, bus)| {
                !bus.removing
                    && bus
                        .processor_type
                        .as_deref()
                        .and_then(processor_kind)
                        .is_some_and(|kind| kind.is_carla())
            })
            .map(|(id, _)| Target::Bus(*id));
        let mut remaining = limit;
        let mut missing = None;
        let mut count = 0;
        for target in tracks.chain(buses) {
            count += 1;
            ensure!(
                self.capabilities
                    .contains(Capabilities::PROCESSOR_CHECKPOINTS),
                "Carla checkpoints are unavailable on this host"
            );
            let owner = self
                .hosted_owner(target)
                .ok_or_else(|| anyhow!("Carla processor owner is not confirmed"))?;
            match self.processor_checkpoints.ready.get(&target) {
                Some((confirmed, state)) => {
                    ensure!(
                        *confirmed == owner,
                        "Carla processor owner changed during capture"
                    );
                    remaining = remaining.checked_sub(state.len() as u64).ok_or_else(|| {
                        anyhow!("Carla checkpoints exceed the remaining host media budget")
                    })?;
                }
                None => {
                    missing.get_or_insert((target, owner));
                }
            }
        }
        if let Some((target, owner)) = missing {
            self.require_mutation_capacity(1)?;
            self.processor_checkpoints.reading = Some(Read {
                target,
                owner,
                snapshot: 0,
                total: 0,
                limit: remaining,
                bytes: Vec::new(),
                discard: false,
            });
            self.requests.push_internal(
                owner.checkpoint_command(Command::Open),
                Purpose::ProcessorCheckpoint,
            );
            return Ok(None);
        }
        ensure!(
            self.processor_checkpoints.ready.len() == count,
            "Carla processor set changed during capture"
        );
        Ok(Some(
            core::mem::take(&mut self.processor_checkpoints.ready)
                .into_iter()
                .map(|(target, (_, state))| (target, state))
                .collect(),
        ))
    }

    pub(crate) fn processor_checkpoint_answered(&mut self, request: Request, reply: Reply) {
        let Request::ProcessorCheckpoint { command, .. } = request else {
            return;
        };
        let Some(mut read) = self.processor_checkpoints.reading.take() else {
            return;
        };
        if read.owner.checkpoint_command(command) != request {
            self.processor_checkpoints.reading = Some(read);
            return;
        }
        if !read.discard && self.hosted_owner(read.target) != Some(read.owner) {
            self.processor_checkpoints.failure =
                Some("Carla processor owner changed during its checkpoint read".into());
            read.discard = true;
        }
        match read.accept(command, reply) {
            Ok(Some(next)) => {
                self.requests.push_internal(
                    read.owner.checkpoint_command(next),
                    Purpose::ProcessorCheckpoint,
                );
                self.processor_checkpoints.reading = Some(read);
            }
            Ok(None) if !read.discard => match String::from_utf8(read.bytes) {
                Ok(state) => {
                    self.processor_checkpoints
                        .ready
                        .insert(read.target, (read.owner, state));
                }
                Err(error) => self.processor_checkpoints.failure = Some(error.to_string()),
            },
            Ok(None) => {}
            Err(error) => {
                if !read.discard {
                    self.processor_checkpoints.failure = Some(error.to_string());
                }
                if read.snapshot != 0 && !matches!(command, Command::Close { .. }) {
                    read.discard = true;
                    self.requests.push_internal(
                        read.owner.checkpoint_command(Command::Close {
                            snapshot: read.snapshot,
                        }),
                        Purpose::ProcessorCheckpoint,
                    );
                    self.processor_checkpoints.reading = Some(read);
                }
            }
        }
    }
}
