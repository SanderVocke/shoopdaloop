use crate::audio::{self, Audio, InputPolicy, Transport};
use crate::control::{self, Algorithm, Reply, Request};
use crate::polling::{Polling, ServiceGaps};
use anyhow::{ensure, Result};
use clap::Args;
use shoop_dsp::{DelayCopy, Latency, Processor};
use shoop_remote_dsp::{stream::Stream, Config};
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
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
    #[arg(long, default_value_t = 0)]
    data_port: u16,
    #[arg(long, value_enum, default_value = "yield")]
    polling: Polling,
}
struct Work {
    dsp: Box<dyn Processor>,
    core: Stream,
    audio: Audio,
    config: Config,
    input_policy: InputPolicy,
    worker_runtime: serde_json::Value,
    elapsed_ns: u64,
    service_gaps: ServiceGaps,
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
fn start(mut work: Work, polling: Polling) -> Result<Worker> {
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let mut raw = vec![0u8; work.config.max_packet_bytes() + 1];
    let handle = thread::Builder::new()
        .name("network-dsp".into())
        .spawn(move || {
            work.worker_runtime = audio::runtime_info();
            let origin = Instant::now();
            let elapsed_base = work.elapsed_ns;
            let mut last_input = Instant::now();
            let mut last_service = 0;
            while !stopping.load(Ordering::Acquire) {
                if work.core.stats().retired {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                let serviced = origin.elapsed().as_nanos() as u64;
                work.service_gaps.observe(serviced - last_service);
                last_service = serviced;
                let mut progress = false;
                if work.audio.flush().is_err() {
                    work.core.retire_with(shoop_remote_dsp::Error::Io);
                    continue;
                }
                match work.audio.receive(&mut raw) {
                    Ok(len) => {
                        let _ = work.core.accept(
                            &raw[..len],
                            elapsed_base.saturating_add(origin.elapsed().as_nanos() as u64),
                        );
                        last_input = Instant::now();
                        progress = true;
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => work.core.retire_with(shoop_remote_dsp::Error::Io),
                }
                for _ in 0..8 {
                    let now = elapsed_base.saturating_add(origin.elapsed().as_nanos() as u64);
                    let audio = &mut work.audio;
                    let clock = || elapsed_base.saturating_add(origin.elapsed().as_nanos() as u64);
                    let emit =
                        |packet: &[u8]| audio.send(packet).map_err(|_| shoop_remote_dsp::Error::Io);
                    let result = match work.input_policy {
                        InputPolicy::Gap => work.core.process_one(now, &mut *work.dsp, clock, emit),
                        InputPolicy::Complete => {
                            work.core
                                .process_one_complete(now, &mut *work.dsp, clock, emit)
                        }
                    };
                    if work.audio.failed() {
                        work.core.retire_with(shoop_remote_dsp::Error::Io);
                    }
                    match result {
                        Ok(true) => progress = true,
                        _ => break,
                    }
                }
                if last_input.elapsed() > Duration::from_secs(2) {
                    work.core.retire();
                }
                if !progress {
                    polling.idle();
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
                        transport,
                        input_policy,
                        algorithm,
                    } => {
                        ensure!(running.is_none(), "pause before configuring");
                        ensure!(
                            version == control::VERSION
                                && epoch != 0
                                && !used_epochs.contains(&epoch)
                                && (transport == Transport::Tcp || udp_port != 0),
                            "invalid version or stream identity"
                        );
                        ensure!(used_epochs.len() < 1024, "connection stream limit reached");
                        ensure!(
                            transport != Transport::Tcp || input_policy == InputPolicy::Complete,
                            "TCP audio requires complete-input processing"
                        );
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
                        drop(parked.take());
                        let audio = Audio::server(
                            transport,
                            SocketAddr::new(tcp.local_addr()?.ip(), options.data_port),
                            SocketAddr::new(peer.ip(), udp_port),
                            config,
                            options.socket_buffer,
                        )?;
                        let (receive_buffer, send_buffer) = audio.buffer_sizes()?;
                        let data_port = audio.address()?.port();
                        let tcp_token = audio.token();
                        let core = Stream::prepare(config, epoch)?;
                        parked = Some(Work {
                            dsp,
                            core,
                            audio,
                            config,
                            input_policy,
                            worker_runtime: serde_json::Value::Null,
                            elapsed_ns: 0,
                            service_gaps: ServiceGaps::default(),
                        });
                        used_epochs.insert(epoch);
                        ui.pump();
                        Reply::Ready {
                            version: control::VERSION,
                            udp_port: if transport == Transport::Udp {
                                data_port
                            } else {
                                0
                            },
                            transport,
                            tcp_port: (transport == Transport::Tcp).then_some(data_port),
                            tcp_token,
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
                        let mut work = parked
                            .take()
                            .ok_or_else(|| anyhow::anyhow!("configure first"))?;
                        work.audio = work.audio.activate()?;
                        running = Some(start(work, options.polling)?);
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
                            service_gaps: w.service_gaps,
                            concealment: w.core.concealment().to_vec(),
                            next_sequence: w.core.next_sequence(),
                            audio: w.audio.report(),
                            worker_runtime: w.worker_runtime.clone(),
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
