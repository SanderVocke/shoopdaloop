use super::*;
use libproc::libproc::{
    bsd_info::BSDInfo,
    file_info::{pidfdinfo, ListFDs, ProcFDType},
    net_info::{SocketFDInfo, SocketInfoKind, TcpSIState},
    proc_pid::{listpidinfo, pidinfo},
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub(super) fn sockets(snapshot: &mut Snapshot) {
    for process in &mut snapshot.processes {
        let pid = process.identity.pid as i32;
        let result = (|| -> Result<()> {
            let info = pidinfo::<BSDInfo>(pid, 0).map_err(anyhow::Error::msg)?;
            if info.pbi_nfiles == 0 {
                return Ok(());
            }
            let fds = listpidinfo::<ListFDs>(pid, info.pbi_nfiles as usize + 16)
                .map_err(anyhow::Error::msg)?;
            for fd in fds {
                if fd.proc_fdtype != ProcFDType::Socket as u32 {
                    continue;
                }
                let socket = match pidfdinfo::<SocketFDInfo>(pid, fd.proc_fd) {
                    Ok(socket) => socket.psi,
                    Err(error) => {
                        // A socket can close between descriptor enumeration and inspection.
                        let current = listpidinfo::<ListFDs>(pid, info.pbi_nfiles as usize + 16)
                            .map_err(anyhow::Error::msg)?;
                        if current.iter().any(|current| current.proc_fd == fd.proc_fd) {
                            snapshot
                                .errors
                                .push(format!("PID {pid}, FD {}: {error}", fd.proc_fd));
                        }
                        continue;
                    }
                };
                if socket.soi_kind != SocketInfoKind::Tcp as i32 {
                    continue;
                }
                // The OS socket kind selects the initialized TCP member of this union.
                let tcp = unsafe { socket.soi_proto.pri_tcp };
                if tcp.tcpsi_state != TcpSIState::Listen as i32 {
                    continue;
                }
                let address = match socket.soi_family {
                    libc::AF_INET => {
                        // AF_INET selects the initialized IPv4 address member.
                        let address = unsafe { tcp.tcpsi_ini.insi_laddr.ina_46.i46a_addr4.s_addr };
                        IpAddr::V4(Ipv4Addr::from(u32::from_be(address)))
                    }
                    libc::AF_INET6 => {
                        // AF_INET6 selects the initialized IPv6 address member.
                        let address = unsafe { tcp.tcpsi_ini.insi_laddr.ina_6.s6_addr };
                        IpAddr::V6(Ipv6Addr::from(address))
                    }
                    _ => continue,
                };
                let port = u16::from_be(tcp.tcpsi_ini.insi_lport as u16);
                process.addresses.push(SocketAddr::new(address, port));
            }
            Ok(())
        })();
        if let Err(error) = result {
            snapshot.errors.push(format!(
                "PID {pid}: socket inspection unavailable: {error:#}"
            ));
        }
    }
}
