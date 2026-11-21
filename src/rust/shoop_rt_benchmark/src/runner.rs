use crate::fixture::{self, Action, Fixture, LOOPS, QUANTUM};
use crate::host_timing;
use anyhow::{anyhow, bail, ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Response, COMMANDS},
    snapshot, ClientMessage, EngineMessage, LoopMode, Telemetry, PROTOCOL_VERSION,
};
use shoop_rt_transport::carrier::{Endpoint, Outbox};
use std::time::Instant;

#[derive(Debug)]
struct ProcessingFailure(anyhow::Error);
impl std::fmt::Display for ProcessingFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ProcessingFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[derive(Debug)]
struct Unsupported;
impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native CPU immediate-command transitions refuse MIDI loops; authored transport is a different control path")
    }
}
impl std::error::Error for Unsupported {}

#[derive(Clone, Copy, Default, Serialize)]
pub struct Measurement {
    pub elapsed_ns: u64,
    pub fabric_cycles: Option<u64>,
    pub storage_bytes: u64,
    pub reclaimed: u64,
    pub reads: u64,
    pub writes: u64,
    pub state_cycles: u64,
    pub job_cycles: u64,
    pub queue_peak: u64,
    pub axi_address_stall_cycles: u64,
    pub axi_write_stall_cycles: u64,
    pub axi_response_stall_cycles: u64,
    pub dram_queue_stall_cycles: u64,
    pub midi_axi_address_stall_cycles: u64,
    pub midi_axi_write_stall_cycles: u64,
    pub midi_axi_response_stall_cycles: u64,
    pub midi_dram_queue_stall_cycles: u64,
    pub midi_reads: u64,
    pub midi_writes: u64,
    pub midi_storage_blocks: Option<u64>,
    pub midi_input_events: u64,
    pub midi_output_events: u64,
    pub midi_record_reads: Option<u64>,
    pub midi_captured_events: Option<u64>,
    pub midi_merged_events: Option<u64>,
}

