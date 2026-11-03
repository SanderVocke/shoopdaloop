pub mod control;
pub mod polling;
pub mod udp;

use anyhow::{ensure, Result};
use control::{Algorithm, Reply, Request};
use shoop_dsp_link::{Descriptor, Latency};
use shoop_remote_dsp::Config;
use std::{
    net::{SocketAddr, TcpStream, UdpSocket},
    path::Path,
    time::Duration,
};

pub struct Session {
    tcp: TcpStream,
    udp: UdpSocket,
    pub config: Config,
    pub epoch: u64,
    pub algorithm_latency: Option<u32>,
    pub client_receive_buffer: usize,
    pub client_send_buffer: usize,
    pub server_receive_buffer: usize,
    pub server_send_buffer: usize,
}
impl Session {
    pub fn connect(
        endpoint: SocketAddr,
        credential: &Path,
        config: Config,
        algorithm: Algorithm,
    ) -> Result<Self> {
        config.validate()?;
        let token = control::token(credential)?;
        let tcp = TcpStream::connect_timeout(&endpoint, Duration::from_secs(5))?;
        let mut tcp =
            shoop_transport_socket::setup::authenticate(tcp, token, false, Duration::from_secs(2))?;
        let udp = UdpSocket::bind(SocketAddr::new(tcp.local_addr()?.ip(), 0))?;
        let sock = socket2::SockRef::from(&udp);
        sock.set_recv_buffer_size(1_048_576)?;
        sock.set_send_buffer_size(1_048_576)?;
        let client_receive_buffer = sock.recv_buffer_size()?;
        let client_send_buffer = sock.send_buffer_size()?;
        udp.set_nonblocking(true)?;
        let mut random = [0; 8];
        getrandom::fill(&mut random).map_err(anyhow::Error::msg)?;
        let epoch = u64::from_le_bytes(random).max(1);
        let ready = control::call(
            &mut tcp,
            &Request::Configure {
                version: control::VERSION,
                config,
                epoch,
                udp_port: udp.local_addr()?.port(),
                algorithm,
            },
        )?;
        let Reply::Ready {
            version,
            udp_port,
            latency_frames,
            receive_buffer,
            send_buffer,
        } = ready
        else {
            anyhow::bail!("expected stream readiness");
        };
        ensure!(version == control::VERSION, "wrong control version");
        udp.connect(SocketAddr::new(endpoint.ip(), udp_port))?;
        Ok(Self {
            tcp,
            udp,
            config,
            epoch,
            algorithm_latency: latency_frames,
            client_receive_buffer,
            client_send_buffer,
            server_receive_buffer: receive_buffer,
            server_send_buffer: send_buffer,
        })
    }
    pub fn parts_mut(&mut self) -> (&mut TcpStream, &UdpSocket) {
        (&mut self.tcp, &self.udp)
    }
    pub fn call(&mut self, request: &Request) -> Result<Reply> {
        control::call(&mut self.tcp, request)
    }
    pub fn start(&mut self) -> Result<()> {
        ensure!(
            matches!(self.call(&Request::Start)?, Reply::Ok),
            "expected start acknowledgement"
        );
        Ok(())
    }
    pub fn pause(&mut self) -> Result<()> {
        ensure!(
            matches!(self.call(&Request::Pause)?, Reply::Ok),
            "expected pause acknowledgement"
        );
        Ok(())
    }
    pub fn save(&mut self) -> Result<String> {
        match self.call(&Request::Save)? {
            Reply::State { state } => Ok(state),
            _ => anyhow::bail!("expected processor state"),
        }
    }
    pub fn descriptor(
        &self,
        max_callback: u16,
        latency: Option<u32>,
        slots: u16,
    ) -> Result<Descriptor> {
        let c = self.config;
        let mut d = Descriptor {
            generation: self.epoch,
            start_frame: c.start_frame,
            sample_rate: c.sample_rate,
            channels: c.channels,
            quantum: c.packet_frames,
            dsp_frames: u16::try_from(c.dsp().frames)?,
            max_callback,
            service_allowance: u32::from(c.allowance) * u32::from(c.packet_frames),
            slots,
            result_budget: slots,
            midi_capacity: c.midi_per_packet,
            algorithm_latency: self
                .algorithm_latency
                .map_or(Latency::Unknown, Latency::Known),
            ..Descriptor::default()
        };
        d.latency = latency.unwrap_or(d.minimum_latency()?);
        Ok(d.validate()?)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.tcp.shutdown(std::net::Shutdown::Both);
    }
}
