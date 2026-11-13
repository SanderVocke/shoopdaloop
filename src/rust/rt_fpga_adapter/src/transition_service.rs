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
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    pub opcode: u8,
    pub index: u32,
    pub words: [u32; 20],
}

struct Slot {
    generation: u32,
    request: Option<[u32; 20]>,
}

pub struct TransitionService {
    image: TransitionImage,
    slots: Vec<Slot>,
    outstanding: Option<(Transfer, [u32; 20])>,
    faulted: bool,
    settled: bool,
}

impl TransitionService {
    pub fn metadata_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.slots.capacity() * std::mem::size_of::<Slot>()
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
                })
                .collect(),
            outstanding: None,
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
            .any(|request| get(request, 128, 64) == sequence)
        {
            return Err(4);
        }
        self.slots
            .iter()
            .position(|slot| slot.generation != 0 && slot.request.is_none())
            .ok_or(2)
    }

    pub fn begin(&mut self, opcode: u8, index: u32, words: [u32; 20]) -> Result<Transfer, u8> {
        if self.faulted || self.settled || self.outstanding.is_some() {
            return Err(5);
        }
        let transfer = match opcode {
            4 => {
                let slot = self.select(&words)?;
                let mut installed = words;
                put(
                    &mut installed,
                    1,
                    32,
                    u64::from(self.slots[slot].generation),
                );
                Transfer {
                    opcode,
                    index: slot as u32,
                    words: installed,
                }
            }
            5 | 6 => {
                let slot = self.slots.get(index as usize).ok_or(3u8)?;
                if slot.request.is_none() || slot.generation != words[0] {
                    return Err(3);
                }
                Transfer {
                    opcode,
                    index,
                    words,
                }
            }
            _ => return Err(10),
        };
        self.outstanding = Some((transfer, words));
        Ok(transfer)
    }

    pub fn finish(&mut self, reply: TpReply) -> Result<TpReply, Error> {
        let Some((transfer, original)) = self.outstanding.take() else {
            return self.disconnected();
        };
        if reply.error > 10 || (reply.error != 0 && reply.words != [0; 26]) {
            return self.disconnected();
        }
        if reply.error != 0 {
            return Ok(reply);
        }
        let slot = &mut self.slots[transfer.index as usize];
        match transfer.opcode {
            4 => {
                if reply.words[0] != transfer.index || reply.words[1] != slot.generation {
                    return self.disconnected();
                }
                slot.request = Some(original);
            }
            5 => {
                if reply.words[0] > 1 || reply.words[1..].iter().any(|&word| word != 0) {
                    return self.disconnected();
                }
            }
            6 if reply.words != [0; 26] => {
                if get(&reply.words, 0, 1) != 1
                    || get(&reply.words, 1, 32) != u64::from(slot.generation)
                    || !(1..=4).contains(&get(&reply.words, 37, 3))
                    || get(&reply.words, 128, 64)
                        != get(slot.request.as_ref().expect("owned request"), 128, 64)
                {
                    return self.disconnected();
                }
                slot.request = None;
                slot.generation = slot.generation.checked_add(1).unwrap_or(0);
            }
            _ => {}
        }
        Ok(reply)
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
        let transfer = match self.begin(opcode, index, words) {
            Ok(transfer) => transfer,
            Err(error) => {
                return Ok(TpReply {
                    error,
                    words: [0; 26],
                });
            }
        };
        match exchange(transfer) {
            Ok(reply) => self.finish(reply),
            Err(_) => self.disconnected(),
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

    pub fn prepare_static(&mut self, words: [u32; 20]) -> Result<Transfer, u8> {
        if self.faulted || self.settled || self.outstanding.is_some() {
            return Err(5);
        }
        let slot = self.select(&words)?;
        self.slots[slot].request = Some(words);
        let mut installed = words;
        put(
            &mut installed,
            1,
            32,
            u64::from(self.slots[slot].generation),
        );
        Ok(Transfer {
            opcode: 4,
            index: slot as u32,
            words: installed,
        })
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
        if matches!(opcode, 4..=6) {
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
