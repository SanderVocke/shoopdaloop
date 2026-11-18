use crate::Serial;
use core::{arch::asm, convert::Infallible, fmt::Write, ptr};
use rt_fpga_rtl::host::{registers::RESULT_OK, Client, Contract, Mailbox, RegisterBus, Request};

const DEVCFG_INT_STATUS: usize = 0xf800700c;
const PCFG_DONE: u32 = 1 << 2;

extern "C" {
    fn shoop_fpga_register_base() -> u32;
    fn shoop_fpga_arena_base() -> u32;
    fn shoop_fpga_arena_bytes() -> u32;
    fn shoop_fpga_clock_hz() -> u32;
    fn shoop_millis() -> u64;
}

struct DeviceRegisters(usize);

impl RegisterBus for DeviceRegisters {
    type Error = Infallible;

    fn read(&mut self, offset: u32) -> Result<u32, Self::Error> {
        let address = ptr::with_exposed_provenance(self.0 + offset as usize);
        let value = unsafe {
            asm!("dmb sy", options(nostack, preserves_flags));
            let value = ptr::read_volatile(address);
            asm!("dmb sy", options(nostack, preserves_flags));
            value
        };
        Ok(value)
    }

    fn write(&mut self, offset: u32, value: u32) -> Result<(), Self::Error> {
        let address = ptr::with_exposed_provenance_mut(self.0 + offset as usize);
        unsafe {
            asm!("dmb sy", options(nostack, preserves_flags));
            ptr::write_volatile(address, value);
            asm!("dmb sy", options(nostack, preserves_flags));
        }
        Ok(())
    }
}

pub fn probe() {
    let configured =
        unsafe { ptr::read_volatile::<u32>(ptr::with_exposed_provenance(DEVCFG_INT_STATUS)) };
    if configured & PCFG_DONE == 0 {
        let _ = writeln!(
            Serial,
            "SHOOP_FPGA_PROBE_UNAVAILABLE no configured bitstream"
        );
        return;
    }
    let (base, expected) = unsafe {
        (
            shoop_fpga_register_base(),
            Contract {
                clock_hz: shoop_fpga_clock_hz(),
                arena_base: shoop_fpga_arena_base(),
                arena_bytes: shoop_fpga_arena_bytes(),
            },
        )
    };
    let mailbox = match Mailbox::open(DeviceRegisters(base as usize), expected) {
        Ok(mailbox) => mailbox,
        Err(error) => {
            let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE {error:?}");
            return;
        }
    };
    let mut client = Client::new(mailbox);
    if let Err(error) = client.start(Request::capture_snapshot(0)) {
        let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE {error:?}");
        return;
    }
    let start = unsafe { shoop_millis() };
    while unsafe { shoop_millis() }.wrapping_sub(start) < 25 {
        match client.poll() {
            Ok(None) => continue,
            Ok(Some(response)) if response.code == RESULT_OK && response.data[0] != 0 => {
                let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_READY {expected:?}");
            }
            Ok(Some(response)) => {
                let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE {response:?}");
            }
            Err(error) => {
                let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE {error:?}");
            }
        }
        return;
    }
    let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE snapshot timeout");
}
