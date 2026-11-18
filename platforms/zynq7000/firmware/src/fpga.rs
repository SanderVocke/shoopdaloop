use crate::Serial;
#[cfg(feature = "fpga-probe")]
use core::fmt::Debug;
use core::fmt::Write;
#[cfg(not(feature = "fixture"))]
use core::{arch::asm, convert::Infallible, ptr};
#[cfg(feature = "fpga-probe")]
use rt_fpga_rtl::host::{registers::RESULT_OK, Client, Error, Request};
use rt_fpga_rtl::host::{Contract, Mailbox, RegisterBus};

extern "C" {
    #[cfg(not(feature = "fixture"))]
    fn shoop_fpga_configured() -> i32;
    #[cfg(not(feature = "fixture"))]
    fn shoop_fpga_register_base() -> u32;
    fn shoop_fpga_arena_base() -> u32;
    fn shoop_fpga_arena_bytes() -> u32;
    fn shoop_fpga_clock_hz() -> u32;
    #[cfg(feature = "fpga-probe")]
    fn shoop_millis() -> u64;
    #[cfg(feature = "fpga-probe")]
    fn shoop_fpga_irq_start() -> i32;
    #[cfg(feature = "fpga-probe")]
    fn shoop_fpga_irq_wait() -> i32;
    #[cfg(feature = "fpga-probe")]
    fn shoop_fpga_irq_stop();
}

#[cfg(not(feature = "fixture"))]
struct DeviceRegisters(usize);

#[cfg(not(feature = "fixture"))]
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

fn contract() -> Contract {
    unsafe {
        Contract {
            clock_hz: shoop_fpga_clock_hz(),
            arena_base: shoop_fpga_arena_base(),
            arena_bytes: shoop_fpga_arena_bytes(),
        }
    }
}

#[cfg(not(feature = "fixture"))]
fn configured() -> bool {
    unsafe { shoop_fpga_configured() != 0 }
}

#[cfg(all(feature = "fpga-probe", not(feature = "fixture")))]
pub fn probe() {
    if !configured() {
        let _ = writeln!(
            Serial,
            "SHOOP_FPGA_PROBE_UNAVAILABLE no configured bitstream"
        );
        return;
    }
    if unsafe { shoop_fpga_irq_start() } != 1 {
        let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE IRQ setup failed");
        return;
    }
    run(
        DeviceRegisters(unsafe { shoop_fpga_register_base() } as usize),
        contract(),
        || {
            unsafe { shoop_fpga_irq_wait() };
        },
    );
    unsafe { shoop_fpga_irq_stop() };
}

#[cfg(feature = "fpga-probe")]
fn run<B: RegisterBus>(bus: B, expected: Contract, mut wait: impl FnMut())
where
    B::Error: Debug,
{
    let mailbox = match Mailbox::open(bus, expected) {
        Ok(mailbox) => mailbox,
        Err(error) => {
            let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE {error:?}");
            return;
        }
    };
    let mut client = Client::new(mailbox);
    let start = unsafe { shoop_millis() };
    match client.exchange(Request::capture_snapshot(0), || {
        wait();
        unsafe { shoop_millis() }.wrapping_sub(start) < 25
    }) {
        Ok(response) if response.code == RESULT_OK && response.data[0] != 0 => {
            let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_READY {expected:?}");
        }
        Ok(response) => {
            let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE {response:?}");
        }
        Err(Error::Timeout) => {
            let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE snapshot timeout");
        }
        Err(error) => {
            let _ = writeln!(Serial, "SHOOP_FPGA_PROBE_UNAVAILABLE {error:?}");
        }
    }
}

#[cfg(feature = "fixture")]
mod fixture;
#[cfg(all(feature = "fixture", feature = "fpga-probe"))]
pub use fixture::probe;

#[cfg(feature = "fpga")]
pub fn serve() {
    #[cfg(not(feature = "fixture"))]
    let bus = {
        if !configured() {
            let _ = writeln!(
                Serial,
                "SHOOP_FPGA_HOST_UNAVAILABLE no configured bitstream"
            );
            return;
        }
        DeviceRegisters(unsafe { shoop_fpga_register_base() } as usize)
    };
    #[cfg(feature = "fixture")]
    let bus = fixture::bus();
    match Mailbox::open(bus, contract()) {
        Ok(mailbox) => shoop_host_freertos::run_fpga(mailbox),
        Err(error) => {
            let _ = writeln!(Serial, "SHOOP_FPGA_HOST_UNAVAILABLE {error:?}");
        }
    }
}
