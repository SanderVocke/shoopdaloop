//! Live OS process/socket inspection; no saved discovery state.
#![cfg(not(target_arch = "wasm32"))]
#![deny(unsafe_op_in_unsafe_fn)]

use anyhow::{bail, ensure, Context, Result};
#[cfg(not(target_os = "linux"))]
use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Identity {
    pub pid: u32,
    pub started: u64,
    pub executable: PathBuf,
}
#[derive(Clone, Debug)]
pub struct Process {
    pub identity: Identity,
    pub name: String,
    pub addresses: Vec<SocketAddr>,
}
#[derive(Default)]
pub struct Snapshot {
    pub processes: Vec<Process>,
    pub errors: Vec<String>,
}
fn system(pids: ProcessesToUpdate<'_>) -> Result<System> {
    let mut system = System::new().context("Inspect OS processes")?;
    system.refresh_processes_specifics(
        pids,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    Ok(system)
}
fn identity_of(process: &sysinfo::Process) -> Result<Identity> {
    Ok(Identity {
        pid: process.pid().as_u32(),
        started: started(process.pid().as_u32())?,
        executable: process
            .exe()
            .context("Executable path unavailable")?
            .to_owned(),
    })
}
pub fn identity(pid: u32) -> Result<Option<Identity>> {
    let system = system(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]))?;
    system
        .process(Pid::from_u32(pid))
        .filter(|process| process.status() != ProcessStatus::Zombie)
        .map(identity_of)
        .transpose()
}
pub fn host_executable(path: &std::path::Path) -> bool {
    path.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case("shoop_host") || name.eq_ignore_ascii_case("shoop_host.exe")
    })
}
pub fn snapshot() -> Result<Snapshot> {
    let system = system(ProcessesToUpdate::All)?;
    let pids: Vec<_> = system
        .processes()
        .iter()
        .filter(|(_, process)| {
            process.status() != ProcessStatus::Zombie
                && (process.exe().is_some_and(host_executable)
                    || host_executable(std::path::Path::new(process.name())))
        })
        .map(|(pid, _)| pid.as_u32())
        .collect();
    inspect_with_system(&system, &pids)
}
pub fn inspect(pids: &[u32]) -> Result<Snapshot> {
    let requested: Vec<_> = pids.iter().copied().map(Pid::from_u32).collect();
    let system = system(ProcessesToUpdate::Some(&requested))?;
    inspect_with_system(&system, pids)
}
fn inspect_with_system(system: &System, pids: &[u32]) -> Result<Snapshot> {
    let mut snapshot = Snapshot::default();
    for pid in pids {
        let Some(process) = system
            .process(Pid::from_u32(*pid))
            .filter(|p| p.status() != ProcessStatus::Zombie)
        else {
            continue;
        };
        match identity_of(process) {
            Ok(identity) => {
                snapshot.processes.push(Process {
                    identity,
                    name: process.name().to_string_lossy().into_owned(),
                    addresses: vec![],
                });
            }
            Err(error) => snapshot.errors.push(format!("PID {pid}: {error:#}")),
        }
    }
    if snapshot.processes.is_empty() {
        return Ok(snapshot);
    }
    #[cfg(target_os = "linux")]
    linux::sockets(&mut snapshot);
    #[cfg(not(target_os = "linux"))]
    sockets(&mut snapshot)?;
    for process in &mut snapshot.processes {
        process.addresses.sort();
        process.addresses.dedup();
    }
    snapshot.processes.sort_by_key(|p| p.identity.pid);
    Ok(snapshot)
}
#[cfg(not(target_os = "linux"))]
fn sockets(snapshot: &mut Snapshot) -> Result<()> {
    let indices: std::collections::HashMap<_, _> = snapshot
        .processes
        .iter()
        .enumerate()
        .map(|(index, p)| (p.identity.pid, index))
        .collect();
    let sockets = netstat2::iterate_sockets_info(
        AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6,
        ProtocolFlags::TCP,
    )
    .context("Inspect OS TCP socket ownership")?;
    for socket in sockets {
        match socket {
            Ok(socket) => {
                let ProtocolSocketInfo::Tcp(tcp) = socket.protocol_socket_info else {
                    continue;
                };
                if tcp.state != TcpState::Listen {
                    continue;
                }
                let address = SocketAddr::new(tcp.local_addr, tcp.local_port);
                for pid in socket.associated_pids {
                    if let Some(index) = indices.get(&pid) {
                        snapshot.processes[*index].addresses.push(address);
                    }
                }
            }
            Err(error) => {
                if snapshot.errors.len() < 16 {
                    snapshot.errors.push(format!("Socket inspection: {error}"));
                }
            }
        }
    }
    Ok(())
}
pub fn force_stop(expected: &Identity) -> Result<()> {
    ensure!(
        expected.pid != std::process::id(),
        "Cannot terminate the manager itself"
    );
    #[cfg(target_os = "linux")]
    {
        use rustix::process::{pidfd_open, pidfd_send_signal, PidfdFlags, Signal};
        let pid = rustix::process::Pid::from_raw(expected.pid as i32).context("Invalid PID")?;
        let handle = pidfd_open(pid, PidfdFlags::empty()).context("Open process handle")?;
        ensure!(
            identity(expected.pid)?.as_ref() == Some(expected),
            "Process identity changed"
        );
        pidfd_send_signal(&handle, Signal::KILL).context("Terminate process")?;
    }
    #[cfg(windows)]
    windows::terminate(expected)?;
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let system = system(ProcessesToUpdate::Some(&[Pid::from_u32(expected.pid)]))?;
        let process = system
            .process(Pid::from_u32(expected.pid))
            .context("Process has exited")?;
        ensure!(
            identity_of(process)? == *expected,
            "Process identity changed"
        );
        ensure!(process.kill(), "OS refused process termination");
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if identity(expected.pid)?.as_ref() != Some(expected) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("Process termination has not completed");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[cfg(target_os = "linux")]
fn started(pid: u32) -> Result<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let fields = stat.rsplit_once(')').context("Invalid process stat")?.1;
    Ok(fields
        .split_whitespace()
        .nth(19)
        .context("Missing process start time")?
        .parse()?)
}
#[cfg(target_os = "macos")]
fn started(pid: u32) -> Result<u64> {
    let info =
        libproc::libproc::proc_pid::pidinfo::<libproc::libproc::bsd_info::BSDInfo>(pid as i32, 0)
            .map_err(anyhow::Error::msg)?;
    Ok(info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}
#[cfg(windows)]
fn started(pid: u32) -> Result<u64> {
    windows::creation(pid)
}
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn started(_pid: u32) -> Result<u64> {
    bail!("Process inspection is unsupported on this platform")
}
