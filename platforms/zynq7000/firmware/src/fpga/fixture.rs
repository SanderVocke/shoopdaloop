use super::{contract, Contract, RegisterBus};
#[cfg(feature = "fpga-probe")]
use super::{run, shoop_fpga_irq_start, shoop_fpga_irq_stop, shoop_fpga_irq_wait};
#[cfg(feature = "fpga-probe")]
use core::fmt::Write;
use rt_fpga_rtl::host::registers::*;
#[cfg(feature = "fpga")]
extern crate alloc;

extern "C" {
    fn shoop_boot_nonce() -> u64;
    fn shoop_fpga_irq_pend_fixture();
}

#[derive(Clone)]
pub(super) struct Fixture {
    contract: Contract,
    mode: u64,
    request_epoch: u32,
    operation: u32,
    tag: u32,
    #[cfg(feature = "fpga")]
    dispatcher: Dispatcher,
}

#[cfg(feature = "fpga")]
#[derive(Clone)]
struct Patch {
    address: usize,
    expected: u64,
    guard: u64,
    mask: u64,
    value: u64,
    rebase: bool,
}

#[cfg(feature = "fpga")]
struct Patches(alloc::vec::Vec<Patch>);
#[cfg(feature = "fpga")]
impl Clone for Patches {
    fn clone(&self) -> Self {
        let mut patches = alloc::vec::Vec::with_capacity(512);
        patches.extend(self.0.iter().cloned());
        Self(patches)
    }
}
#[cfg(feature = "fpga")]
impl core::ops::Deref for Patches {
    type Target = alloc::vec::Vec<Patch>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[cfg(feature = "fpga")]
impl core::ops::DerefMut for Patches {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "fpga")]
#[derive(Clone)]
struct Dispatcher {
    batch: Patches,
    batch_open: bool,
    batch_limit: usize,
    batch_revision: u64,
    batch_frame: u64,
    batch_deadline: Option<u64>,
    request: [u32; HOST_PAYLOAD_WORDS],
    response: [u32; HOST_PAYLOAD_WORDS],
    code: u32,
    revision: u64,
    frame: u64,
    generation: u32,
    snapshot_base: usize,
    audio: bool,
    held: bool,
    rows: [u64; 16 * 128],
}

impl RegisterBus for Fixture {
    type Error = u32;

    fn read(&mut self, offset: u32) -> Result<u32, Self::Error> {
        let value = match offset {
            REG_ID => HOST_ID ^ u32::from(self.mode == 3),
            REG_VERSION => HOST_VERSION,
            REG_CORE_CLOCK_HZ => self.contract.clock_hz,
            REG_ARENA_BASE => self.contract.arena_base,
            REG_ARENA_BYTES => self.contract.arena_bytes,
            REG_EPOCH => 1,
            REG_SUBMISSION_RESULT => SUBMIT_ACCEPTED,
            REG_FAULT_EPOCH => 0,
            REG_STATUS if self.tag == 0 => STATUS_REQUEST_SPACE,
            REG_STATUS if self.mode == 2 => 0,
            REG_STATUS => STATUS_RESPONSE_PENDING,
            REG_RESPONSE_TAG => self.tag,
            REG_EVENT_COUNT => 0,
            #[cfg(feature = "fpga")]
            REG_PROGRAM_DEPTH | REG_JOB_DEPTH => 1024,
            #[cfg(feature = "fpga")]
            REG_STATE_DEPTH | REG_BUFFERS | REG_VIEWS => 512,
            #[cfg(feature = "fpga")]
            REG_MEDIA_BLOCKS => 31 * 1024,
            #[cfg(feature = "fpga")]
            REG_QUANTUM | REG_CHUNK_FRAMES | REG_BLOCK_CHUNKS => 32,
            #[cfg(feature = "fpga")]
            REG_COMPLETED_QUANTA => (self.dispatcher.frame / 32) as u32,
            #[cfg(feature = "fpga")]
            REG_PEAK_QUANTUM_CYCLES => 500,
            #[cfg(feature = "fpga")]
            REG_AUDIO_XRUNS => 0,
            #[cfg(feature = "fpga")]
            REG_RESPONSE_CODE => self.dispatcher.code,
            #[cfg(feature = "fpga")]
            word if (REG_RESPONSE_DATA..REG_RESPONSE_POP).contains(&word) => {
                self.dispatcher.response[((word - REG_RESPONSE_DATA) / 4) as usize]
            }
            #[cfg(feature = "fpga-probe")]
            REG_RESPONSE_CODE => RESULT_OK,
            #[cfg(feature = "fpga-probe")]
            REG_RESPONSE_DATA => 1,
            #[cfg(feature = "fpga-probe")]
            word if (REG_RESPONSE_DATA..REG_RESPONSE_POP).contains(&word) => 0,
            _ => return Err(offset),
        };
        Ok(value)
    }

