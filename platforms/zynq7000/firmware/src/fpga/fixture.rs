use super::{contract, run, Contract, RegisterBus};
use rt_fpga_rtl::host::registers::*;

extern "C" {
    fn shoop_boot_nonce() -> u64;
}

struct Fixture {
    contract: Contract,
    mode: u64,
    request_epoch: u32,
    operation: u32,
    tag: u32,
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
            REG_STATUS if self.tag == 0 => STATUS_REQUEST_SPACE,
            REG_STATUS if self.mode == 2 => 0,
            REG_STATUS => STATUS_RESPONSE_PENDING,
            REG_RESPONSE_TAG => self.tag,
            REG_RESPONSE_CODE => RESULT_OK,
            REG_RESPONSE_DATA => 1,
            word if (REG_RESPONSE_DATA..REG_RESPONSE_POP).contains(&word) => 0,
            _ => return Err(offset),
        };
        Ok(value)
    }

    fn write(&mut self, offset: u32, value: u32) -> Result<(), Self::Error> {
        match offset {
            REG_REQUEST_EPOCH => self.request_epoch = value,
            REG_REQUEST_OP => self.operation = value,
            REG_REQUEST_PUSH
                if self.operation == OP_CAPTURE_SNAPSHOT && self.request_epoch == 1 =>
            {
                self.tag = value;
            }
            REG_RESPONSE_POP if self.tag == value => self.tag = 0,
            word if (REG_REQUEST_DATA..REG_REQUEST_PUSH).contains(&word) && value == 0 => {}
            _ => return Err(offset),
        }
        Ok(())
    }
}

pub fn probe() {
    let expected = contract();
    run(
        Fixture {
            contract: expected,
            mode: unsafe { shoop_boot_nonce() },
            request_epoch: 0,
            operation: 0,
            tag: 0,
        },
        expected,
    );
}
