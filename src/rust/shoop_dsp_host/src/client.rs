use crate::control::{self, Algorithm, Reply, Request};
use crate::polling::{Polling, ServiceGaps};
use anyhow::{ensure, Result};
use clap::Args;
use serde_json::{json, Value};
use shoop_remote_dsp::{wire, Config};
use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf, time::Instant};

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    endpoint: SocketAddr,
    #[arg(long)]
    credential_file: PathBuf,
    #[arg(long, default_value_t = 0)]
    start_frame: u64,
    #[arg(long, default_value_t = 48_000)]
    sample_rate: u32,
    #[arg(long, default_value_t = 16)]
    packet_frames: u16,
    #[arg(long, default_value_t = 4)]
    multiplier: u16,
    #[arg(long, default_value_t = 8)]
    allowance: u16,
    #[arg(long, default_value_t = 2)]
    channels: u16,
    #[arg(long, default_value_t = 1500)]
    mtu: u16,
    #[arg(long, default_value_t = 4)]
    midi_per_packet: u16,
    #[arg(long, default_value_t = 64)]
    reorder_packets: u16,
    #[arg(long, default_value_t = 200)]
    gap_timeout_us: u32,
    #[arg(long,default_value="delay-copy",value_parser=["delay-copy","carla"])]
    processor: String,
    #[arg(long, default_value_t = 0)]
    delay: u32,
    #[arg(long)]
    state_file: Option<PathBuf>,
    #[arg(long)]
    state_output: Option<PathBuf>,
    #[arg(long)]
    expected_gain: Option<f32>,
    #[arg(long, default_value_t = 2.0)]
    seconds: f64,
    #[arg(long, default_value_t = 0.5)]
    warmup: f64,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 0)]
    drop_every: u64,
    #[arg(long, default_value_t = 0)]
    duplicate_every: u64,
    #[arg(long)]
    impulse: bool,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    validate_only: bool,
    #[arg(long, value_enum, default_value = "yield")]
    polling: Polling,
}
fn signal(frame: u64, channel: usize, seed: u64, impulse: bool) -> f32 {
    if impulse {
        if frame == 0 {
            1.0
        } else {
            0.0
        }
    } else {
        (((frame
            .wrapping_mul(17)
            .wrapping_add(channel as u64 * 4093)
            .wrapping_add(seed))
            % 65521) as f32
            - 32760.0)
            / 65536.0
    }
}
fn distribution(mut values: Vec<u64>) -> Value {
    values.sort_unstable();
    let count = values.len();
    let mut histogram = BTreeMap::<u64, u64>::new();
    for value in &values {
        *histogram.entry(*value / 1000).or_default() += 1;
    }
    let percentile = |p: f64| -> Option<u64> {
        if count == 0 {
            None
        } else {
            Some(
                values[((count as f64 * p).ceil() as usize)
                    .saturating_sub(1)
                    .min(count - 1)],
            )
        }
    };
    json!({"count":count,"mean_ns":if count==0{None}else{Some(values.iter().map(|v|*v as f64).sum::<f64>()/count as f64)},
        "p50_ns":percentile(0.5),"p95_ns":percentile(0.95),"p99_ns":percentile(0.99),"p999_ns":percentile(0.999),
        "max_ns":values.last(),"histogram_us":histogram.into_iter().collect::<Vec<_>>()})
}
pub fn run(options: Options) -> Result<()> {
    let config = Config {
        start_frame: options.start_frame,
        sample_rate: options.sample_rate,
        packet_frames: options.packet_frames,
        multiplier: options.multiplier,
        allowance: options.allowance,
        channels: options.channels,
        mtu: options.mtu,
        midi_per_packet: options.midi_per_packet,
        reorder_packets: options.reorder_packets,
        gap_timeout_us: options.gap_timeout_us,
    };
    if let Err(error) = config.validate() {
        let report = json!({"status":"rejected","reason":error.to_string(),"config":config});
        if let Some(path) = &options.output {
            std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        }
        println!("{report}");
        return Ok(());
    }
    if options.validate_only {
        let report = json!({"status":"admitted","config":config});
        if let Some(path) = &options.output {
            std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        }
        println!("{report}");
        return Ok(());
    }
    ensure!(
        options.seconds.is_finite()
            && options.seconds > 0.0
            && options.seconds <= 120.0
            && options.warmup.is_finite()
            && (0.0..=10.0).contains(&options.warmup),
        "invalid duration"
    );
    ensure!(
        options.expected_gain.is_none_or(f32::is_finite),
        "invalid expected gain"
    );
    let packet_seconds = config.packet_frames as f64 / config.sample_rate as f64;
    let k = config.multiplier as usize;
    let p = config.packet_frames as usize;
    let warmup_packets = ((options.warmup / packet_seconds).ceil() as usize).div_ceil(k) * k;
    let measured_packets = ((options.seconds / packet_seconds).ceil() as usize).div_ceil(k) * k;
    let count = warmup_packets + measured_packets;
    ensure!(
        count <= 1_000_000,
        "measurement exceeds prepared packet capacity"
    );
    ensure!(
        config
            .start_frame
            .checked_add(count as u64 * p as u64)
            .is_some(),
        "source interval overflows frame range"
    );
    let state = options
        .state_file
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?;
    let algorithm = if options.processor == "carla" {
        Algorithm::Carla {
            patchbay16: config.channels == 16,
            state,
        }
    } else {
        Algorithm::DelayCopy {
            delay: options.delay,
        }
    };
    let mut session = shoop_dsp_link_native::Session::connect(
        options.endpoint,
        &options.credential_file,
        config,
        algorithm.clone(),
    )?;
    let epoch = session.epoch;
    let latency = session.algorithm_latency;
    let client_receive_buffer = session.client_receive_buffer;
    let client_send_buffer = session.client_send_buffer;
    let server_receive_buffer = session.server_receive_buffer;
    let server_send_buffer = session.server_send_buffer;
    let (mut tcp, udp) = session.parts_mut();
    let mut tx = vec![u64::MAX; count];
    let mut rx = vec![u64::MAX; count];
    let mut processing = vec![0; count];
    let mut samples = vec![0.; count * p * config.channels as usize];
    let mut input = vec![0.; p * config.channels as usize];
    let mut raw = vec![0; config.max_packet_bytes() + 1];
    let mut out = raw.clone();
    let mut returns = shoop_remote_dsp::returns::Returns::prepare(config, epoch)?;
    let mut presentation = vec![0.; input.len()];
    let mut presentation_midi = shoop_dsp::MidiBuffer::new(config.midi_per_packet as usize);
    let mut delivered = vec![false; count];
    let mut consume_next = 0usize;
    control::call(&mut tcp, &Request::Start)?;
    let origin = Instant::now();
    let mut next = 0usize;
    let mut duplicate = 0;
    let mut invalid = 0;
    let mut send_errors = 0;
    let mut deliberately_dropped = 0;
    let mut max_source_backlog = 0;
    let finish = config.deadline_ns(count as u64 - 1)? + 50_000_000;
    let mut service_gaps = ServiceGaps::default();
    let mut last_service = 0;
    while origin.elapsed().as_nanos() as u64 <= finish {
        let now = origin.elapsed().as_nanos() as u64;
        service_gaps.observe(now - last_service);
        last_service = now;
        let mut progressed = false;
        for _ in 0..128 {
            match udp.recv(&mut raw) {
                Ok(len) => {
                    let received_at = origin.elapsed().as_nanos() as u64;
                    match wire::decode(&raw[..len], config) {
                        Ok(packet)
                            if packet.header.kind == wire::OUTPUT
                                && packet.header.epoch == epoch
                                && packet.header.sequence < count as u64 =>
                        {
                            let _ = returns.accept(&raw[..len], received_at);
                            let sequence = packet.header.sequence as usize;
                            if rx[sequence] != u64::MAX {
                                duplicate += 1;
                                continue;
                            }
                            rx[sequence] = received_at;
                            processing[sequence] = packet.header.process_ns;
                            for c in 0..config.channels as usize {
                                for i in 0..p {
                                    samples[sequence * p * config.channels as usize + c * p + i] =
                                        packet.sample(c, i);
                                }
                            }
                        }
                        _ => invalid += 1,
                    }
                    progressed = true;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => {
                    invalid += 1;
                    break;
                }
            }
        }
        let consume_now = origin.elapsed().as_nanos() as u64;
        while consume_next < count && consume_now >= config.deadline_ns(consume_next as u64)? {
            let delivery = returns.consume(
                consume_next as u64,
                consume_now,
                &mut presentation,
                &mut presentation_midi,
            )?;
            delivered[consume_next] =
                delivery.outcome != shoop_remote_dsp::returns::Outcome::Silence;
            consume_next += 1;
        }
        for _ in 0..8 {
            if next == count || now < config.ready_ns(next as u64)? {
                break;
            }
            let sequence = next;
            next += 1;
            progressed = true;
            let backlog = (now - config.ready_ns(sequence as u64)?) / config.packet_ns();
            max_source_backlog = max_source_backlog.max(backlog);
            if options.drop_every > 0
                && sequence as u64 % options.drop_every == options.drop_every - 1
            {
                deliberately_dropped += 1;
                continue;
            }
            if backlog > config.reorder_packets as u64 + config.multiplier as u64 {
                send_errors += 1;
                continue;
            }
            for c in 0..config.channels as usize {
                for i in 0..p {
                    input[c * p + i] =
                        signal((sequence * p + i) as u64, c, options.seed, options.impulse);
                }
            }
            let mut header = wire::Header::input(epoch, sequence as u64, config);
            header.transport.playing = true;
            let length = wire::encode(header, config, &input, &[], &mut out)?;
            let sent_at = origin.elapsed().as_nanos() as u64;
            match udp.send(&out[..length]) {
                Ok(_) => tx[sequence] = sent_at,
                Err(_) => send_errors += 1,
            }
            if options.duplicate_every > 0 && sequence as u64 % options.duplicate_every == 0 {
                let _ = udp.send(&out[..length]);
            }
        }
        if !progressed {
            options.polling.idle();
        }
    }
    while consume_next < count {
        let delivery = returns.consume(
            consume_next as u64,
            finish,
            &mut presentation,
            &mut presentation_midi,
        )?;
        delivered[consume_next] = delivery.outcome != shoop_remote_dsp::returns::Outcome::Silence;
        consume_next += 1;
    }
    control::call(&mut tcp, &Request::Pause)?;
    let (stats, concealment, next_sequence, server_service_gaps) =
        match control::call(&mut tcp, &Request::Stats)? {
            Reply::Status {
                stats,
                concealment,
                next_sequence,
                service_gaps,
            } => (stats, concealment, next_sequence, service_gaps),
            _ => anyhow::bail!("expected stream statistics"),
        };
    if let Some(path) = &options.state_output {
        match control::call(&mut tcp, &Request::Save)? {
            Reply::State { state } => std::fs::write(path, state)?,
            _ => anyhow::bail!("expected algorithm state"),
        }
    }
    control::call(&mut tcp, &Request::Stop)?;
    let mut concealed = vec![false; count];
    for (base, mask) in &concealment {
        for j in 0..k {
            if mask & (1 << j) != 0 && *base as usize + j < count {
                concealed[*base as usize + j] = true;
            }
        }
    }
    let mut on_time = 0;
    let mut late = 0;
    let mut missing = 0;
    let mut correctness_errors = 0u64;
    let mut verified_samples = 0u64;
    let mut unverified_samples = 0u64;
    let mut sender_late = 0;
    let mut measured_sent = 0;
    let mut rtt = Vec::new();
    let mut lateness = Vec::new();
    let mut batch = Vec::new();
    let mut dsp_times = Vec::new();
    for sequence in warmup_packets..count {
        if tx[sequence] != u64::MAX {
            measured_sent += 1;
            let late_ns = tx[sequence].saturating_sub(config.ready_ns(sequence as u64)?);
            lateness.push(late_ns);
            if late_ns >= config.packet_ns() {
                sender_late += 1;
            }
        }
        if rx[sequence] == u64::MAX {
            missing += 1;
            continue;
        }
        if delivered[sequence] {
            on_time += 1;
        } else {
            late += 1;
        }
        if tx[sequence] != u64::MAX {
            rtt.push(rx[sequence].saturating_sub(tx[sequence]));
        }
        let expected_gain = if options.processor == "delay-copy" {
            Some(1.)
        } else {
            options.expected_gain
        };
        for c in 0..config.channels as usize {
            for i in 0..p {
                let n = sequence * p + i;
                if stats.diagnostics_truncated || expected_gain.is_none() {
                    unverified_samples += 1;
                    continue;
                }
                let delay = if options.processor == "delay-copy" {
                    options.delay as usize
                } else {
                    0
                };
                let expected = if n < delay || concealed[(n - delay) / p] {
                    0.0
                } else {
                    signal((n - delay) as u64, c, options.seed, options.impulse)
                        * expected_gain.unwrap()
                };
                let actual = samples[sequence * p * config.channels as usize + c * p + i];
                if if options.processor == "delay-copy" {
                    actual.to_bits() != expected.to_bits()
                } else {
                    !actual.is_finite() || (actual - expected).abs() > 0.0001
                } {
                    correctness_errors += 1;
                }
                verified_samples += 1;
            }
        }
    }
    for base in (warmup_packets..count).step_by(k) {
        let last = base + k - 1;
        if !stats.diagnostics_truncated
            && !concealed[base..base + k].iter().any(|v| *v)
            && tx[base..base + k].iter().all(|t| *t != u64::MAX)
            && rx[base..base + k].iter().all(|r| *r != u64::MAX)
        {
            batch.push(
                rx[base..base + k]
                    .iter()
                    .max()
                    .unwrap()
                    .saturating_sub(tx[last]),
            );
        }
        if let Some(i) = (base..base + k).find(|i| rx[*i] != u64::MAX) {
            dsp_times.push(processing[i]);
        }
    }
    let report = json!({"status":"measured","config":config,"algorithm":options.processor,"delay_frames":options.delay,
        "algorithm_latency_frames":latency,"seed":options.seed,"impulse":options.impulse,"warmup_seconds":options.warmup,"measurement_seconds":options.seconds,
        "capture_grace_ms":50,"total_packets":count,"measured_packets":measured_packets,"sent":measured_sent,"on_time":on_time,"late":late,"missing":missing,
        "deadline_miss_fraction":(late+missing) as f64/measured_packets as f64,"sender_period_overruns":sender_late,"maximum_sender_backlog_packets":max_source_backlog,
        "send_errors_or_skipped":send_errors,"deliberately_dropped":deliberately_dropped,"duplicates":duplicate,"invalid":invalid,
        "correctness_errors":correctness_errors,"verified_samples":verified_samples,"unverified_samples":unverified_samples,
        "server":stats,"concealment":concealment,"next_sequence":next_sequence,
        "polling":options.polling,"client_service_gaps":service_gaps,"server_service_gaps":server_service_gaps,
        "socket_buffers":{"server_receive":server_receive_buffer,"server_send":server_send_buffer,"client_receive":client_receive_buffer,"client_send":client_send_buffer},
        "rtt":distribution(rtt),"sender_lateness":distribution(lateness),"batch_turnaround":distribution(batch),"dsp_processing":distribution(dsp_times)});
    if let Some(path) = &options.output {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    println!(
        "{}",
        json!({"status":"measured","correctness_errors":correctness_errors,"on_time":on_time,"late":late,"missing":missing,"output":options.output})
    );
    ensure!(correctness_errors == 0, "DSP sample correctness failed");
    Ok(())
}