    fn write(&mut self, offset: u32, value: u32) -> Result<(), Self::Error> {
        match offset {
            REG_REQUEST_EPOCH => self.request_epoch = value,
            REG_REQUEST_OP => self.operation = value,
            #[cfg(feature = "fpga")]
            REG_REQUEST_PUSH if self.request_epoch == 1 && self.tag == 0 => {
                if self.mode == 1 && self.dispatcher.generation == 0 {
                    unsafe { shoop_fpga_irq_pend_fixture() };
                }
                self.dispatcher.execute(self.operation);
                self.tag = value;
            }
            #[cfg(feature = "fpga-probe")]
            REG_REQUEST_PUSH
                if self.operation == OP_CAPTURE_SNAPSHOT && self.request_epoch == 1 =>
            {
                self.tag = value;
            }
            REG_RESPONSE_POP if self.tag == value => self.tag = 0,
            #[cfg(feature = "fpga")]
            REG_ABORT if value == 1 => self.dispatcher.audio = false,
            #[cfg(feature = "fpga")]
            word if (REG_REQUEST_DATA..REG_REQUEST_PUSH).contains(&word) => {
                self.dispatcher.request[((word - REG_REQUEST_DATA) / 4) as usize] = value;
            }
            #[cfg(feature = "fpga-probe")]
            word if (REG_REQUEST_DATA..REG_REQUEST_PUSH).contains(&word) && value == 0 => {}
            _ => return Err(offset),
        }
        Ok(())
    }
}

