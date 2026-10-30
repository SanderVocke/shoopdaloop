use std::io;
use std::sync::atomic::AtomicU32;
use std::time::Instant;

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct Completion;

#[cfg(target_os = "linux")]
impl Completion {
    pub fn create() -> io::Result<Self> {
        Ok(Self)
    }

    pub fn open(port: u32) -> io::Result<Self> {
        if port != 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Self)
    }

    pub fn port(&self) -> u32 {
        0
    }

    pub fn wait(&self, state: &AtomicU32, expected: u32, deadline: Instant) -> io::Result<()> {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(());
        };
        let timeout = libc::timespec {
            tv_sec: remaining.as_secs().min(libc::time_t::MAX as u64) as libc::time_t,
            tv_nsec: remaining.subsec_nanos() as libc::c_long,
        };
        let result = unsafe {
            // The word is shared across processes, so FUTEX_PRIVATE_FLAG is not valid here.
            libc::syscall(
                libc::SYS_futex,
                state.as_ptr(),
                libc::FUTEX_WAIT,
                expected,
                &timeout,
            )
        };
        if result >= 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EAGAIN | libc::EINTR | libc::ETIMEDOUT) => Ok(()),
            _ => Err(error),
        }
    }

    pub fn notify(&self, state: &AtomicU32) -> io::Result<()> {
        let result = unsafe { libc::syscall(libc::SYS_futex, state.as_ptr(), libc::FUTEX_WAKE, 1) };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub(crate) struct Completion {
    socket: std::net::UdpSocket,
    port: u32,
}

#[cfg(not(target_os = "linux"))]
impl Completion {
    pub fn create() -> io::Result<Self> {
        let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let port = u32::from(socket.local_addr()?.port());
        Ok(Self { socket, port })
    }

    pub fn open(port: u32) -> io::Result<Self> {
        let port = u16::try_from(port)
            .ok()
            .filter(|port| *port != 0)
            .ok_or(io::ErrorKind::InvalidData)?;
        let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        socket.connect((std::net::Ipv4Addr::LOCALHOST, port))?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            port: u32::from(port),
        })
    }

    pub fn port(&self) -> u32 {
        self.port
    }

    pub fn wait(&self, _state: &AtomicU32, _expected: u32, deadline: Instant) -> io::Result<()> {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(());
        };
        if remaining.is_zero() {
            return Ok(());
        }
        self.socket.set_read_timeout(Some(remaining))?;
        match self.socket.recv_from(&mut [0_u8; 1]) {
            Ok(_) => Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub fn notify(&self, _state: &AtomicU32) -> io::Result<()> {
        match self.socket.send(&[0]) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error),
        }
    }
}
