use super::*;
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

pub(super) fn sockets(snapshot: &mut Snapshot) {
    for process in &mut snapshot.processes {
        let result = (|| -> Result<()> {
            let pid = process.identity.pid;
            let mut inodes = HashSet::new();
            for fd in std::fs::read_dir(format!("/proc/{pid}/fd"))? {
                let fd = fd?;
                match std::fs::read_link(fd.path()) {
                    Ok(link) => {
                        if let Some(inode) = link
                            .to_str()
                            .and_then(|s| s.strip_prefix("socket:["))
                            .and_then(|s| s.strip_suffix(']'))
                        {
                            inodes.insert(inode.parse::<u64>()?);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            for (name, ipv6) in [("tcp", false), ("tcp6", true)] {
                let table = match std::fs::read_to_string(format!("/proc/{pid}/net/{name}")) {
                    Ok(value) => value,
                    Err(error) if ipv6 && error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                for line in table.lines().skip(1) {
                    if let Some((inode, address)) = row(line, ipv6)? {
                        if inodes.contains(&inode) {
                            process.addresses.push(address);
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            snapshot.errors.push(format!(
                "PID {} socket inspection: {error:#}",
                process.identity.pid
            ));
        }
    }
}
fn row(line: &str, ipv6: bool) -> Result<Option<(u64, SocketAddr)>> {
    let fields: Vec<_> = line.split_whitespace().collect();
    ensure!(fields.len() >= 10, "Invalid TCP socket row");
    if fields[3] != "0A" {
        return Ok(None);
    }
    let (host, port) = fields[1]
        .split_once(':')
        .context("Invalid socket address")?;
    let ip = if ipv6 {
        ensure!(
            host.len() == 32 && host.is_ascii(),
            "Invalid IPv6 socket address"
        );
        let mut bytes = [0; 16];
        for (word, destination) in host
            .as_bytes()
            .chunks_exact(8)
            .zip(bytes.chunks_exact_mut(4))
        {
            destination.copy_from_slice(
                &u32::from_str_radix(std::str::from_utf8(word)?, 16)?.to_ne_bytes(),
            );
        }
        IpAddr::V6(Ipv6Addr::from(bytes))
    } else {
        IpAddr::V4(Ipv4Addr::from(u32::from_str_radix(host, 16)?.to_ne_bytes()))
    };
    Ok(Some((
        fields[9].parse()?,
        SocketAddr::new(ip, u16::from_str_radix(port, 16)?),
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn proc_rows_preserve_wide_inodes_and_filter_non_listeners() {
        let fields = "0: 0100007F:1234 00000000:0000 0A 0:0 00:0 0 1000 0 4294967299";
        assert_eq!(
            row(fields, false).unwrap(),
            Some((4294967299, "127.0.0.1:4660".parse().unwrap()))
        );
        assert!(row(&fields.replace(" 0A ", " 01 "), false)
            .unwrap()
            .is_none());
        assert!(row("invalid", false).is_err());
        let ipv6 = fields.replace("0100007F", "00000000000000000000000001000000");
        assert_eq!(
            row(&ipv6, true).unwrap().unwrap().1,
            "[::1]:4660".parse().unwrap()
        );
    }
}
