use super::{Error, Mailbox, Metrics, RegisterBus, Response, SyncEvent, registers::*};
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

    pub fn begin_control(revision: u64) -> Self {
        Self::new(
            OP_BEGIN_CONTROL,
            &[revision as u32, (revision >> 32) as u32],
        )
    }

    pub fn end_control() -> Self {
        Self::new(OP_END_CONTROL, &[])
    }

    pub fn drain_stop() -> Self {
        Self::new(OP_DRAIN_STOP, &[])
    }

    pub fn clear_view(view: u32) -> Self {
        Self::new(OP_CLEAR_VIEW, &[view])
    }

    pub fn store_word(space: u8, address: u32, value: u32) -> Self {
        Self::new(OP_STORE_CONFIG, &[0x100 | u32::from(space), address, value])
    }

    pub fn storage_usage() -> Self {
        Self::new(OP_STORE_CONFIG, &[6, 16])
    }

    #[cfg(feature = "rt-interface")]
    pub fn switch_program(
        expected: u64,
        program: shoop_rt_protocol::immediate::SwitchProgram,
    ) -> Self {
        Self::new(
            OP_SWITCH_PROGRAM,
            &[
                expected as u32,
                (expected >> 32) as u32,
                program.revision as u32,
                (program.revision >> 32) as u32,
                u32::from(program.region),
                u32::from(program.instructions),
                u32::from(program.sync_count),
                u32::from(program.states),
                u32::from(program.jobs),
                u32::from(program.buffers),
            ],
        )
    }

    pub fn capture_snapshot(loop_slots: u32) -> Self {
        Self::new(OP_CAPTURE_SNAPSHOT, &[loop_slots])
    }

    pub fn capture_loop(slot: u32) -> Self {
        Self::new(OP_CAPTURE_SNAPSHOT, &[1, slot])
    }

    pub fn read_snapshot(generation: u32, word: u32, count: u32) -> Self {
        Self::new(OP_READ_SNAPSHOT, &[generation, word, count])
    }

    pub fn write_loop_word(address: u32, value: u64) -> Self {
        Self::new(
            OP_CORE_CONFIG,
            &[0x102, address, value as u32, (value >> 32) as u32],
        )
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

    /// Run one request with an owner-supplied time budget. A timeout retains its tag.
    pub fn exchange(
        &mut self,
        request: Request,
        mut time_left: impl FnMut() -> bool,
    ) -> Result<Response, Error<B::Error>> {
        self.start(request)?;
        loop {
            if let Some(response) = self.poll()? {
                return Ok(response);
            }
            if !time_left() {
                return Err(Error::Timeout);
            }
        }
    }

    pub fn sync_event(&mut self) -> Result<Option<SyncEvent>, Error<B::Error>> {
        self.mailbox.sync_event()
    }

    pub fn metrics(&mut self) -> Result<Metrics, Error<B::Error>> {
        self.mailbox.metrics()
    }

    pub fn abort(&mut self) -> Result<(), Error<B::Error>> {
        self.mailbox.abort()
    }
}
