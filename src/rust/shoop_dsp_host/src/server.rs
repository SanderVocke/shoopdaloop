use crate::control::{self, Algorithm, Reply, Request};
use anyhow::{ensure, Result};
use clap::Args;
use shoop_dsp::{DelayCopy, Latency, Processor};
use shoop_remote_dsp::{stream::Stream, Config};
use std::{
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = "127.0.0.1:0")]
    bind: SocketAddr,
    #[arg(long)]
    credential_file: PathBuf,
    #[arg(long,default_value="delay-copy",value_parser=["delay-copy","carla"])]
    processor: String,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    lan: bool,
    #[arg(long, default_value_t = 1_048_576)]
    socket_buffer: usize,
}
struct Work {
    dsp: Box<dyn Processor>,
    core: Stream,
    udp: UdpSocket,
    config: Config,
    peer: SocketAddr,
    elapsed_ns: u64,
}
struct Worker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Work>,
}
#[derive(Default)]
struct Ui {
    #[cfg(feature = "carla")]
    service: Option<shoop_carla_native::CarlaMainThreadUiService>,
    #[cfg(feature = "carla")]
    dispatcher: Option<shoop_carla_native::CarlaMainThreadUiDispatcher>,
}
impl Ui {
    fn pump(&mut self) {
        #[cfg(feature = "carla")]
        if let Some(service) = self.service.as_mut() {
            service.pump();
        }
    }
    fn prepare(&mut self, config: Config, algorithm: Algorithm) -> Result<Box<dyn Processor>> {
        match algorithm {
            Algorithm::DelayCopy { delay } => {
                Ok(Box::new(DelayCopy::prepare(config.dsp(), delay)?))
            }
            Algorithm::Carla { patchbay16, state } => {
                #[cfg(feature = "carla")]
                {
                    use shoop_carla_native::{
                        api::ChainType, dsp::CarlaDsp, CarlaMainThreadUiService,
                    };
                    if self.service.is_none() {
                        let (service, dispatcher) = CarlaMainThreadUiService::new();
                        self.service = Some(service);
                        self.dispatcher = Some(dispatcher);
                    }
                    ensure!(
                        config.multiplier as usize * config.midi_per_packet as usize + 32
                            <= shoop_carla_native::CARLA_MIDI_BUFFER_CAPACITY,
                        "Carla MIDI capacity must include recovery events"
                    );
                    let kind = if patchbay16 {
                        ChainType::CarlaPatchbay16x
                    } else {
                        ChainType::CarlaRack
                    };
                    let processor = CarlaDsp::prepare(
                        config.dsp(),
                        kind,
                        state.as_deref(),
                        self.dispatcher.clone(),
                    )?;
                    self.pump();
                    Ok(Box::new(processor))
                }
                #[cfg(not(feature = "carla"))]
                {
                    let _ = (patchbay16, state);
                    anyhow::bail!("build with --features carla to host Carla");
                }
            }
        }
    }
}
fn start(mut work: Work) -> Result<Worker> {
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let mut raw = vec![0u8; work.config.max_packet_bytes() + 1];
    let handle = thread::Builder::new()
        .name("network-dsp".into())
        .spawn(move || {
            let origin = Instant::now();
            let elapsed_base = work.elapsed_ns;
            let mut last_input = Instant::now();
            while !stopping.load(Ordering::Acquire) {
                if work.core.stats().retired {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                let mut progress = false;
                match work.udp.recv_from(&mut raw) {
                    Ok((len, peer)) if peer == work.peer => {
                        let _ = work.core.accept(
                            &raw[..len],
                            elapsed_base.saturating_add(origin.elapsed().as_nanos() as u64),
                        );
                        last_input = Instant::now();
                        progress = true;
                    }
                    Ok(_) => {}
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => work.core.retire_with(shoop_remote_dsp::Error::Io),
                }
                for _ in 0..8 {
                    let now = elapsed_base.saturating_add(origin.elapsed().as_nanos() as u64);
                    let udp = &work.udp;
                    let peer = work.peer;
                    match work.core.process_one(
                        now,
                        &mut *work.dsp,
                        || elapsed_base.saturating_add(origin.elapsed().as_nanos() as u64),
                        |packet| {
                            udp.send_to(packet, peer)
                                .map(|_| ())
                                .map_err(|_| shoop_remote_dsp::Error::Io)
                        },
                    ) {
                        Ok(true) => progress = true,
                        _ => break,
                    }
                }
                if last_input.elapsed() > Duration::from_secs(2) {
                    work.core.retire();
                }
                if !progress {
                    std::hint::spin_loop();
                    thread::yield_now();
                }
            }
            work.elapsed_ns = elapsed_base.saturating_add(origin.elapsed().as_nanos() as u64);
            work
        })?;
    Ok(Worker { stop, handle })
}
fn pause(worker: Worker, ui: &mut Ui) -> Result<Work> {
    worker.stop.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.handle.is_finished() {
        ui.pump();
        if Instant::now() >= deadline {
            eprintln!("DSP failed to stop within 5 seconds; terminating its dedicated process");
            std::process::exit(1);
        }
        thread::sleep(Duration::from_micros(100));
    }
    worker
        .handle
        .join()
        .map_err(|_| anyhow::anyhow!("DSP worker panicked"))
}
fn session(mut tcp: TcpStream, options: &Options, token: [u8; 32], ui: &mut Ui) -> Result<()> {
    tcp = shoop_transport_socket::setup::authenticate(tcp, token, true, Duration::from_secs(2))?;
    let peer = tcp.peer_addr()?;
    let mut parked: Option<Work> = None;
    let mut running: Option<Worker> = None;
    let mut used_epochs = std::collections::BTreeSet::new();
    let result = (|| -> Result<()> {
        loop {
            let request: Request = control::read(&mut tcp, Duration::from_secs(120), || ui.pump())?;
            let stop = matches!(request, Request::Stop);
            let reply = (|| -> Result<Reply> {
                Ok(match request {
                    Request::Configure {
                        version,
                        config,
                        epoch,
                        udp_port,
                        algorithm,
                    } => {
                        ensure!(running.is_none(), "pause before configuring");
                        ensure!(
                            version == control::VERSION
                                && epoch != 0
                                && !used_epochs.contains(&epoch)
                                && udp_port != 0,
                            "invalid version or stream identity"
                        );
                        ensure!(used_epochs.len() < 1024, "connection stream limit reached");
                        ensure!(
                            matches!(
                                (&*options.processor, &algorithm),
                                ("delay-copy", Algorithm::DelayCopy { .. })
                                    | ("carla", Algorithm::Carla { .. })
                            ),
                            "processor does not match endpoint"
                        );
                        config.validate()?;
                        let dsp = ui.prepare(config, algorithm)?;
                        let latency = match dsp.info().latency {
                            Latency::Known(frames) => Some(frames),
                            Latency::Unknown => None,
                        };
                        let udp = UdpSocket::bind(SocketAddr::new(tcp.local_addr()?.ip(), 0))?;
                        let sock = socket2::SockRef::from(&udp);
                        sock.set_recv_buffer_size(options.socket_buffer)?;
                        sock.set_send_buffer_size(options.socket_buffer)?;
                        let receive_buffer = sock.recv_buffer_size()?;
                        let send_buffer = sock.send_buffer_size()?;
                        udp.set_nonblocking(true)?;
                        let udp_port_reply = udp.local_addr()?.port();
                        let core = Stream::prepare(config, epoch)?;
                        parked = Some(Work {
                            dsp,
                            core,
                            udp,
                            config,
                            peer: SocketAddr::new(peer.ip(), udp_port),
                            elapsed_ns: 0,
                        });
                        used_epochs.insert(epoch);
                        ui.pump();
                        Reply::Ready {
                            version: control::VERSION,
                            udp_port: udp_port_reply,
                            latency_frames: latency,
                            receive_buffer,
                            send_buffer,
                        }
                    }
                    Request::Start => {
                        ensure!(running.is_none(), "already running");
                        ensure!(
                            parked.as_ref().is_some_and(|w| !w.core.stats().retired),
                            "configure a fresh stream after retirement"
                        );
                        running = Some(start(
                            parked
                                .take()
                                .ok_or_else(|| anyhow::anyhow!("configure first"))?,
                        )?);
                        Reply::Ok
                    }
                    Request::Pause => {
                        if let Some(worker) = running.take() {
                            parked = Some(pause(worker, ui)?);
                        }
                        Reply::Ok
                    }
                    Request::Stats => {
                        let w = parked.as_ref().ok_or_else(|| {
                            anyhow::anyhow!("pause before requesting stream statistics")
                        })?;
                        Reply::Status {
                            stats: w.core.stats(),
                            concealment: w.core.concealment().to_vec(),
                            next_sequence: w.core.next_sequence(),
                        }
                    }
                    Request::Save => {
                        let w = parked
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("pause before state operations"))?;
                        Reply::State {
                            state: String::from_utf8(w.dsp.save_state()?)?,
                        }
                    }
                    Request::Restore { state } => {
                        let w = parked
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("configure a paused stream first"))?;
                        ensure!(
                            w.core.next_sequence() == 0,
                            "restore requires a new stream epoch"
                        );
                        w.dsp.restore_state(state.as_bytes())?;
                        Reply::Ok
                    }
                    Request::Show { visible } => {
                        parked
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("pause before changing UI visibility"))?
                            .dsp
                            .set_visible(visible)?;
                        Reply::Ok
                    }
                    Request::Ping | Request::Stop => Reply::Ok,
                })
            })()
            .unwrap_or_else(|error| Reply::Error {
                error: format!("{error:#}"),
            });
            control::write(&mut tcp, &reply)?;
            if stop {
                break;
            }
        }
        Ok(())
    })();
    if let Some(worker) = running.take() {
        parked = Some(pause(worker, ui)?);
    }
    drop(parked);
    ui.pump();
    result
}
pub fn run(options: Options) -> Result<()> {
    ensure!(
        options.lan || options.bind.ip().is_loopback(),
        "non-loopback listener requires --lan"
    );
    let token = control::token(&options.credential_file)?;
    let listener = TcpListener::bind(options.bind)?;
    listener.set_nonblocking(true)?;
    println!(
        "{}",
        serde_json::json!({"control_endpoint":listener.local_addr()?.to_string(),"version":control::VERSION})
    );
    let mut ui = Ui::default();
    loop {
        ui.pump();
        match listener.accept() {
            Ok((tcp, _)) => {
                if let Err(error) = session(tcp, &options, token, &mut ui) {
                    eprintln!("DSP connection ended: {error:#}");
                }
                if options.once {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
