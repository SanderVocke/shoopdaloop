//! `PortIo` through the CPU host bridge's AXI-lite register map.
pub mod map;

use crate::{
    looping::{MediaCommand, MediaResult, TpReply},
    port::{Capacities, PortIo, Run, Status, StoreReply},
};
use alloc::{format, string::String, vec::Vec};

/// 32-bit register access to the bridge, addressed by byte offset.
pub trait Bus {
    fn read(&mut self, offset: u32) -> u32;
    fn write(&mut self, offset: u32, value: u32);
}

/// Bridge counters that observe execution without affecting it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub quanta: u64,
    pub last_cycles: u32,
    pub output_words: u32,
    pub digest: u64,
}

pub struct Bridge<B> {
    bus: B,
    capacities: Capacities,
    physical_base: u32,
    slot_limit: u32,
    media_slots: u32,
    /// Status polls before a port transaction is reported as timed out.
    pub poll_limit: u32,
    pub last_error: Option<String>,
}

fn field(word: u32, (lsb, width): (u32, u32)) -> u32 {
    (word >> lsb) & ((1 << width) - 1)
}

impl<B: Bus> Bridge<B> {
    /// Identifies the bridge and reads the compiled capacities of its core.
    pub fn new(mut bus: B) -> Result<Self, String> {
        let identity = bus.read(map::ID);
        if identity != map::IDENTITY {
            return Err(format!("no host bridge: identity {identity:#010x}"));
        }
        let version = bus.read(map::VERSION);
        if version != map::VERSION_NUMBER {
            return Err(format!("unsupported host bridge version {version}"));
        }
        let mut capacity = |name: &str| {
            let index = map::CAPACITIES
                .iter()
                .position(|n| *n == name)
                .expect("capacity name in the generated map") as u32;
            bus.read(map::CAPACITY + 4 * index)
        };
        let slots = capacity("SLOTS");
        let physical_base = capacity("PHYSICAL_BASE");
        let capacities = Capacities {
            instructions: capacity("PROGRAM_DEPTH") as usize,
            states: capacity("STATE_DEPTH") as usize,
            buffers: capacity("BUFFERS"),
            quantum: capacity("MAX_FRAMES"),
            input_ports: capacity("INPUT_PORTS") as u16,
            output_ports: capacity("OUTPUT_PORTS") as u16,
            loop_slots: capacity("LOOP_DEPTH"),
            pending_requests: capacity("REQUEST_DEPTH"),
            events: capacity("EVENT_DEPTH"),
            views: capacity("VIEWS") as usize,
            chunk_limit: slots,
            external_audio: capacity("EXTERNAL_IO") != 0,
        };
        Ok(Self {
            bus,
            capacities,
            physical_base,
            slot_limit: slots,
            media_slots: slots.min(128),
            poll_limit: 1_000_000,
            last_error: None,
        })
    }
    /// The DDR address of the core's arena, fixed when the bitstream was built.
    pub fn physical_base(&self) -> u32 {
        self.physical_base
    }
    pub fn bus(&self) -> &B {
        &self.bus
    }
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }
    /// Sets the media store slot limit applied by later image loads.
    pub fn arena_capacity(&mut self, slots: u32) -> Result<(), String> {
        if slots == 0 || slots > self.slot_limit {
            return Err(format!(
                "arena capacity {slots} outside 1..={}",
                self.slot_limit
            ));
        }
        self.media_slots = slots;
        Ok(())
    }
    pub fn counters(&mut self) -> Counters {
        let pair = |bus: &mut B, offset| {
            u64::from(bus.read(offset)) | u64::from(bus.read(offset + 4)) << 32
        };
        Counters {
            quanta: pair(&mut self.bus, map::QUANTA),
            last_cycles: self.bus.read(map::LAST_CYCLES),
            output_words: self.bus.read(map::OUTPUT_WORDS),
            digest: pair(&mut self.bus, map::DIGEST),
        }
    }
    fn wait(&mut self, busy: (u32, u32), what: &str) -> Result<u32, String> {
        for _ in 0..self.poll_limit {
            let status = self.bus.read(map::STATUS);
            if field(status, busy) == 0 {
                return Ok(status);
            }
        }
        Err(format!("{what} timeout"))
    }
    fn write_words(&mut self, base: u32, words: &[u32]) {
        for (i, &word) in words.iter().enumerate() {
            self.bus.write(base + 4 * i as u32, word);
        }
    }
    fn read_words<const N: usize>(&mut self, base: u32) -> [u32; N] {
        core::array::from_fn(|i| self.bus.read(base + 4 * i as u32))
    }
}

