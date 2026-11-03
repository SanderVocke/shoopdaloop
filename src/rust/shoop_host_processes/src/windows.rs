use super::*;
use std::{io, mem::MaybeUninit};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    System::Threading::{
        GetProcessTimes, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_TERMINATE,
    },
};

struct Handle(HANDLE);
impl Handle {
    fn open(pid: u32, terminate: bool) -> Result<Self> {
        // OpenProcess returns an owned handle, released by Drop.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | if terminate { PROCESS_TERMINATE } else { 0 },
                0,
                pid,
            )
        };
        ensure!(
            !handle.is_null(),
            "Open process: {}",
            io::Error::last_os_error()
        );
        Ok(Self(handle))
    }
    fn creation(&self) -> Result<u64> {
        let mut times = [MaybeUninit::<FILETIME>::uninit(); 4];
        // Four distinct, correctly-sized output buffers remain alive through the call.
        let ok = unsafe {
            GetProcessTimes(
                self.0,
                times[0].as_mut_ptr(),
                times[1].as_mut_ptr(),
                times[2].as_mut_ptr(),
                times[3].as_mut_ptr(),
            )
        };
        ensure!(
            ok != 0,
            "Read process times: {}",
            io::Error::last_os_error()
        );
        // A successful GetProcessTimes initializes all four buffers.
        let created = unsafe { times[0].assume_init() };
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // This is the owned handle returned by OpenProcess.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub(super) fn creation(pid: u32) -> Result<u64> {
    Handle::open(pid, false)?.creation()
}
pub(super) fn terminate(expected: &Identity) -> Result<()> {
    let handle = Handle::open(expected.pid, true)?;
    ensure!(
        handle.creation()? == expected.started
            && identity(expected.pid)?.as_ref() == Some(expected),
        "Process identity changed"
    );
    // The retained handle targets the verified process even if its PID is later reused.
    ensure!(
        unsafe { TerminateProcess(handle.0, 1) } != 0,
        "Terminate process: {}",
        io::Error::last_os_error()
    );
    Ok(())
}
