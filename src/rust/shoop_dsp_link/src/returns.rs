use crate::{validate_block, Block, Buffer, Descriptor, Error, Header};
use alloc::vec::Vec;

struct Slot {
    frame: Option<u64>,
    block: Block,
}
pub struct Returns {
    descriptor: Descriptor,
    next: u64,
    slots: Vec<Slot>,
}
impl Returns {
    pub fn prepare(descriptor: Descriptor) -> Result<Self, Error> {
        let d = descriptor.validate()?;
        let slots = (0..d.return_slots()?)
            .map(|_| {
                Ok(Slot {
                    frame: None,
                    block: Block::prepare(d)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Self {
            descriptor: d,
            next: d.start_frame,
            slots,
        })
    }
    fn index(&self, frame: u64) -> usize {
        (((frame - self.descriptor.start_frame) / u64::from(self.descriptor.quantum))
            % self.slots.len() as u64) as usize
    }
    pub fn accept(&mut self, block: &impl Buffer) -> Result<(), Error> {
        let d = self.descriptor;
        validate_block(d, block)?;
        let frame = block.header().frame;
        if frame < self.next {
            return Err(Error::Stale);
        }
        if (frame - self.next) / u64::from(d.quantum) >= self.slots.len() as u64 {
            return Err(Error::Future);
        }
        let index = self.index(frame);
        let slot = &mut self.slots[index];
        if slot.frame == Some(frame) {
            return Err(Error::Duplicate);
        }
        slot.block.copy_from(block)?;
        slot.frame = Some(frame);
        Ok(())
    }
    /// Commit one whole logical result at its first output sample. No wall clock
    /// is consulted; a caller cannot consume early or skip the frame sequence.
    pub fn consume_at(
        &mut self,
        frame: u64,
        available: bool,
        output: &mut Block,
    ) -> Result<bool, Error> {
        let d = self.descriptor;
        if self.next.checked_add(u64::from(d.latency)) != Some(frame) {
            return Err(Error::Deadline);
        }
        let index = self.index(self.next);
        let slot = &mut self.slots[index];
        let ready = available && slot.frame == Some(self.next);
        if ready {
            output.copy_from(&slot.block)?;
        } else {
            output.audio_mut().fill(0.);
            output.clear_midi();
            output.set_header(Header {
                generation: d.generation,
                frame: self.next,
                midi_reset: true,
                ..Header::default()
            });
        }
        slot.frame = None;
        self.next = self
            .next
            .checked_add(u64::from(d.quantum))
            .ok_or(Error::Capacity)?;
        Ok(ready)
    }
}
