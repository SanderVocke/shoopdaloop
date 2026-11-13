//! Request ownership and installation above the transition execution boundary.
use crate::{
    adapter::Device,
    engine::EngineDevice,
    image::{Image, TransitionImage, Value},
    looping::{LoopDevice, MediaCommand, MediaResult, TpReply},
    protocol::{get, put},
};
use rt_runtime::Error;
use serde::{Deserialize, Serialize};
mod membership;
#[cfg(test)]
mod tests;
pub mod wire;
use membership::{Member, Membership};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    pub opcode: u8,
    pub index: u32,
    pub words: [u32; 20],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Transfer(Transfer),
    Reply(TpReply),
}
#[derive(Serialize)]
pub struct StaticProgram {
    pub setup: Vec<Transfer>,
    pub actions: Vec<Transfer>,
}
struct Slot {
    generation: u32,
    request: Option<[u32; 20]>,
    terminal: bool,
}
struct Transaction {
    operation: u8,
    slot: usize,
    original: [u32; 20],
    at: usize,
    stage: Option<u64>,
}
pub struct TransitionService {
    image: TransitionImage,
    slots: Vec<Slot>,
    outstanding: Option<Transaction>,
    program: Vec<Transfer>,
    membership: Membership,
    stage_counter: u64,
    published: u64,
    faulted: bool,
    settled: bool,
}
impl TransitionService {
    pub fn metadata_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.slots.capacity() * std::mem::size_of::<Slot>()
            + self.program.capacity() * std::mem::size_of::<Transfer>()
            + self.membership.metadata_bytes()
            + self.image.description.loops.capacity()
                * std::mem::size_of::<crate::image::LoopSpec>()
            + self
                .image
                .description
                .loops
                .iter()
                .map(|spec| spec.kind.capacity())
                .sum::<usize>()
            + self.image.dependency_order.capacity() * std::mem::size_of::<crate::image::Identity>()
    }
    pub fn new(image: &TransitionImage) -> Result<Self, Error> {
        if !matches!(image.description.quantum, 1 | 16 | 32 | 64 | 128)
            || image.limits.loop_slots == 0
            || image.limits.loop_slots > 65536
            || image.limits.pending_requests == 0
            || image.limits.pending_requests > 65536
            || image
                .description
                .loops
                .iter()
                .any(|spec| spec.id.slot >= image.limits.loop_slots || spec.id.generation == 0)
        {
            return Err(Error::InvalidParameter);
        }
        Ok(Self {
            image: image.clone(),
            slots: (0..image.limits.pending_requests)
                .map(|_| Slot {
                    generation: 1,
                    request: None,
                    terminal: false,
                })
                .collect(),
            outstanding: None,
            program: Vec::with_capacity(
                image.limits.pending_requests as usize + image.limits.loop_slots as usize + 3,
            ),
            membership: Membership::new(image.limits.loop_slots, image.limits.pending_requests),
            stage_counter: 0,
            published: 0,
            faulted: false,
            settled: false,
        })
    }
    fn select(&self, words: &[u32; 20]) -> Result<usize, u8> {
        let target = get(words, 64, 64);
        let spec = self
            .image
            .description
            .loops
            .iter()
            .find(|spec| u64::from(spec.id.slot) | (u64::from(spec.id.generation) << 32) == target)
            .ok_or(3u8)?;
        let operation = get(words, 34, 2);
        if get(words, 44, 1) == 0
            && matches!(operation, 1 | 3)
            && get(words, 384, 64) & u64::from(self.image.description.quantum - 1) != 0
        {
            return Err(6);
        }
        if get(words, 33, 1) != 0 {
            let source = get(words, 256, 64);
            if !self.image.description.loops.iter().any(|spec| {
                u64::from(spec.id.slot) | (u64::from(spec.id.generation) << 32) == source
            }) {
                return Err(3);
            }
            if spec
                .sync_source
                .map(|id| u64::from(id.slot) | (u64::from(id.generation) << 32))
                != Some(source)
            {
                return Err(7);
            }
        }
        let sequence = get(words, 128, 64);
        if self
            .slots
            .iter()
            .filter_map(|slot| slot.request.as_ref())
            .any(|r| get(r, 128, 64) == sequence)
        {
            return Err(4);
        }
        self.slots
            .iter()
            .position(|slot| slot.generation != 0 && slot.request.is_none())
            .ok_or(2)
    }
    fn publication(&mut self, reserved: Option<(usize, [u32; 20])>) -> Result<(), u8> {
        let slots = &self.slots;
        self.membership
            .rebuild(slots.iter().enumerate().filter_map(|(i, slot)| {
                if let Some((selected, words)) = reserved {
                    if selected == i {
                        return Some(Member::new(i as u32, slot.generation, &words));
                    }
                }
                slot.request
                    .as_ref()
                    .filter(|_| !slot.terminal)
                    .map(|r| Member::new(i as u32, slot.generation, r))
            }))?;
        let mut begin = [0; 20];
        put(&mut begin, 0, 64, self.published);
        begin[2] = self.membership.len() as u32;
        self.program.push(Transfer {
            opcode: 18,
            index: 0,
            words: begin,
        });
        for (index, node) in self
            .membership
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.generation != 0)
        {
            let mut words = [0; 20];
            words[2] = node.generation;
            words[3] = node.value;
            self.program.push(Transfer {
                opcode: 19,
                index: index as u32,
                words,
            });
        }
        for (index, head) in self.membership.heads.iter().enumerate() {
            let mut words = [0; 20];
            words[2] = head[0];
            words[4] = head[1];
            self.program.push(Transfer {
                opcode: 20,
                index: index as u32,
                words,
            });
        }
        self.program.push(Transfer {
            opcode: 21,
            index: 0,
            words: [0; 20],
        });
        Ok(())
    }
    pub fn begin(&mut self, opcode: u8, index: u32, words: [u32; 20]) -> Result<Transfer, u8> {
        if ((self.faulted || self.settled) && opcode != 13) || self.outstanding.is_some() {
            return Err(5);
        }
        self.program.clear();
        let slot = match opcode {
            4 => {
                let slot = self.select(&words)?;
                self.publication(Some((slot, words)))?;
                let installed = wire::install(&words, self.slots[slot].generation);
                self.program.push(Transfer {
                    opcode,
                    index: slot as u32,
                    words: installed,
                });
                slot
            }
            5 | 6 => {
                let slot = self.slots.get(index as usize).ok_or(3u8)?;
                if slot.request.is_none() || slot.generation != words[0] {
                    return Err(3);
                }
                self.program.push(Transfer {
                    opcode,
                    index,
                    words,
                });
                index as usize
            }
            13 => {
                self.slots.get(index as usize).ok_or(3u8)?;
                self.program.push(Transfer {
                    opcode,
                    index,
                    words,
                });
                index as usize
            }
            22 => {
                self.publication(None)?;
                0
            }
            _ => return Err(10),
        };
        self.outstanding = Some(Transaction {
            operation: opcode,
            slot,
            original: words,
            at: 0,
            stage: None,
        });
        Ok(self.program[0])
    }
    pub fn finish(&mut self, mut reply: TpReply) -> Result<Progress, Error> {
        let Some(mut tx) = self.outstanding.take() else {
            return self.disconnected();
        };
        let transfer = self.program[tx.at];
        if reply.error > 10 || (reply.error != 0 && reply.words != [0; 26]) {
            return self.disconnected();
        }
        if reply.error != 0 {
            return Ok(Progress::Reply(reply));
        }
        match transfer.opcode {
            18 => {
                let nonce = get(&reply.words, 0, 64);
                if nonce <= self.stage_counter || reply.words[2..].iter().any(|&v| v != 0) {
                    return self.disconnected();
                }
                self.stage_counter = nonce;
                tx.stage = Some(nonce);
                for step in &mut self.program[tx.at + 1..] {
                    if matches!(step.opcode, 19..=21) {
                        put(&mut step.words, 0, 64, nonce);
                    } else if step.opcode == 4 {
                        put(&mut step.words, wire::PUBLICATION, 64, nonce);
                    }
                }
            }
            19 | 20 => {
                if reply.words != [0; 26] {
                    return self.disconnected();
                }
            }
            21 => {
                if Some(get(&reply.words, 0, 64)) != tx.stage
                    || reply.words[2..].iter().any(|&v| v != 0)
                {
                    return self.disconnected();
                }
                self.published = tx.stage.expect("acknowledged publication");
                if tx.operation == 22 {
                    reply.words = [0; 26];
                }
            }
            4 => {
                let slot = &mut self.slots[tx.slot];
                if reply.words[0] != tx.slot as u32
                    || reply.words[1] != slot.generation
                    || reply.words[2..].iter().any(|&v| v != 0)
                {
                    return self.disconnected();
                }
                slot.request = Some(tx.original);
                slot.terminal = false;
            }
            5 => {
                if reply.words[0] > 1 || reply.words[1..].iter().any(|&v| v != 0) {
                    return self.disconnected();
                }
                self.slots[tx.slot].terminal = true;
            }
            6 | 13 => {
                let slot = &self.slots[tx.slot];
                if transfer.opcode == 6 && reply.words == [0; 26] {
                    if slot.terminal {
                        return self.disconnected();
                    }
                } else {
                    let Ok(words) =
                        wire::result(&reply.words, slot.request.as_ref(), slot.generation)
                    else {
                        return self.disconnected();
                    };
                    let outcome = get(&words, 37, 3);
                    if (transfer.opcode == 6 || slot.terminal) && outcome == 0 {
                        return self.disconnected();
                    }
                    reply.words = words;
                    let slot = &mut self.slots[tx.slot];
                    if transfer.opcode == 6 {
                        slot.request = None;
                        slot.terminal = false;
                        slot.generation = slot.generation.checked_add(1).unwrap_or(0);
                    } else {
                        slot.terminal |= outcome != 0;
                    }
                }
            }
            _ => return self.disconnected(),
        }
        tx.at += 1;
        if tx.at < self.program.len() {
            let next = self.program[tx.at];
            self.outstanding = Some(tx);
            Ok(Progress::Transfer(next))
        } else {
            Ok(Progress::Reply(reply))
        }
    }
    pub fn disconnected<T>(&mut self) -> Result<T, Error> {
        self.faulted = true;
        self.outstanding = None;
        Err(Error::Disconnected)
    }
    pub fn exchange(
        &mut self,
        opcode: u8,
        index: u32,
        words: [u32; 20],
        mut exchange: impl FnMut(Transfer) -> Result<TpReply, Error>,
    ) -> Result<TpReply, Error> {
        let mut transfer = match self.begin(opcode, index, words) {
            Ok(t) => t,
            Err(error) => {
                return Ok(TpReply {
                    error,
                    words: [0; 26],
                });
            }
        };
        loop {
            let reply = match exchange(transfer) {
                Ok(r) => r,
                Err(_) => return self.disconnected(),
            };
            match self.finish(reply)? {
                Progress::Transfer(t) => transfer = t,
                Progress::Reply(r) => return Ok(r),
            }
        }
    }
    pub fn observe(&mut self, opcode: u8, reply: &TpReply) {
        if opcode == 0 && reply.error == 0 {
            self.settled = get(&reply.words, 128, 2) == 2;
            self.faulted |= get(&reply.words, 128, 2) == 3;
        } else if opcode == 16 && reply.error == 0 {
            self.settled = true;
        } else if matches!(opcode, 9 | 11) && reply.error == 0 {
            self.settled = false;
        } else if opcode == 9 && reply.error != 5 {
            self.faulted = true;
        }
    }
    pub fn prepare_static_batch(&mut self, requests: &[[u32; 20]]) -> Result<StaticProgram, u8> {
        if self.faulted
            || self.settled
            || self.outstanding.is_some()
            || self.stage_counter != 0
            || self.slots.iter().any(|s| s.request.is_some())
        {
            return Err(5);
        }
        let mut identities = Vec::with_capacity(requests.len().min(self.slots.len()));
        for &original in requests {
            let slot = match self.select(&original) {
                Ok(i) => i,
                Err(e) => {
                    for slot in &mut self.slots {
                        slot.request = None;
                    }
                    return Err(e);
                }
            };
            self.slots[slot].request = Some(original);
            identities.push(slot);
        }
        self.program.clear();
        self.publication(None)?;
        for step in &mut self.program {
            if matches!(step.opcode, 19..=21) {
                put(&mut step.words, 0, 64, 1);
            }
        }
        let setup = self.program.clone();
        let actions = identities
            .into_iter()
            .zip(requests)
            .map(|(slot, original)| {
                let mut words = wire::install(original, self.slots[slot].generation);
                put(&mut words, wire::PUBLICATION, 64, 1);
                Transfer {
                    opcode: 4,
                    index: slot as u32,
                    words,
                }
            })
            .collect();
        self.stage_counter = 1;
        self.published = 1;
        Ok(StaticProgram { setup, actions })
    }
}

