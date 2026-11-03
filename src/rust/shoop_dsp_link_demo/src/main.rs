use anyhow::{ensure, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint as Port, GraphDescription, PortType},
    planner::NoPlanner,
    resource::ResourceStore,
    Config, NodeId,
};
use serde_json::{json, Value};
use shoop_dsp_link::{memory, service::ReferenceService, Descriptor, Latency};
use shoop_dsp_link_native::{
    control::{Algorithm, Reply, Request},
    polling::Polling,
    udp::{Adapter, Worker},
    Session,
};
use shoop_dsp_link_node::{Observation, Quality, RemoteNode, Stats};
use std::{
    net::SocketAddr,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(about = "A real rt_runtime graph through a portable DSP processing link")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    GenerateCredential { path: PathBuf },
    Run(Options),
    Serve(shoop_dsp_host::server::Options),
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Backend {
    Reference,
    Udp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Mode {
    Stepped,
    Paced,
}
#[derive(Args)]
struct Options {
    #[arg(long, value_enum, default_value = "udp")]
    backend: Backend,
    #[arg(long, value_enum, default_value = "stepped")]
    mode: Mode,
    #[arg(long)]
    endpoint: Option<SocketAddr>,
    #[arg(long)]
    credential_file: Option<PathBuf>,
    #[arg(long, default_value_t = 48_000)]
    sample_rate: u32,
    #[arg(long, default_value_t = 2)]
    channels: u16,
    #[arg(long, default_value_t = 64)]
    callback_frames: u16,
    #[arg(long, default_value_t = 4096)]
    frames: usize,
    #[arg(long, default_value_t = 3)]
    quantum: u16,
    #[arg(long, default_value_t = 5)]
    dsp_frames: u16,
    #[arg(long, default_value_t = 16)]
    packet_frames: u16,
    #[arg(long, default_value_t = 4)]
    multiplier: u16,
    #[arg(long, default_value_t = 64)]
    allowance: u16,
    #[arg(long, default_value_t = 1500)]
    mtu: u16,
    #[arg(long, default_value_t = 4)]
    midi_capacity: u16,
    #[arg(long)]
    latency_frames: Option<u32>,
    #[arg(long, default_value_t = 256)]
    slots: u16,
    #[arg(long, default_value_t = 35)]
    delay: u32,
    #[arg(long, default_value = "delay-copy", value_parser = ["delay-copy", "carla"])]
    processor: String,
    #[arg(long)]
    state_file: Option<PathBuf>,
    #[arg(long)]
    state_output: Option<PathBuf>,
    #[arg(long)]
    expected_gain: Option<f32>,
    #[arg(long, default_value_t = 0)]
    warmup_frames: usize,
    #[arg(long, value_enum, default_value = "yield")]
    polling: Polling,
    #[arg(long)]
    output: Option<PathBuf>,
}
enum Driver {
    Reference {
        service: ReferenceService,
        dsp: shoop_dsp::DelayCopy,
    },
    Udp(Worker),
}
impl Driver {
    fn service(&mut self, frame: u64, d: Descriptor, mode: Mode) -> Result<()> {
        match self {
            Self::Reference { service, dsp } => {
                service.poll(dsp)?;
            }
            Self::Udp(worker) if mode == Mode::Stepped => {
                worker.wait_until(
                    frame / u64::from(d.dsp_frames) * u64::from(d.dsp_frames),
                    Duration::from_secs(5),
                )?;
            }
            _ => {}
        }
        Ok(())
    }
    fn finish(self, missing: &mut [bool], state_output: Option<&PathBuf>) -> Result<Value> {
        match self {
            Self::Reference { service, .. } => {
                let s = service.stats();
                Ok(
                    json!({"dsp_blocks":s.dsp_blocks,"concealed_frames":s.concealed_frames,
                    "output_dropped":service.dropped_results}),
                )
            }
            Self::Udp(worker) => {
                let mut adapter = worker.stop()?;
                let native = adapter.stats();
                adapter.session.pause()?;
                let Reply::Status {
                    stats, concealment, ..
                } = adapter.session.call(&Request::Stats)?
                else {
                    anyhow::bail!("expected server statistics");
                };
                let c = adapter.session.config;
                for (sequence, mask) in &concealment {
                    for j in 0..c.multiplier {
                        if mask & (1u64 << j) != 0 {
                            let begin = (sequence + u64::from(j)) * u64::from(c.packet_frames);
                            let end = begin + u64::from(c.packet_frames);
                            for frame in begin..end.min(missing.len() as u64) {
                                missing[frame as usize] = true;
                            }
                        }
                    }
                }
                if let Some(path) = state_output {
                    std::fs::write(path, adapter.session.save()?)?;
                }
                Ok(
                    json!({"adapter":native,"server":stats,"oracle_complete":!stats.diagnostics_truncated}),
                )
            }
        }
    }
}
fn signal(frame: u64, channel: usize) -> f32 {
    (((frame * 17 + channel as u64 * 4093 + 1) % 65521) + 1) as f32 / 65536.
}
fn run(o: Options) -> Result<()> {
    ensure!(
        o.frames > 0 && o.frames <= 1_000_000 && o.callback_frames != 0,
        "invalid demo interval"
    );
    ensure!(
        o.expected_gain.is_none_or(f32::is_finite),
        "invalid expected gain"
    );
    let session = match o.backend {
        Backend::Reference => {
            ensure!(
                o.processor == "delay-copy",
                "reference backend uses direct delay-copy"
            );
            None
        }
        Backend::Udp => {
            let c = shoop_remote_dsp::Config {
                sample_rate: o.sample_rate,
                channels: o.channels,
                packet_frames: o.packet_frames,
                multiplier: o.multiplier,
                allowance: o.allowance,
                mtu: o.mtu,
                midi_per_packet: o.midi_capacity,
                reorder_packets: 64,
                ..Default::default()
            };
            let algorithm = if o.processor == "carla" {
                Algorithm::Carla {
                    patchbay16: o.channels == 16,
                    state: o
                        .state_file
                        .as_ref()
                        .map(std::fs::read_to_string)
                        .transpose()?,
                }
            } else {
                Algorithm::DelayCopy { delay: o.delay }
            };
            Some(Session::connect(
                o.endpoint
                    .ok_or_else(|| anyhow::anyhow!("UDP requires --endpoint"))?,
                o.credential_file
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("UDP requires --credential-file"))?,
                c,
                algorithm,
            )?)
        }
    };
    let d = if let Some(s) = &session {
        s.descriptor(o.callback_frames, o.latency_frames, o.slots)?
    } else {
        let mut d = Descriptor {
            sample_rate: o.sample_rate,
            channels: o.channels,
            quantum: o.quantum,
            dsp_frames: o.dsp_frames,
            max_callback: o.callback_frames,
            slots: o.slots,
            result_budget: o.slots,
            midi_capacity: o.midi_capacity,
            service_allowance: 0,
            algorithm_latency: Latency::Known(o.delay),
            ..Default::default()
        };
        d.latency = o.latency_frames.unwrap_or(d.minimum_latency()?);
        d.validate()?
    };
    ensure!(
        o.frames
            > d.latency as usize
                + d.quantum as usize
                + if o.processor == "delay-copy" {
                    o.delay as usize
                } else {
                    0
                },
        "demo interval must exceed the link and algorithm delays"
    );
    ensure!(
        o.warmup_frames
            < o.frames
                .saturating_sub(d.latency as usize + d.quantum as usize),
        "verification warm-up leaves no measurable output"
    );
    let (endpoint, peer) = memory::pair(d)?;
    let midi_ports = usize::from(d.midi_capacity != 0);
    let config = Config {
        sample_rate: d.sample_rate,
        max_frames: d.max_callback as usize,
        audio_inputs: d.channels as usize,
        audio_outputs: d.channels as usize,
        midi_inputs: midi_ports,
        midi_outputs: midi_ports,
        max_nodes: 1,
        max_ports: 128,
        max_edges: 64,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        midi_events_per_port: 256,
        midi_bytes_per_port: 4096,
        ..Default::default()
    }
    .prepare()?;
    let id = NodeId::from_parts(0, 1)?;
    let (node, mut observer) = RemoteNode::prepare_observed(endpoint, config)?;
    let mut edges = Vec::new();
    for kind in [PortType::Audio, PortType::Midi] {
        let count = if kind == PortType::Audio {
            d.channels as usize
        } else {
            midi_ports
        };
        for p in 0..count {
            edges.push(Connection {
                source: Port::host(kind, p as u32),
                destination: Port::node(id, kind, p as u32),
            });
            edges.push(Connection {
                source: Port::node(id, kind, p as u32),
                destination: Port::host(kind, p as u32),
            });
        }
    }
    let (control, mut graph) = GraphRuntime::<_, NoPlanner, ()>::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(id, node)],
        NoPlanner,
        ResourceStore::new(config)?,
        &GraphDescription {
            nodes: vec![id],
            connections: edges,
        },
        &[],
    )?;
    let mut driver = if let Some(session) = session {
        Driver::Udp(Worker::spawn(Adapter::prepare(session, peer)?, o.polling)?)
    } else {
        Driver::Reference {
            service: ReferenceService::prepare(peer)?,
            dsp: shoop_dsp::DelayCopy::prepare(d.dsp_config(), o.delay)?,
        }
    };
    let mut input = vec![vec![0.; d.max_callback as usize]; d.channels as usize];
    let mut output = input.clone();
    let mut captured = vec![0.; o.frames * d.channels as usize];
    let mut quality = vec![None; o.frames.div_ceil(d.quantum as usize)];
    let mut missing = vec![false; o.frames];
    let mut midi_in = (0..midi_ports)
        .map(|_| MidiBuffer::new(256, 4096))
        .collect::<Result<Vec<_>, _>>()?;
    let mut midi_out = (0..midi_ports)
        .map(|_| MidiBuffer::new(256, 4096))
        .collect::<Result<Vec<_>, _>>()?;
    let mut recovery = (0..midi_ports)
        .map(|_| MidiBuffer::new(64, 256))
        .collect::<Result<Vec<_>, _>>()?;
    let mut stats = Stats::default();
    let mut midi_events = 0usize;
    let origin = Instant::now();
    while graph.next_frame().0 < o.frames as u64 {
        let frame = graph.next_frame().0 as usize;
        let n = (d.max_callback as usize).min(o.frames - frame);
        for (c, audio) in input.iter_mut().enumerate() {
            for (i, sample) in audio[..n].iter_mut().enumerate() {
                *sample = signal((frame + i) as u64, c);
            }
        }
        for port in &mut midi_in {
            port.clear();
        }
        for port in &mut midi_out {
            port.clear();
        }
        if let Some(port) = midi_in.first_mut() {
            for i in 0..n {
                let at = (frame + i) % 97;
                if at == 0 {
                    port.push(i as u32, n, &[0x90, 60, 100])?;
                } else if at == 48 {
                    port.push(i as u32, n, &[0x80, 60, 0])?;
                }
            }
        }
        let mut ins: [&[f32]; 16] = [&[]; 16];
        for (view, data) in ins.iter_mut().zip(&input) {
            *view = &data[..n];
        }
        let mut outs: [&mut [f32]; 16] = std::array::from_fn(|_| Default::default());
        for (view, data) in outs.iter_mut().zip(&mut output) {
            *view = &mut data[..n];
        }
        graph.process(
            AudioBlock {
                frame: graph.next_frame(),
                frames: n,
                inputs: &ins[..d.channels as usize],
                outputs: &mut outs[..d.channels as usize],
            },
            MidiBlock {
                inputs: &midi_in,
                outputs: &mut midi_out,
            },
        )?;
        for c in 0..d.channels as usize {
            captured[c * o.frames + frame..c * o.frames + frame + n]
                .copy_from_slice(&output[c][..n]);
        }
        for port in &midi_out {
            midi_events += port.len();
        }
        for (index, port) in recovery.iter_mut().enumerate() {
            port.clear();
            midi_events += graph.drain_midi_recovery(index, port, n)?;
        }
        while let Some(record) = observer.receive() {
            match record {
                Observation::Delivery(delivery) => {
                    quality[(delivery.frame / u64::from(d.quantum)) as usize] =
                        Some(delivery.quality)
                }
                Observation::InputDropped(begin) => {
                    for i in
                        begin as usize..(begin as usize + d.quantum as usize).min(missing.len())
                    {
                        missing[i] = true;
                    }
                }
                Observation::Stats(s) => stats = s,
            }
        }
        driver.service(graph.next_frame().0, d, o.mode)?;
        if o.mode == Mode::Paced {
            let deadline =
                Duration::from_secs_f64(graph.next_frame().0 as f64 / d.sample_rate as f64);
            if let Some(wait) = deadline.checked_sub(origin.elapsed()) {
                thread::sleep(wait);
            }
        }
    }
    drop(graph);
    drop(control);
    let service = driver.finish(&mut missing, o.state_output.as_ref())?;
    let complete = service
        .get("oracle_complete")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let oracle = o.processor == "delay-copy" || o.expected_gain.is_some();
    let gain = o.expected_gain.unwrap_or(1.);
    let delay = if o.processor == "delay-copy" {
        o.delay as usize
    } else {
        0
    };
    let mut errors = 0usize;
    let mut verified = 0usize;
    let mut unverified = 0usize;
    let mut fallback = 0usize;
    let mut warmup = 0usize;
    let mut mismatch_examples = Vec::new();
    for frame in 0..o.frames {
        if frame < d.latency as usize {
            for c in 0..d.channels as usize {
                errors += usize::from(captured[c * o.frames + frame] != 0.);
            }
            continue;
        }
        let source = frame - d.latency as usize;
        let q = quality[source / d.quantum as usize]
            .ok_or_else(|| anyhow::anyhow!("missing delivery observation"))?;
        for c in 0..d.channels as usize {
            let actual = captured[c * o.frames + frame];
            if q == Quality::Silence {
                fallback += 1;
                errors += usize::from(actual != 0.);
            } else if source < o.warmup_frames {
                warmup += 1;
            } else if oracle && complete {
                let expected = if source < delay || missing[source - delay] {
                    0.
                } else {
                    signal((source - delay) as u64, c) * gain
                };
                let wrong = if o.processor == "delay-copy" {
                    actual.to_bits() != expected.to_bits()
                } else {
                    (actual - expected).abs() > 0.0001 || !actual.is_finite()
                };
                errors += usize::from(wrong);
                if wrong && mismatch_examples.len() < 8 {
                    mismatch_examples.push(json!({"frame":frame,"source":source,"channel":c,"expected":expected,"actual":actual}));
                }
                verified += 1;
            } else {
                unverified += 1;
            }
        }
    }
    let report = json!({"backend":format!("{:?}",o.backend).to_lowercase(),"mode":format!("{:?}",o.mode).to_lowercase(),
        "source_frames":o.frames,"sample_rate":d.sample_rate,"channels":d.channels,"callback_frames":d.max_callback,
        "quantum":d.quantum,"dsp_frames":d.dsp_frames,"link_latency_frames":d.latency,
        "algorithm_latency_frames":match d.algorithm_latency {Latency::Known(n)=>Some(n),Latency::Unknown=>None},
        "total_known_latency_frames":d.total_latency(),"correctness_errors":errors,"verified_samples":verified,
        "unverified_samples":unverified,"fallback_samples":fallback,"warmup_samples":warmup,
        "warmup_frames":o.warmup_frames,"midi_output_events":midi_events,
        "node":{"submitted":stats.submitted,"input_dropped":stats.input_dropped,"received":stats.received,
            "rejected":stats.rejected,"processed":stats.processed,"concealed":stats.concealed,
            "fallback":stats.fallback,"midi_dropped":stats.midi_dropped,"midi_resets":stats.midi_resets,
            "retired":stats.retired,"observation_dropped":stats.observation_dropped},"service":service,
        "mismatch_examples":mismatch_examples,
        "timing_claim":if o.mode==Mode::Stepped {"simulated-time correctness; service waits occur outside render"} else {"software-paced demonstration; no real-time guarantee"}});
    if let Some(path) = o.output {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    println!("{report}");
    ensure!(
        errors == 0 && stats.observation_dropped == 0,
        "graph sample verification failed"
    );
    ensure!(stats.processed > 0, "demo received no processed output");
    Ok(())
}
fn main() -> Result<()> {
    match Cli::parse().command {
        Command::GenerateCredential { path } => shoop_dsp_link_native::create_credential(&path),
        Command::Run(o) => run(o),
        Command::Serve(o) => shoop_dsp_host::server::run(o),
    }
}