#[cfg(feature = "fpga")]
impl Dispatcher {
    fn execute(&mut self, operation: u32) {
        self.response.fill(0);
        self.code = RESULT_OK;
        if matches!(
            operation,
            OP_BEGIN_BATCH | OP_STAGE_PATCH | OP_COMMIT_BATCH | OP_DISCARD_BATCH
        ) && (self.request[9..].iter().any(|word| *word != 0)
            || (matches!(operation, OP_COMMIT_BATCH | OP_DISCARD_BATCH)
                && self.request.iter().any(|word| *word != 0)))
        {
            self.batch_open = false;
            self.code = RESULT_INVALID;
            return;
        }
        let pair = |offset: usize| {
            u64::from(self.request[offset]) | u64::from(self.request[offset + 1]) << 32
        };
        match operation {
            OP_STORE_CONFIG if self.request[..2] == [6, 16] => {
                self.response[1] = 31 * 1024;
            }
            OP_STORE_CONFIG | OP_CLEAR_VIEW | OP_AUDIO_CONFIG => (),
            OP_CAPTURE_SNAPSHOT => {
                if self.audio && !self.held {
                    self.frame += 32;
                }
                self.generation += 1;
                self.snapshot_base = self.request[1] as usize * 16;
                self.response[0] = self.generation;
                self.response[1..5].copy_from_slice(&frame_revision(self.frame, self.revision));
            }
            OP_READ_SNAPSHOT if self.request[0] == self.generation => {
                let start = self.snapshot_base + self.request[1] as usize;
                let count = self.request[2] as usize;
                for (index, dest) in self.response.chunks_exact_mut(2).take(count).enumerate() {
                    let word = self.rows[start + index];
                    dest.copy_from_slice(&[word as u32, (word >> 32) as u32]);
                }
            }
            OP_CORE_CONFIG if self.request[0] == 0x102 => {
                self.rows[self.request[1] as usize] = pair(2);
            }
            OP_CORE_CONFIG => (),
            OP_BEGIN_CONTROL if !self.held && pair(0) == self.revision => {
                self.held = true;
                self.response[..4].copy_from_slice(&frame_revision(self.frame, self.revision));
            }
            OP_END_CONTROL if self.held => self.held = false,
            OP_SWITCH_PROGRAM if pair(0) == self.revision => {
                self.revision = pair(2);
                self.response[..4].copy_from_slice(&frame_revision(self.frame, self.revision));
            }
            OP_BEGIN_BATCH
                if !self.held
                    && (1..=512).contains(&self.request[4])
                    && (1..=32).contains(&self.request[5])
                    && self.request[4] <= self.request[5] * 16
                    && self.request[6] <= 1 =>
            {
                self.batch.clear();
                self.batch_open = true;
                self.batch_limit = self.request[4] as usize;
                self.batch_revision = pair(0);
                self.batch_frame = pair(2);
                self.batch_deadline = (self.request[6] != 0).then(|| pair(7));
            }
            OP_STAGE_PATCH if self.batch_open => {
                let address = (self.request[0] & 0xffff) as usize;
                if self.held
                    || self.request[0] >> 17 != 0
                    || (self.request[0] & (1 << 16) != 0 && !matches!(address & 15, 10 | 13))
                    || self.batch.len() == self.batch_limit
                    || address >= self.rows.len()
                    || self.batch.last().is_some_and(|p| p.address >= address)
                {
                    self.batch_open = false;
                    self.code = RESULT_INVALID;
                } else {
                    self.batch.push(Patch {
                        address,
                        expected: pair(1),
                        guard: pair(3),
                        mask: pair(5),
                        value: pair(7),
                        rebase: self.request[0] & (1 << 16) != 0,
                    });
                }
            }
            OP_COMMIT_BATCH => {
                self.response[..4].copy_from_slice(&frame_revision(self.frame, self.revision));
                let valid = self.batch_open && self.batch.len() == self.batch_limit;
                self.batch_open = false;
                let reason = if !valid {
                    Some(0)
                } else if self.batch_revision != self.revision {
                    Some(2)
                } else if self.batch_frame > self.frame
                    || self.batch_deadline.is_some_and(|end| self.frame > end)
                {
                    Some(1)
                } else if self.batch.iter().any(|p| {
                    (self.rows[p.address] ^ p.expected) & p.guard != 0
                        || (p.rebase
                            && p.value
                                .checked_sub(self.batch_frame)
                                .and_then(|delta| self.frame.checked_add(delta))
                                .is_none())
                }) {
                    Some(3)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    self.code = RESULT_REJECTED;
                    self.response[4] = reason;
                } else {
                    for p in self.batch.iter() {
                        let value = if p.rebase {
                            self.frame + (p.value - self.batch_frame)
                        } else {
                            p.value
                        };
                        self.rows[p.address] = (self.rows[p.address] & !p.mask) | (value & p.mask);
                    }
                }
            }
            OP_DISCARD_BATCH => self.batch_open = false,
            OP_ENABLE_AUDIO => self.audio = self.request[0] == 1,
            OP_DRAIN_STOP if !self.held => self.audio = false,
            _ => {
                if matches!(
                    operation,
                    OP_BEGIN_BATCH | OP_STAGE_PATCH | OP_COMMIT_BATCH | OP_DISCARD_BATCH
                ) {
                    self.batch_open = false;
                    self.code = RESULT_INVALID;
                } else {
                    self.code = RESULT_REJECTED;
                }
            }
        }
    }
}

#[cfg(feature = "fpga")]
fn frame_revision(frame: u64, revision: u64) -> [u32; 4] {
    [
        frame as u32,
        (frame >> 32) as u32,
        revision as u32,
        (revision >> 32) as u32,
    ]
}

pub(super) fn bus() -> Fixture {
    Fixture {
        contract: contract(),
        mode: unsafe { shoop_boot_nonce() },
        request_epoch: 0,
        operation: 0,
        tag: 0,
        #[cfg(feature = "fpga")]
        dispatcher: Dispatcher {
            batch: Patches(alloc::vec::Vec::with_capacity(512)),
            batch_open: false,
            batch_limit: 0,
            batch_revision: 0,
            batch_frame: 0,
            batch_deadline: None,
            request: [0; HOST_PAYLOAD_WORDS],
            response: [0; HOST_PAYLOAD_WORDS],
            code: 0,
            frame: 0,
            revision: 0,
            generation: 0,
            snapshot_base: 0,
            audio: false,
            held: false,
            rows: [0; 16 * 128],
        },
    }
}

#[cfg(feature = "fpga-probe")]
pub fn probe() {
    let expected = contract();
    let mode = unsafe { shoop_boot_nonce() };
    if mode == 1 {
        unsafe {
            assert_eq!(shoop_fpga_irq_start(), 1);
            shoop_fpga_irq_pend_fixture();
            assert_eq!(shoop_fpga_irq_wait(), 1);
            assert_eq!(shoop_fpga_irq_wait(), 0);
            shoop_fpga_irq_stop();
        }
        let _ = writeln!(crate::Serial, "SHOOP_FPGA_FIXTURE_IRQ_PASS");
    }
    run(bus(), expected, || {});
}