pub struct ManagedDevice<D> {
    inner: D,
    service: Option<TransitionService>,
}

impl<D> ManagedDevice<D> {
    pub fn new(inner: D) -> Self {
        Self {
            inner,
            service: None,
        }
    }
    pub fn inner(&self) -> &D {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut D {
        &mut self.inner
    }
}

impl<D: Device> Device for ManagedDevice<D> {
    fn load(&mut self, image: &Image) -> Result<(), Error> {
        let service = TransitionService::new(&image.transition)?;
        if let Err(error) = self.inner.load(image) {
            if let Some(service) = &mut self.service {
                let _ = service.disconnected::<()>();
            }
            return Err(error);
        }
        self.service = Some(service);
        Ok(())
    }
    fn write_state(&mut self, address: u16, value: Value) -> Result<(), Error> {
        self.inner.write_state(address, value)
    }
    fn process(&mut self, image: &Image, inputs: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        let result = self.inner.process(image, inputs);
        if let Some(service) = &mut self.service {
            service.settled = false;
            service.faulted |= result.is_err();
        }
        result
    }
}

impl<D: LoopDevice> LoopDevice for ManagedDevice<D> {
    fn tp(&mut self, opcode: u8, index: u32, words: [u32; 20]) -> Result<TpReply, Error> {
        if matches!(opcode, 4..=6 | 13 | 22) {
            let Some(service) = &mut self.service else {
                return Ok(TpReply {
                    error: 5,
                    words: [0; 26],
                });
            };
            service.exchange(opcode, index, words, |transfer| {
                self.inner
                    .tp(transfer.opcode, transfer.index, transfer.words)
            })
        } else {
            let reply = self.inner.tp(opcode, index, words)?;
            if let Some(service) = &mut self.service {
                service.observe(opcode, &reply);
            }
            Ok(reply)
        }
    }
    fn memory_report(&mut self) -> Result<crate::memory::MemoryReport, Error> {
        self.inner.memory_report()
    }
    fn media(&mut self, command: MediaCommand) -> Result<MediaResult, Error> {
        self.inner.media(command)
    }
    fn chunk_limit(&mut self, limit: u32) -> Result<(), Error> {
        self.inner.chunk_limit(limit)
    }
}

impl<D: EngineDevice> EngineDevice for ManagedDevice<D> {
    fn program(&mut self, image: &Image) -> Result<(), Error> {
        self.inner.program(image)
    }
    fn read_state(&mut self, address: u16) -> Result<Value, Error> {
        self.inner.read_state(address)
    }
    fn scratch(&mut self, buffer: u16) -> Result<Vec<i32>, Error> {
        self.inner.scratch(buffer)
    }
}