pub trait Driver {
    fn begin_fixture(&mut self) {}
    fn next_frame(&self) -> u64;
    fn fill_input(&mut self);
    fn process(&mut self) -> Result<Measurement>;
    fn output(&self, channel: usize) -> &[f32];
    fn midi_output(&self) -> &[fixture::MidiPacket] {
        &[]
    }
    fn midi_latency_frames(&self) -> usize {
        0
    }
    fn latency_frames(&self) -> usize {
        0
    }
    fn supports_grab(&self) -> bool {
        false
    }
    fn metadata(&self) -> serde_json::Value;
    fn finish(&mut self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
}

#[derive(Clone, Copy, Default)]
pub struct LoopState {
    pub mode: u8,
    pub position: u64,
    pub length: u64,
    pub revision: u64,
    pub planned: u8,
}

pub struct Control<E> {
    pub endpoint: E,
    outbox: Outbox,
    pub responses: Vec<Response>,
    pub configuration: (u64, u64),
    pub states: [LoopState; LOOPS as usize],
    pub snapshot_frame: u64,
    bytes: Vec<u8>,
    received: usize,
    total: usize,
    pub results: Vec<(u64, shoop_rt_protocol::Outcome)>,
}
impl<E: Endpoint> Control<E> {
    pub fn new(endpoint: E, outbox: Outbox) -> Result<Self> {
        let mut this = Self {
            endpoint,
            outbox,
            responses: Vec::with_capacity(16),
            configuration: (0, 0),
            states: [LoopState::default(); LOOPS as usize],
            snapshot_frame: 0,
            bytes: vec![0; snapshot::capacity(1, LOOPS as usize, 1).unwrap()],
            received: 0,
            total: 0,
            results: Vec::with_capacity(256),
        };
        this.send(ClientMessage::Hello {
            version: PROTOCOL_VERSION,
        })?;
        this.send(ClientMessage::ObserveSnapshots(true))?;
        this.send(ClientMessage::ConfigureSnapshots(
            snapshot::SnapshotDelivery::Pull,
        ))?;
        this.pump()?;
        Ok(this)
    }
    pub fn send(&mut self, message: ClientMessage) -> Result<()> {
        self.endpoint
            .try_deliver(message, &mut self.outbox)
            .map_err(|e| anyhow!("endpoint refused delivery: {e:?}"))
    }
    pub fn pump(&mut self) -> Result<()> {
        for _ in 0..4096 {
            self.endpoint.poll(&mut self.outbox);
            let Some(message) = self.outbox.pop() else {
                return Ok(());
            };
            match message {
                EngineMessage::ImmediateResponse(response) => self.responses.push(response),
                EngineMessage::SnapshotConfiguration { graph, topology } => {
                    self.configuration = (graph, topology)
                }
                EngineMessage::SnapshotReply(chunk) => {
                    if chunk.offset == 0 {
                        self.received = 0;
                        self.total = chunk.total as usize;
                    }
                    ensure!(
                        chunk.offset as usize == self.received && self.total <= self.bytes.len(),
                        "invalid snapshot transfer"
                    );
                    let end = self.received + chunk.len as usize;
                    ensure!(end <= self.total, "snapshot overflow");
                    self.bytes[self.received..end]
                        .copy_from_slice(&chunk.bytes[..chunk.len as usize]);
                    self.received = end;
                }
                EngineMessage::Result {
                    correlation,
                    outcome,
                } => self.results.push((correlation, outcome)),
                EngineMessage::Disconnected(reason) => bail!("engine disconnected: {reason:?}"),
                _ => {}
            }
        }
        bail!("endpoint polling did not quiesce")
    }
    pub fn snapshot(&mut self) -> Result<()> {
        self.received = 0;
        self.total = 0;
        self.send(ClientMessage::RequestSnapshot { after: 0 })?;
        self.pump()?;
        ensure!(
            self.total > 0 && self.received == self.total,
            "incomplete snapshot"
        );
        let header = snapshot::Header::read(&self.bytes[..self.total])
            .ok_or_else(|| anyhow!("invalid snapshot header"))?;
        ensure!(
            header.loops == LOOPS,
            "snapshot has {} loops; expected {LOOPS}",
            header.loops
        );
        self.configuration = (header.graph, header.topology);
        self.snapshot_frame = header.frame;
        let mut seen = [false; LOOPS as usize];
        let mut plans = [0u8; LOOPS as usize];
        let mut plan_seen = [false; LOOPS as usize];
        for record in snapshot::Records(&self.bytes[snapshot::HEADER..self.total]) {
            let record = record.map_err(|e| anyhow!("snapshot record: {e:?}"))?;
            if let snapshot::Record::LoopPlan { id, plan, failures } = record {
                let slot = id.0.index as usize;
                ensure!(
                    slot < plans.len() && !plan_seen[slot] && failures == 0,
                    "invalid or failed loop plans"
                );
                plan_seen[slot] = true;
                plans[slot] =
                    u8::from(plan.first.is_some()) | (u8::from(plan.second.is_some()) << 1);
            }
            if let snapshot::Record::Telemetry(Telemetry::Loop {
                loop_id,
                mode,
                position,
                length,
                capture_incomplete,
                content_revision,
                ..
            }) = record
            {
                let id = loop_id.0.index as usize;
                ensure!(
                    id < seen.len() && !seen[id] && !capture_incomplete,
                    "invalid/incomplete loop capture"
                );
                seen[id] = true;
                self.states[id] = LoopState {
                    mode: match mode {
                        LoopMode::Stopped => 0,
                        LoopMode::Playing => 1,
                        LoopMode::Recording => 2,
                        _ => bail!("unexpected mode {mode:?}"),
                    },
                    position,
                    length,
                    revision: content_revision,
                    planned: 0,
                };
            }
        }
        ensure!(seen.into_iter().all(|v| v), "missing loop observations");
        ensure!(
            plan_seen.into_iter().all(|v| v),
            "missing loop plan observations"
        );
        for (state, plan) in self.states.iter_mut().zip(plans) {
            state.planned = plan;
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct Callback {
    pub callback: usize,
    pub frame: u64,
    pub event: Option<String>,
    pub control_ns: u64,
    pub measurement: Measurement,
    pub recording: u32,
    pub playing: u32,
    pub wraps: u32,
    pub planned_slots_retired: u32,
    pub takes_replaced: u32,
    pub output_peak: [f32; 2],
    pub midi_input_events: usize,
    pub midi_output_events: usize,
}
#[derive(Serialize)]
pub struct EventResult {
    pub label: String,
    pub callback: usize,
    pub execution_frame: u64,
    pub commands: usize,
    pub groups: usize,
    pub same_frame: bool,
}
#[derive(Serialize)]
pub struct Run {
    pub schema_version: u32,
    pub fixture_sha256: String,
    pub backend: String,
    pub status: String,
    pub fixture: Fixture,
    pub metadata: serde_json::Value,
    pub callbacks: Vec<Callback>,
    pub events: Vec<EventResult>,
    pub coverage: serde_json::Value,
    pub error: Option<String>,
}

pub fn run<E: Endpoint, D: Driver>(
    backend: &str,
    fixture: Fixture,
    mut control: Control<E>,
    mut driver: D,
) -> Run {
    driver.begin_fixture();
    let metadata = {
        let _metadata = host_timing::span("fixture.metadata");
        driver.metadata()
    };
    let mut report = Run {
        schema_version: 1,
        fixture_sha256: Sha256::digest(serde_json::to_vec(&fixture).expect("fixture encoding"))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        backend: backend.into(),
        status: "running".into(),
        metadata,
        callbacks: Vec::with_capacity(fixture.callbacks),
        events: Vec::with_capacity(fixture.events.len()),
        coverage: serde_json::json!({}),
        error: None,
        fixture,
    };
    if matches!(report.fixture.pattern, fixture::Pattern::Grab) && !driver.supports_grab() {
        report.status = "unsupported".into();
        report.error = Some("grab and input history are not supported by this backend".into());
    } else {
        let execution = {
            let _execute = host_timing::span("fixture.execute");
            execute(&mut report, &mut control, &mut driver)
        };
        match execution {
            Ok(()) => report.status = "completed".into(),
            Err(error) => {
                report.status = if error.downcast_ref::<Unsupported>().is_some() {
                    "unsupported"
                } else if error.downcast_ref::<ProcessingFailure>().is_some() {
                    "processing-failure"
                } else {
                    "incorrect-results"
                }
                .into();
                report.error = Some(format!("{error:#}"));
            }
        }
        let finalization = {
            let _finalization = host_timing::span("fixture.finalization");
            driver.finish()
        };
        match finalization {
            Ok(value) => report.metadata["finalization"] = value,
            Err(error) => {
                report.status = "processing-failure".into();
                report.error = Some(format!("finalization: {error:#}"));
            }
        }
    }
    {
        let _teardown = host_timing::span("fixture.teardown");
        drop(driver);
        drop(control);
    }
    report
}

fn execute<E: Endpoint, D: Driver>(
    report: &mut Run,
    control: &mut Control<E>,
    driver: &mut D,
) -> Result<()> {
    let mut expected = [0u8; LOOPS as usize];
    let mut captures = [0u64; LOOPS as usize];
    let mut gains = [fixture::GAIN; LOOPS as usize];
    let mut previous = [LoopState::default(); LOOPS as usize];
    let mut growth = 0;
    let mut released = 0;
    let mut last_storage = 0;
    let mut total_wraps = 0;
    let mut replaced_takes = 0;
    let mut retired_plans = 0;
    let mut cold_block_boundary_verified = false;
    let origin = driver.next_frame();
    let mut midi_input = Vec::with_capacity(fixture::MIDI_INPUT_CAPACITY);
    let mut expected_midi = Vec::with_capacity(fixture::MIDI_OUTPUT_CAPACITY);
    let mut delayed_midi = std::collections::VecDeque::new();
    delayed_midi.extend((0..driver.midi_latency_frames() / QUANTUM).map(|_| Vec::new()));
    let mut midi_events_checked = 0u64;
    let mut midi_silent_callbacks = 0u64;
    let mut midi_dense_callbacks = 0u64;
    let mut cold_midi_events = 0usize;
    let mut midi_block_boundary_verified = false;
    let mut delayed: [std::collections::VecDeque<f32>; 2] = std::array::from_fn(|_| {
        let mut queue = std::collections::VecDeque::with_capacity(driver.latency_frames() + 1);
        queue.extend(std::iter::repeat_n(0., driver.latency_frames()));
        queue
    });
    for callback in 0..report.fixture.callbacks {
        let _callback = host_timing::span("fixture.callback");
        if callback % 16 == 0 {
            eprintln!(
                "{} {:?}: callback {callback}/{}",
                report.backend, report.fixture.pattern, report.fixture.callbacks
            );
        }
        let frame = driver.next_frame();
        ensure!(
            frame == origin + (callback * QUANTUM) as u64,
            "callback/frame mismatch"
        );
        let event = report
            .fixture
            .events
            .iter()
            .find(|e| e.callback == callback);
        let commands = event.map(|e| e.commands()).unwrap_or_default();
        control.responses.clear();
        let delivery = host_timing::span("fixture.command_delivery");
        let start = Instant::now();
        for group in commands.chunks(COMMANDS) {
            let batch =
                Batch::new(control.configuration.0, control.configuration.1, group).unwrap();
            control.send(ClientMessage::Immediate(batch))?;
        }
        control.pump()?;
        let mut control_ns = start.elapsed().as_nanos() as u64;
        drop(delivery);
        {
            let _stimulus = host_timing::span("fixture.input_stimulus");
            driver.fill_input();
        }
        let measurement = {
            let _process = host_timing::span("fixture.process");
            driver.process().map_err(ProcessingFailure)?
        };
        if report.backend == "fpga-rtl" && report.fixture.media == fixture::Media::Midi {
            ensure!(
                measurement.reads == 0 && measurement.writes == 0,
                "MIDI-only callback touched audio media memory"
            );
        }
        let receive = host_timing::span("fixture.endpoint_receive");
        let start = Instant::now();
        control.pump()?;
        control.snapshot()?;
        control_ns += start.elapsed().as_nanos() as u64;
        drop(receive);
        let _validation = host_timing::span("fixture.validate_and_record");
        ensure!(
            control.snapshot_frame == frame,
            "snapshot frame mismatch: {} != {frame}",
            control.snapshot_frame
        );
        if callback == 0
            && report.backend == "cpu"
            && report.fixture.media.midi()
            && control
                .responses
                .iter()
                .map(|r| r.records().len())
                .sum::<usize>()
                == commands.len()
            && control.responses.iter().all(|r| {
                r.header.code() == Code::Applied
                    && r.records()
                        .iter()
                        .all(|record| record.code() == Code::Unsupported)
            })
        {
            return Err(Unsupported.into());
        }
        let mut completed = 0;
        for response in &control.responses {
            ensure!(
                response.header.code() == Code::Applied,
                "command group refused"
            );
            ensure!(
                response.header.frame() == frame,
                "commands executed at different frames"
            );
            for record in response.records() {
                ensure!(
                    matches!(record.code(), Code::Applied | Code::Scheduled),
                    "command failed: {:?}: {:?}",
                    record.command(),
                    record.code()
                );
                ensure!(
                    commands.get(completed).copied() == Some(record.command()),
                    "command response does not match submitted stimulus"
                );
                completed += 1;
            }
        }
        ensure!(
            completed == commands.len(),
            "missing command responses: {completed}/{}",
            commands.len()
        );
        if let Some(event) = event {
            for id in event.first..event.first + event.count {
                let i = id as usize;
                match event.action {
                    Action::Record => {
                        expected[i] = 2;
                        captures[i] = frame - origin;
                    }
                    Action::Play => expected[i] = 1,
                    Action::Grab => {
                        expected[i] = 1;
                        captures[i] = frame - origin - report.fixture.record_frames as u64;
                    }
                    Action::Stop => expected[i] = 0,
                    Action::Gain(value) => gains[i] = value,
                    _ => {}
                }
            }
            report.events.push(EventResult {
                label: event.label.clone(),
                callback,
                execution_frame: frame,
                commands: completed,
                groups: control.responses.len(),
                same_frame: true,
            });
        }
        let mut recording = 0;
        let mut playing = 0;
        let mut wraps = 0;
        let mut callback_replaced = 0;
        let mut callback_retired = 0;
        for (i, state) in control.states.iter().enumerate() {
            ensure!(
                state.mode == expected[i],
                "loop {i} mode {} != {} at callback {callback}",
                state.mode,
                expected[i]
            );
            if let Some(event) =
                event.filter(|event| (event.first..event.first + event.count).contains(&(i as u32)))
            {
                let length = match event.action {
                    Action::Play => Some(frame - origin - captures[i]),
                    Action::Grab => Some(report.fixture.record_frames as u64),
                    _ => None,
                };
                if let Some(length) = length {
                    ensure!(
                        state.length == length,
                        "loop {i} captured {} frames; expected {length}",
                        state.length
                    );
                }
            }
            recording += u32::from(state.mode == 2);
            playing += u32::from(state.mode == 1);
            wraps += u32::from(
                state.mode == 1 && previous[i].mode == 1 && state.position < previous[i].position,
            );
            callback_replaced +=
                u64::from(previous[i].revision != 0 && state.revision > previous[i].revision);
            callback_retired += (previous[i].planned & !state.planned).count_ones() as u64;
        }
        let mut peaks = [0f32; 2];
        for (channel, peak) in peaks
            .iter_mut()
            .enumerate()
            .take(report.fixture.media.audio_channels())
        {
            for (offset, &actual) in driver.output(channel).iter().enumerate() {
                ensure!(
                    actual.is_finite(),
                    "non-finite output at callback {callback} channel {channel} offset {offset}"
                );
                *peak = peak.max(actual.abs());
                {
                    let expected_sample: f32 = control
                        .states
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.mode == 1)
                        .map(|(i, s)| {
                            gains[i]
                                * fixture::input(
                                    channel,
                                    captures[i] + (s.position + offset as u64) % s.length.max(1),
                                )
                        })
                        .sum();
                    delayed[channel].push_back(expected_sample);
                    let expected_sample = delayed[channel].pop_front().unwrap();
                    ensure!((actual - expected_sample).abs() < 0.00002, "stereo sample mismatch at callback {callback} channel {channel} offset {offset}: {actual} != {expected_sample}");
                }
            }
        }
        let mut input_events = 0;
        if report.fixture.media.midi() {
            fixture::midi_input((callback * QUANTUM) as u64, &mut midi_input);
            input_events = midi_input.len();
            if let Some(captured) = measurement.midi_captured_events {
                ensure!(
                    captured == input_events as u64 * u64::from(recording),
                    "MIDI capture work differs from recording stimulus"
                );
            }
            if callback < report.fixture.record_frames / QUANTUM
                && !matches!(report.fixture.pattern, fixture::Pattern::Grab)
            {
                cold_midi_events += input_events;
                if cold_midi_events > fixture::MIDI_BLOCK_EVENTS as usize {
                    if let Some(blocks) = measurement.midi_storage_blocks {
                        ensure!(
                            blocks >= 2 * u64::from(LOOPS),
                            "MIDI recordings did not cross event blocks"
                        );
                    }
                    midi_block_boundary_verified = true;
                }
            }
            expected_midi.clear();
            for (id, state) in control
                .states
                .iter()
                .enumerate()
                .filter(|(_, s)| s.mode == 1)
            {
                ensure!(
                    state.position % QUANTUM as u64 == 0 && state.length % QUANTUM as u64 == 0,
                    "unaligned fixture MIDI playback position"
                );
                fixture::midi_input(captures[id] + state.position, &mut midi_input);
                expected_midi.extend(midi_input.iter().copied());
            }
            expected_midi.sort_by_key(|event| event.offset);
            delayed_midi.push_back(expected_midi.clone());
            let expected = delayed_midi.pop_front().unwrap();
            ensure!(driver.midi_output() == expected, "MIDI bytes/timestamps/order mismatch at callback {callback}: actual {} events, expected {}; first actual {:?}, expected {:?}",
                driver.midi_output().len(), expected.len(), driver.midi_output().first(), expected.first());
            midi_events_checked += expected.len() as u64;
            midi_silent_callbacks += u64::from(playing > 0 && expected.is_empty());
            midi_dense_callbacks += u64::from(expected.len() > 256);
        }
        growth += u64::from(measurement.storage_bytes > last_storage);
        released += u64::from(measurement.storage_bytes < last_storage);
        if callback == fixture::FPGA_BLOCK_FRAMES / QUANTUM
            && report.fixture.media.audio()
            && !matches!(report.fixture.pattern, fixture::Pattern::Grab)
        {
            ensure!(
                recording == LOOPS && measurement.storage_bytes > last_storage,
                "cold recordings did not grow at the FPGA block boundary"
            );
            cold_block_boundary_verified = true;
        }
        let observed_event = if callback_retired > 0 {
            Some("planned-transitions-due")
        } else if measurement.storage_bytes > last_storage && recording > 0 {
            Some("storage-growth")
        } else if measurement.storage_bytes < last_storage {
            Some("storage-release")
        } else if input_events == fixture::MIDI_INPUT_CAPACITY || driver.midi_output().len() > 256 {
            Some("midi-burst")
        } else {
            None
        };
        replaced_takes += callback_replaced;
        retired_plans += callback_retired;
        last_storage = measurement.storage_bytes;
        total_wraps += wraps;
        previous = control.states;
        report.callbacks.push(Callback {
            callback,
            frame,
            event: event
                .map(|e| e.label.clone())
                .or_else(|| observed_event.map(str::to_owned)),
            control_ns,
            measurement,
            recording,
            playing,
            wraps,
            planned_slots_retired: callback_retired as u32,
            takes_replaced: callback_replaced as u32,
            output_peak: peaks,
            midi_input_events: input_events,
            midi_output_events: driver.midi_output().len(),
        });
    }
    ensure!(
        growth > 1,
        "recordings did not grow across allocation boundaries"
    );
    ensure!(total_wraps >= LOOPS, "insufficient playback wrap coverage");
    if report.fixture.media.audio() && !matches!(report.fixture.pattern, fixture::Pattern::Grab) {
        ensure!(
            cold_block_boundary_verified,
            "missing cold block-boundary coverage"
        );
    }
    if matches!(report.fixture.pattern, fixture::Pattern::Lifecycle) {
        ensure!(
            replaced_takes >= u64::from(LOOPS),
            "completed takes were not replaced"
        );
        ensure!(
            retired_plans >= 2 * u64::from(LOOPS),
            "scheduled transitions were not exercised"
        );
    }
    if report.fixture.media.midi() {
        ensure!(
            midi_block_boundary_verified
                && midi_events_checked > 0
                && midi_dense_callbacks > 0
                && midi_silent_callbacks > 0,
            "missing MIDI block/burst/quiet validation coverage"
        );
    }
    report.coverage = serde_json::json!({"storage_growth_callbacks":growth,"storage_release_callbacks":released,
        "wraps":total_wraps,"replaced_takes":replaced_takes,"retired_plan_slots":retired_plans,"complete_samples":report.callbacks.len()==report.fixture.callbacks,
        "cold_block_boundary_verified":cold_block_boundary_verified,
        "stereo_samples_checked":report.fixture.media.audio(),
        "midi_events_checked":midi_events_checked,"midi_bytes_timestamps_order_checked":report.fixture.media.midi(),
        "midi_silent_playback_callbacks":midi_silent_callbacks,"midi_dense_playback_callbacks":midi_dense_callbacks,
        "midi_block_boundary_verified":midi_block_boundary_verified,"cold_midi_events_per_loop":cold_midi_events,"commands_verified":report.events.iter().map(|e| e.commands).sum::<usize>()});
    Ok(())
}
