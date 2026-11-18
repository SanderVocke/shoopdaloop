use super::{Error, Mailbox, RegisterBus, Response, SyncEvent, registers::*};
use crate::target::{ConfigurationWrite, Space};

/// Encoded dispatcher operations. Construction and transmission use fixed-size storage.
pub struct Request {
    op: u32,
    words: [u32; HOST_PAYLOAD_WORDS],
}

impl Request {
    fn new(op: u32, words: &[u32]) -> Self {
        let mut data = [0; HOST_PAYLOAD_WORDS];
        data[..words.len()].copy_from_slice(words);
        Self { op, words: data }
    }

    pub fn configuration_write(write: &ConfigurationWrite) -> Option<Self> {
        let (op, space, address, width) = match write.space {
            Space::Program => (
                OP_CORE_CONFIG,
                0,
                u32::from(u16::try_from(write.address).ok()?),
                96,
            ),
            Space::State => (
                OP_CORE_CONFIG,
                1,
                u32::from(u16::try_from(write.address).ok()?),
                129,
            ),
            Space::Loop => (
                OP_CORE_CONFIG,
                2,
                u32::from(u16::try_from(write.address).ok()?),
                64,
            ),
            Space::History => (OP_AUDIO_CONFIG, 3, u32::try_from(write.address).ok()?, 32),
        };
        if !write.value.fits(width) {
            return None;
        }
        let mut words = [0; 7];
        words[0] = 0x100 | space;
        words[1] = address;
        words[2..].copy_from_slice(&write.value.array::<5>());
        Some(Self::new(op, &words))
    }

    pub fn input_sample(address: u32, sample: i32) -> Self {
        Self::new(OP_AUDIO_CONFIG, &[0x100, address, sample as u32])
    }

    pub fn output_sample(address: u32) -> Self {
        Self::new(OP_AUDIO_CONFIG, &[1, address])
    }

    pub fn run_quantum() -> Self {
        Self::new(OP_RUN_QUANTUM, &[])
    }

    pub fn enable_audio(enable: bool) -> Self {
        Self::new(OP_ENABLE_AUDIO, &[u32::from(enable)])
    }

    pub fn capture_snapshot(loop_slots: u32) -> Self {
        Self::new(OP_CAPTURE_SNAPSHOT, &[loop_slots])
    }

    pub fn read_snapshot(generation: u32, word: u32, count: u32) -> Self {
        Self::new(OP_READ_SNAPSHOT, &[generation, word, count])
    }
}

/// Single-owner dispatcher client. A request remains outstanding until its exact tag returns.
pub struct Client<B> {
    mailbox: Mailbox<B>,
    next_tag: u32,
    pending: Option<u32>,
}

impl<B: RegisterBus> Client<B> {
    pub fn new(mailbox: Mailbox<B>) -> Self {
        Self {
            mailbox,
            next_tag: 1,
            pending: None,
        }
    }

    pub fn start(&mut self, request: Request) -> Result<(), Error<B::Error>> {
        if self.pending.is_some() {
            return Err(Error::Full);
        }
        let tag = self.next_tag;
        self.mailbox.submit(tag, request.op, &request.words)?;
        self.pending = Some(tag);
        self.next_tag = tag.wrapping_add(1).max(1);
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<Response>, Error<B::Error>> {
        let Some(response) = self.mailbox.response()? else {
            return Ok(None);
        };
        if self.pending != Some(response.tag) {
            return Err(Error::UnexpectedResponse);
        }
        self.pending = None;
        Ok(Some(response))
    }

    pub fn sync_event(&mut self) -> Result<Option<SyncEvent>, Error<B::Error>> {
        self.mailbox.sync_event()
    }
}
