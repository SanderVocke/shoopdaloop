//! Bounded workstation-side Fastboot UDP transport. No raw flash commands are exposed.
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use shoop_deployment_protocol as protocol;
use shoop_transport::{RecordPort, MAX_RECORD_BYTES};
use std::{
    fs::File,
    io::{ErrorKind, Read},
    net::{SocketAddr, UdpSocket},
    path::Path,
    time::{Duration, Instant},
};

/// A deployment query uses the existing authenticated listener. The greeting is
/// carrier readiness only; decoding it as the current UI protocol would prevent OTA
/// across future UI/session version changes.
#[tracing::instrument(skip_all)]
pub fn management(
    profile: &crate::remote::RemoteProfile,
    request: protocol::Request,
    timeout: Duration,
) -> Result<protocol::Reply> {
    profile.validate_deployment()?;
    let target: crate::remote::Target = profile.clone().into();
    let token = target.deployment_token()?;
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("invalid deployment timeout")?;
    let mut port = crate::connection::open(&target, &target.management, &token, deadline)?;
    let mut bytes = [0; MAX_RECORD_BYTES];
    let mut greeting = false;
    loop {
        if let Some(n) = port.take(&mut bytes) {
            if !greeting {
                // The carrier handshake already authenticated this peer. Bound and
                // consume its opaque readiness record before posting our envelope.
                ensure!(n <= 64, "invalid management readiness record");
                greeting = true;
                let n = protocol::encode_request(request, &mut bytes)
                    .map_err(|_| anyhow::anyhow!("invalid deployment request"))?;
                port.post(&bytes[..n])
                    .map_err(|_| anyhow::anyhow!("deployment request could not be queued"))?;
            } else {
                return protocol::decode_reply(&bytes[..n]).map_err(|_| {
                    anyhow::anyhow!("host does not support this deployment protocol")
                });
            }
        }
        ensure!(
            port.failure().is_none(),
            "deployment management connection lost"
        );
        ensure!(
            Instant::now() < deadline,
            "deployment management deadline exceeded"
        );
        let _ = port.flush();
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub const MAX_BUNDLE_BYTES: usize = 32 * 1024 * 1024;
const PACKET_BYTES: usize = 1024;
const RETRY: Duration = Duration::from_millis(200);

pub struct Bundle {
    pub bytes: Vec<u8>,
    pub hash: [u8; 32],
}
impl Bundle {
    #[tracing::instrument(skip_all)]
    pub fn read(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take((MAX_BUNDLE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_BUNDLE_BYTES,
            "bundle must contain 1..=32 MiB"
        );
        let hash = Sha256::digest(&bytes).into();
        Ok(Self { bytes, hash })
    }
}

/// One negotiated UDP session with a fixed overall deadline and duplicate-safe retries.
pub struct Fastboot {
    socket: UdpSocket,
    sequence: u16,
    packet_bytes: usize,
    deadline: Instant,
}
impl Fastboot {
    #[tracing::instrument(skip_all)]
    pub fn connect(peer: SocketAddr, timeout: Duration) -> Result<Self> {
        ensure!(!timeout.is_zero(), "updater deadline must be positive");
        let socket = UdpSocket::bind(if peer.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })?;
        socket.connect(peer)?;
        socket.set_read_timeout(Some(RETRY))?;
        socket.set_write_timeout(Some(RETRY))?;
        let mut this = Self {
            socket,
            sequence: 0,
            packet_bytes: PACKET_BYTES,
            deadline: Instant::now()
                .checked_add(timeout)
                .context("invalid updater timeout")?,
        };
        let query = this.exchange(1, &[])?;
        ensure!(query.len() == 2, "invalid Fastboot QUERY reply");
        this.sequence = u16::from_be_bytes([query[0], query[1]]);
        let init = this.exchange(2, &[])?;
        ensure!(
            init.len() == 4 && init[..2] == [0, 1],
            "unsupported Fastboot UDP version"
        );
        this.packet_bytes = usize::from(u16::from_be_bytes([init[2], init[3]]));
        ensure!(
            (5..=PACKET_BYTES).contains(&this.packet_bytes),
            "invalid Fastboot packet size"
        );
        Ok(this)
    }

    fn exchange(&mut self, id: u8, payload: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            payload.len() + 4 <= self.packet_bytes,
            "Fastboot packet exceeds negotiated limit"
        );
        let mut packet = vec![id, 0];
        packet.extend_from_slice(&self.sequence.to_be_bytes());
        packet.extend_from_slice(payload);
        let mut response = [0; PACKET_BYTES + 1];
        let mut retry_at = Instant::now();
        loop {
            ensure!(Instant::now() < self.deadline, "Fastboot deadline exceeded");
            if Instant::now() >= retry_at {
                match self.socket.send(&packet) {
                    Ok(n) => ensure!(n == packet.len(), "short UDP send"),
                    Err(e)
                        if matches!(
                            e.kind(),
                            ErrorKind::ConnectionRefused
                                | ErrorKind::WouldBlock
                                | ErrorKind::TimedOut
                        ) => {}
                    Err(e) => return Err(e.into()),
                }
                retry_at = Instant::now() + RETRY;
            }
            match self.socket.recv(&mut response) {
                Ok(n) => {
                    if n < 4
                        || n > self.packet_bytes
                        || response[1] != 0
                        || u16::from_be_bytes([response[2], response[3]]) != self.sequence
                    {
                        continue;
                    }
                    ensure!(response[0] != 0, "Fastboot peer reported a protocol error");
                    if response[0] != id {
                        continue;
                    }
                    if id != 1 {
                        self.sequence = self.sequence.wrapping_add(1);
                    }
                    // INFO is unsolicited progress; the final reply has the next sequence.
                    if id == 3 && response[4..n].starts_with(b"INFO") {
                        packet = vec![id, 0];
                        packet.extend_from_slice(&self.sequence.to_be_bytes());
                        retry_at = Instant::now() + RETRY;
                        continue;
                    }
                    return Ok(response[4..n].to_vec());
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::ConnectionRefused
                            | ErrorKind::WouldBlock
                            | ErrorKind::TimedOut
                            | ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn command(&mut self, command: &str) -> Result<Vec<u8>> {
        ensure!(command.len() <= 64, "Fastboot command too long");
        let acknowledgement = self.exchange(3, command.as_bytes())?;
        ensure!(
            acknowledgement.is_empty(),
            "expected Fastboot command acknowledgement"
        );
        let reply = self.exchange(3, &[])?;
        if reply.starts_with(b"FAIL") {
            bail!("Fastboot refused: {}", String::from_utf8_lossy(&reply[4..]));
        }
        Ok(reply)
    }
    fn okay(reply: &[u8]) -> Result<&[u8]> {
        reply
            .strip_prefix(b"OKAY")
            .context("expected Fastboot OKAY")
    }

    #[tracing::instrument(skip_all)]
    pub fn getvar(&mut self, name: &str) -> Result<String> {
        ensure!(
            name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid status variable"
        );
        let reply = self.command(&format!("getvar:{name}"))?;
        Ok(std::str::from_utf8(Self::okay(&reply)?)?.to_owned())
    }

    #[tracing::instrument(skip_all)]
    pub fn download(&mut self, bundle: &Bundle) -> Result<()> {
        ensure!(
            !bundle.bytes.is_empty() && bundle.bytes.len() <= MAX_BUNDLE_BYTES,
            "invalid bundle length"
        );
        let expected = format!("DATA{:08x}", bundle.bytes.len());
        ensure!(
            self.command(&format!("download:{:08x}", bundle.bytes.len()))? == expected.as_bytes(),
            "Fastboot download length mismatch"
        );
        for chunk in bundle.bytes.chunks(self.packet_bytes - 4) {
            ensure!(
                self.exchange(3, chunk)?.is_empty(),
                "unexpected download acknowledgement"
            );
        }
        Self::okay(&self.exchange(3, &[])?)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub fn install(&mut self) -> Result<()> {
        Self::okay(&self.command("oem board:install")?)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub fn reboot(&mut self) -> Result<()> {
        Self::okay(&self.command("reboot")?)?;
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    management: String,
    profile: String,
    bundle: [u8; 32],
    client: [u8; 16],
    sequence: u64,
    boot_nonce: u64,
    done: bool,
}
impl Journal {
    fn operation(&self) -> protocol::Operation {
        protocol::Operation {
            client: u128::from_le_bytes(self.client),
            sequence: self.sequence,
        }
    }
    fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.write_all(b"\n")?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    }
    fn load(path: &Path) -> Result<Option<Self>> {
        let mut bytes = Vec::new();
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        file.take(4097).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 4096, "deployment journal exceeds 4 KiB");
        let journal: Self = serde_json::from_slice(&bytes)?;
        ensure!(
            journal.version == 1 && journal.operation().valid() && journal.boot_nonce != 0,
            "invalid deployment journal"
        );
        Ok(Some(journal))
    }
}

fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .context("deployment deadline exceeded; rerun the same command to reconcile its journal")
}
fn info(reply: protocol::Reply) -> Result<protocol::Info> {
    match reply {
        protocol::Reply::Info(info) => Ok(info),
        protocol::Reply::Refused(reason) => bail!("deployment status refused: {reason:?}"),
        _ => bail!("unexpected deployment status response"),
    }
}
fn finish(journal: &mut Journal, path: &Path, info: protocol::Info) -> Result<protocol::Info> {
    ensure!(
        info.profile == journal.profile,
        "deployment profile changed"
    );
    ensure!(
        info.hash == journal.bundle && info.confirmed,
        "firmware is not durably confirmed"
    );
    journal.done = true;
    journal.save(path)?;
    Ok(info)
}

/// Authenticate, cleanly enter the updater, stage the inactive image, and await
/// durable guest confirmation. The small public journal makes lost replies and
/// interrupted workstation processes safe to reconcile on the next invocation.
#[tracing::instrument(skip_all)]
pub fn deploy(
    profile: &crate::remote::RemoteProfile,
    bundle: &Bundle,
    updater: SocketAddr,
    journal_path: &Path,
    timeout: Duration,
) -> Result<protocol::Info> {
    profile.validate_deployment()?;
    let target: crate::remote::Target = profile.clone().into();
    target.deployment_token()?;
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("invalid deployment timeout")?;
    let saved = Journal::load(journal_path)?.filter(|j| !j.done);
    let mut journal = if let Some(journal) = saved {
        ensure!(
            journal.management == profile.management && journal.bundle == bundle.hash,
            "pending journal belongs to another target or bundle; reconcile that operation first"
        );
        journal
    } else {
        let current = info(management(
            profile,
            protocol::Request::Status,
            remaining(deadline)?.min(Duration::from_secs(3)),
        )?)?;
        if current.hash == bundle.hash && current.confirmed {
            return Ok(current);
        }
        ensure!(
            !current.busy,
            "host has a live controller or shutdown owner"
        );
        let operation = crate::registry::identity()?;
        let journal = Journal {
            version: 1,
            management: profile.management.clone(),
            profile: current.profile,
            bundle: bundle.hash,
            client: operation.to_le_bytes(),
            sequence: 1,
            boot_nonce: current.boot_nonce,
            done: false,
        };
        journal.save(journal_path)?;
        journal
    };
    let operation = journal.operation();
    println!(
        "requesting clean updater entry (operation {:032x}/{})",
        operation.client, operation.sequence
    );
    let mut accepted = false;
    let mut bootloader = loop {
        let budget = remaining(deadline)?.min(Duration::from_secs(1));
        if let Ok(status) = management(profile, protocol::Request::Status, budget) {
            let current = info(status)?;
            ensure!(
                current.profile == journal.profile,
                "deployment profile changed"
            );
            if current.hash == bundle.hash && current.confirmed {
                return finish(&mut journal, journal_path, current);
            }
            if current.boot_nonce != journal.boot_nonce {
                if current.last_operation == Some(operation) && current.confirmed {
                    journal.done = true;
                    journal.save(journal_path)?;
                    bail!("candidate rolled back to {}", current.build);
                }
                bail!("host boot changed before this deployment could be reconciled");
            }
            let request = if accepted {
                protocol::Request::Result {
                    boot_nonce: journal.boot_nonce,
                    operation,
                }
            } else {
                protocol::Request::Enter {
                    boot_nonce: journal.boot_nonce,
                    operation,
                }
            };
            match management(
                profile,
                request,
                remaining(deadline)?.min(Duration::from_secs(1)),
            ) {
                Ok(protocol::Reply::Accepted(op)) if op == operation => accepted = true,
                Ok(protocol::Reply::Finished {
                    operation: op,
                    result: Ok(()),
                }) if op == operation => accepted = true,
                Ok(protocol::Reply::Finished {
                    result: Err(reason),
                    ..
                })
                | Ok(protocol::Reply::Refused(reason)) => {
                    // A known refusal is terminal for this operation; a subsequent
                    // command can request cleanup again with a fresh operation ID.
                    journal.done = true;
                    journal.save(journal_path)?;
                    bail!("updater entry refused: {reason:?}");
                }
                Ok(_) => bail!("unexpected updater-entry response"),
                Err(_) => {} // Reset may have won the race with the reply.
            }
        }
        if let Ok(mut fastboot) =
            Fastboot::connect(updater, remaining(deadline)?.min(Duration::from_secs(1)))
        {
            let expected = journal
                .client
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            ensure!(
                fastboot.getvar("shoop-client")? == expected
                    && fastboot.getvar("shoop-operation")? == journal.sequence.to_string(),
                "updater operation identity does not match the authenticated request"
            );
            ensure!(
                fastboot.getvar("shoop-profile")? == journal.profile,
                "updater target profile mismatch"
            );
            break fastboot;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    // Negotiation used a short discovery budget. The authenticated operation
    // cookie has now matched; give this session the remaining transaction budget.
    bootloader.deadline = deadline;
    println!("transferring {} bytes to inactive slot", bundle.bytes.len());
    bootloader.download(bundle)?;
    bootloader.install()?;
    println!("candidate verified; rebooting and waiting for durable health confirmation");
    let _ = bootloader.reboot(); // A lost final UDP ACK is reconciled by boot identity.
    loop {
        if let Ok(reply) = management(
            profile,
            protocol::Request::Status,
            remaining(deadline)?.min(Duration::from_secs(1)),
        ) {
            let current = info(reply)?;
            ensure!(
                current.profile == journal.profile,
                "deployment profile changed"
            );
            if current.boot_nonce > journal.boot_nonce && current.last_operation == Some(operation)
            {
                if current.confirmed && current.hash == bundle.hash {
                    return finish(&mut journal, journal_path, current);
                }
                if current.confirmed && current.hash != bundle.hash {
                    journal.done = true;
                    journal.save(journal_path)?;
                    bail!("candidate rolled back to {}", current.build);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