impl<B: Bus> PortIo for Bridge<B> {
    fn capacities(&self) -> Capacities {
        self.capacities
    }
    fn media_slots(&self) -> u32 {
        self.media_slots
    }
    fn reset_core(&mut self) -> Result<(), String> {
        self.bus.write(map::CONTROL, 1);
        self.wait(map::STATUS_RESET_BUSY, "core reset").map(|_| ())
    }
    fn core_status(&mut self) -> Result<Status, String> {
        let status = self.bus.read(map::STATUS);
        Ok(Status {
            faulted: field(status, map::STATUS_FAULTED) != 0,
            error_stage: field(status, map::STATUS_ERROR_STAGE) as u8,
            error_code: field(status, map::STATUS_ERROR_CODE) as u8,
        })
    }
    fn transition(&mut self, opcode: u8, index: u32, data: [u32; 20]) -> Result<TpReply, String> {
        self.bus.write(map::TP_OPCODE, u32::from(opcode));
        self.bus.write(map::TP_INDEX, index);
        self.write_words(map::TP_DATA, &data);
        self.bus.write(map::TP_GO, 1);
        self.wait(map::STATUS_TP_BUSY, "TP response")?;
        let error = self.bus.read(map::TP_ERROR) as u8;
        Ok(TpReply {
            error,
            words: self.read_words(map::TP_RESPONSE),
        })
    }
    fn configure(
        &mut self,
        program: bool,
        address: u16,
        write: Option<[u32; 5]>,
    ) -> Result<[u32; 5], String> {
        let control = u32::from(!program) | u32::from(write.is_some()) << 1;
        self.bus.write(map::CFG_CONTROL, control);
        self.bus.write(map::CFG_ADDRESS, u32::from(address));
        if let Some(words) = write {
            self.write_words(map::CFG_WRITE, &words);
        }
        self.bus.write(map::CFG_GO, 1);
        self.wait(map::STATUS_CFG_BUSY, "configuration response")?;
        if self.bus.read(map::CFG_ERROR) != 0 {
            return Err("configuration rejected".into());
        }
        Ok(self.read_words(map::CFG_READ))
    }
    fn audio_word(&mut self, space: u8, address: u32, write: Option<i32>) -> Result<i32, String> {
        self.bus.write(
            map::AUDIO_CONTROL,
            u32::from(space) | u32::from(write.is_some()) << 2,
        );
        self.bus.write(map::AUDIO_ADDRESS, address);
        self.bus
            .write(map::AUDIO_WRITE, write.unwrap_or_default() as u32);
        self.bus.write(map::AUDIO_GO, 1);
        self.wait(map::STATUS_AUDIO_BUSY, "audio configuration response")?;
        if self.bus.read(map::AUDIO_ERROR) != 0 {
            return Err("audio configuration rejected".into());
        }
        Ok(self.bus.read(map::AUDIO_READ) as i32)
    }
    fn store_configure(
        &mut self,
        space: u8,
        address: u32,
        write: Option<[u32; 5]>,
    ) -> Result<[u32; 5], String> {
        let control = u32::from(space) | u32::from(write.is_some()) << 3;
        self.bus.write(map::STORE_CFG_CONTROL, control);
        self.bus.write(map::STORE_CFG_ADDRESS, address);
        if let Some(words) = write {
            self.write_words(map::STORE_CFG_WRITE, &words);
        }
        self.bus.write(map::STORE_CFG_GO, 1);
        self.wait(map::STATUS_STORE_CFG_BUSY, "media configuration response")?;
        if self.bus.read(map::STORE_CFG_ERROR) != 0 {
            return Err("media configuration response failed".into());
        }
        Ok(self.read_words(map::STORE_CFG_READ))
    }
    fn store_command(
        &mut self,
        command: MediaCommand,
        write: &[i32],
    ) -> Result<StoreReply, String> {
        self.bus.write(map::STORE_OPCODE, u32::from(command.opcode));
        self.bus.write(map::STORE_VIEW, command.view);
        self.write_words(
            map::STORE_START,
            &[command.start as u32, (command.start >> 32) as u32],
        );
        self.write_words(
            map::STORE_LENGTH,
            &[command.length as u32, (command.length >> 32) as u32],
        );
        self.bus.write(map::STORE_COUNT, command.count);
        self.bus.write(map::STORE_GO, 1);
        let mut samples = Vec::new();
        let mut pushed = 0;
        let mut finished = false;
        for _ in 0..self.poll_limit {
            let busy = field(self.bus.read(map::STATUS), map::STATUS_STORE_BUSY) != 0;
            let space = self.bus.read(map::STORE_TX_SPACE) as usize;
            for &sample in write.iter().skip(pushed).take(if busy { space } else { 0 }) {
                self.bus.write(map::STORE_TX, sample as u32);
                pushed += 1;
            }
            for _ in 0..self.bus.read(map::STORE_RX_LEVEL) {
                samples.push(self.bus.read(map::STORE_RX) as i32);
            }
            if !busy {
                finished = true;
                break;
            }
        }
        if !finished {
            return Err(format!(
                "media completion timeout: opcode {}, view {}, frames {}",
                command.opcode, command.view, command.length
            ));
        }
        let result = self.bus.read(map::STORE_RESULT);
        Ok(StoreReply {
            code: result as u8,
            result: MediaResult {
                view: self.bus.read(map::STORE_RESULT_VIEW),
                empty: result & 0x100 != 0,
                samples,
            },
            written: self.bus.read(map::STORE_WRITTEN) as usize,
        })
    }
    fn run_quantum(&mut self, run: Run) -> Result<Status, String> {
        self.write_words(
            map::RUN,
            &[
                run.instructions,
                run.states,
                run.quantum,
                run.chunk_frames,
                run.job_budget,
                run.buffers,
                run.inputs,
                run.outputs,
            ],
        );
        self.bus.write(map::RUN_GO, 1);
        self.wait(map::STATUS_RUN_BUSY, "pipeline completion")?;
        self.core_status()
    }
    fn record_error(&mut self, message: String) {
        self.last_error = Some(message);
    }
}
