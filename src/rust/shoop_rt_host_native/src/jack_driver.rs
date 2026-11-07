use crate::jack_graph::{self as graph, Generations};
use crate::jack_ports;
use shoop_port_graph::{self as contract, Snapshot, Status};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use arrayvec::ArrayVec;
use jack::{AudioIn, AudioOut, MidiIn, Port};
use shoop_rt_engine::{EngineDescription, HostPorts, MAX_HOST_PORTS};
use shoop_rt_protocol::{DriverKind, DriverRunState, Name};

use crate::cycle::{Cycle, DirectIo};
use crate::status::{RenderPhase, Shared, StreamFailure};
use crate::{assemble, reset_wait, Driver, Prepared, Running, StartError, Started};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JackConfig {
    pub client_name: String,
}
impl Default for JackConfig {
    fn default() -> Self {
        Self {
            client_name: "ShoopDaLoop".into(),
        }
    }
}
impl JackConfig {
    pub fn validate(&self) -> Result<(), StartError> {
        if self.client_name.trim().is_empty()
            || self.client_name.len() > 63
            || self.client_name.contains([':', '\0'])
        {
            return Err(StartError::InvalidConfig("invalid JACK client name"));
        }
        Ok(())
    }
}

struct Notifications(Arc<Shared>, Arc<AtomicU64>, Arc<Generations>);
impl jack::NotificationHandler for Notifications {
    fn port_registration(&mut self, _: &jack::Client, id: jack::PortId, _: bool) {
        self.2.changed(id);
        self.1.fetch_add(1, Ordering::Release);
    }
    fn port_rename(
        &mut self,
        _: &jack::Client,
        _: jack::PortId,
        _: &str,
        _: &str,
    ) -> jack::Control {
        self.1.fetch_add(1, Ordering::Release);
        jack::Control::Continue
    }
    fn ports_connected(&mut self, _: &jack::Client, _: jack::PortId, _: jack::PortId, _: bool) {
        self.1.fetch_add(1, Ordering::Release);
    }
    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
        self.0.xrun(&self.0.counters.stream_underruns);
        jack::Control::Continue
    }
    fn sample_rate(&mut self, _: &jack::Client, rate: u32) -> jack::Control {
        self.observe_sample_rate(rate);
        jack::Control::Continue
    }
}
impl Notifications {
    fn observe_sample_rate(&self, rate: u32) {
        if rate != self.0.sample_rate() {
            self.0.fail(StreamFailure::StreamInvalidated);
        }
    }
}

struct Process {
    cycle: Cycle,
    ports: jack_ports::Callback,
    shared: Arc<Shared>,
}
struct Io<'a, 's> {
    scope: &'s jack::ProcessScope,
    inputs: &'a [Option<Port<AudioIn>>],
    outputs: &'a mut [Option<Port<AudioOut>>],
    midi: &'a [Option<Port<MidiIn>>],
    writers: &'a mut [Option<jack::MidiWriter<'s>>],
}
impl DirectIo for Io<'_, '_> {
    fn begin(&mut self) {}
    fn input(&self, port: usize, offset: usize, samples: &mut [f32]) {
        if let Some(input) = &self.inputs[port] {
            samples.copy_from_slice(&input.as_slice(self.scope)[offset..offset + samples.len()]);
        } else {
            samples.fill(0.);
        }
    }
    fn output(&mut self, port: usize, offset: usize, samples: &[f32]) {
        if let Some(output) = &mut self.outputs[port] {
            output.as_mut_slice(self.scope)[offset..offset + samples.len()]
                .copy_from_slice(samples);
        }
    }
    fn midi_input(&self, port: usize, receive: &mut dyn FnMut(u32, &[u8])) {
        if let Some(input) = &self.midi[port] {
            for event in input.iter(self.scope) {
                receive(event.time, event.bytes);
            }
        }
    }
    fn midi_output(&mut self, port: usize, offset: u32, bytes: &[u8]) -> bool {
        self.writers
            .get_mut(port)
            .and_then(Option::as_mut)
            .is_none_or(|writer| {
                writer
                    .write(&jack::RawMidi {
                        time: offset,
                        bytes,
                    })
                    .is_ok()
            })
    }
}
impl jack::ProcessHandler for Process {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        self.ports.begin(scope);
        let failed = self.shared.failure().is_some();
        {
            // One writer per prepared MIDI output; the capacity stays uninitialized.
            let mut writers: ArrayVec<Option<jack::MidiWriter<'_>>, MAX_HOST_PORTS> = self
                .ports
                .midi_outputs
                .iter_mut()
                .map(|port| port.as_mut().map(|port| port.writer(scope)))
                .collect();
            if failed {
                for port in self.ports.outputs.iter_mut().flatten() {
                    port.as_mut_slice(scope).fill(0.);
                }
            } else {
                if self.shared.phase() == RenderPhase::Disconnected
                    && self.shared.stopping.load(Ordering::Acquire)
                {
                    for writer in writers.iter_mut().flatten() {
                        let mut reset = rt_runtime::midi::ResetState::default();
                        reset.request();
                        while let Some(bytes) = reset.peek() {
                            if writer
                                .write(&jack::RawMidi {
                                    time: 0,
                                    bytes: &bytes,
                                })
                                .is_err()
                            {
                                Shared::count(&self.shared.counters.midi_out_failed, 1);
                                break;
                            }
                            reset.acknowledge().expect("pending reset packet");
                        }
                    }
                }
                let mut io = Io {
                    scope,
                    inputs: &self.ports.inputs,
                    outputs: &mut self.ports.outputs,
                    midi: &self.ports.midi_inputs,
                    writers: &mut writers,
                };
                self.cycle
                    .process_direct(scope.n_frames() as usize, &mut io);
            }
        }
        self.ports.finish();
        if !failed {
            self.shared.set_state(DriverRunState::Running);
        }
        jack::Control::Continue
    }
    fn buffer_size(&mut self, _: &jack::Client, frames: u32) -> jack::Control {
        self.shared.set_buffer_frames(frames);
        jack::Control::Continue
    }
}

pub(crate) struct Client {
    pub graph: contract::Control,
    monitor_stop: Arc<AtomicBool>,
    monitor: JoinHandle<Option<Cycle>>,
}
impl Client {
    pub fn stop(self) -> Option<Cycle> {
        self.monitor_stop.store(true, Ordering::Release);
        self.monitor.join().ok().flatten()
    }
}
fn error(error: impl std::fmt::Display) -> StartError {
    StartError::Device(error.to_string())
}

pub(crate) fn start(
    config: &JackConfig,
    engine: &EngineDescription,
) -> Result<Started, StartError> {
    config.validate()?;
    let (client, _) = jack::Client::new(&config.client_name, jack::ClientOptions::NO_START_SERVER)
        .map_err(error)?;
    let instance_name = client.name().to_owned();
    let sample_rate = client.sample_rate();
    let buffer_frames = client.buffer_size();
    let mut ports = HostPorts::numbered(0, 0, 0);
    let capacities = [
        engine.strips() * engine.strip_channels as usize,
        engine.strips() * engine.strip_channels as usize
            + engine.buses as usize * engine.bus_channels as usize,
        engine.strips(),
        engine.strips(),
    ];
    for (kind, bank) in [
        &mut ports.audio_inputs,
        &mut ports.audio_outputs,
        &mut ports.midi_inputs,
        &mut ports.midi_outputs,
    ]
    .into_iter()
    .enumerate()
    {
        if capacities[kind] > MAX_HOST_PORTS {
            return Err(StartError::InvalidConfig(
                "prepared session exceeds host I/O capacity",
            ));
        }
        while bank.len() < capacities[kind] {
            bank.push(
                Name::new(&format!(
                    "{}{kind}/{}",
                    shoop_rt_protocol::HOST_IO_PREFIX,
                    bank.len()
                ))
                .expect("bounded private port name"),
            );
        }
    }
    let slot_names: [Vec<String>; 4] = [
        &ports.audio_inputs,
        &ports.audio_outputs,
        &ports.midi_inputs,
        &ports.midi_outputs,
    ]
    .map(|ports| ports.iter().map(|p| p.as_str().to_owned()).collect());
    let Prepared {
        description,
        endpoint,
        outbox,
        render,
        shared,
        layout,
    } = Prepared::with_output_count(
        engine,
        ports,
        DriverKind::Jack,
        sample_rate,
        Some(buffer_frames),
        0,
    )?;
    let cycle = Cycle::new(render, shared.clone(), &description, Vec::new(), Vec::new())
        .map_err(|_| StartError::InvalidConfig("could not prepare JACK callback buffers"))?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let dirty = Arc::new(AtomicU64::new(1));
    let generations = Arc::new(Generations::new());
    let mut epoch = [0; 8];
    getrandom::fill(&mut epoch).map_err(error)?;
    let (control, mut worker) = contract::channel(
        u64::from_ne_bytes(epoch).max(1),
        "jack",
        graph::CAPABILITIES,
    )
    .map_err(error)?;
    worker.enable_io_bindings();
    let mut graph = graph::Graph::new();
    let (callback_ports, io_control) = jack_ports::prepare(slot_names);
    let mut publications = crate::jack_publication::Publications::new(io_control);
    let active = client
        .activate_async(
            shoop_jack::Notifications::new(
                Notifications(shared.clone(), dirty.clone(), generations.clone()),
                shutdown.clone(),
            ),
            Process {
                cycle,
                ports: callback_ports,
                shared: shared.clone(),
            },
        )
        .map_err(error)?;
    let monitor_stop = Arc::new(AtomicBool::new(false));
    let finished = monitor_stop.clone();
    let observed = shared.clone();
    let replies = control.clone();
    let monitor = thread::Builder::new()
        .name("jack-control".into())
        .spawn(move || {
            let client = active.as_client();
            let mut seen = 0;
            let mut last_scan = Instant::now();
            let refresh =
                |graph: &mut graph::Graph, worker: &mut contract::Worker, seen: &mut u64| {
                    for _ in 0..3 {
                        let before = dirty.load(Ordering::Acquire);
                        let snapshot = graph.read(client, &generations);
                        if before != dirty.load(Ordering::Acquire) {
                            continue;
                        }
                        let snapshot = snapshot.unwrap_or_else(|error| Snapshot {
                            status: Status::Unavailable,
                            error: Some(error),
                            ..Default::default()
                        });
                        worker.publish(snapshot)?;
                        *seen = before;
                        return Ok(());
                    }
                    Err(contract::Error::new(
                        contract::ErrorKind::Busy,
                        "JACK graph is changing",
                    ))
                };
            let confirm = |graph: &mut graph::Graph,
                           worker: &mut contract::Worker,
                           seen: &mut u64,
                           request: &contract::Request| {
                let deadline = Instant::now() + Duration::from_millis(100);
                loop {
                    let result = refresh(graph, worker, seen).and_then(|()| {
                        let snapshot = replies.snapshot();
                        snapshot.check(request)?;
                        let outcome = graph::Graph::outcome(&snapshot, request)?;
                        graph::Graph::confirm(&snapshot, request, outcome)
                    });
                    match result {
                        Err(error)
                            if Instant::now() < deadline
                                && !shutdown.load(Ordering::Acquire)
                                && !finished.load(Ordering::Acquire)
                                && matches!(
                                    error.kind,
                                    contract::ErrorKind::Backend | contract::ErrorKind::Busy
                                ) =>
                        {
                            thread::sleep(Duration::from_millis(1))
                        }
                        result => return result,
                    }
                }
            };
            while !finished.load(Ordering::Acquire) {
                if shutdown.load(Ordering::Acquire) {
                    observed.fail(StreamFailure::DeviceNotAvailable);
                    break;
                }
                if seen != dirty.load(Ordering::Acquire)
                    || last_scan.elapsed() >= Duration::from_secs(1)
                {
                    let _ = refresh(&mut graph, &mut worker, &mut seen);
                    last_scan = Instant::now();
                }
                if let Some((id, request, result)) =
                    publications.poll(client, &mut graph, &mut worker)
                {
                    let result =
                        result.and_then(|()| confirm(&mut graph, &mut worker, &mut seen, &request));
                    worker.complete(id, result);
                }
                if !publications.busy() {
                    if let Some((id, request)) = worker.next_request() {
                        let refreshed = refresh(&mut graph, &mut worker, &mut seen);
                        if matches!(
                            request.operation,
                            contract::Operation::Publish { .. }
                                | contract::Operation::Unpublish { .. }
                        ) {
                            let result = refreshed.and_then(|()| {
                                publications.start(
                                    client,
                                    &mut graph,
                                    &replies.snapshot(),
                                    &mut worker,
                                    id,
                                    request.clone(),
                                )
                            });
                            let _ = refresh(&mut graph, &mut worker, &mut seen);
                            match result {
                                Ok(true) => {}
                                Ok(false) => {
                                    let result =
                                        confirm(&mut graph, &mut worker, &mut seen, &request);
                                    worker.complete(id, result);
                                }
                                Err(error) => worker.complete(id, Err(error)),
                            }
                        } else {
                            let result = refreshed.and_then(|()| {
                                graph.apply(client, &generations, &replies.snapshot(), &request)
                            });
                            let result = result.and_then(|_| {
                                confirm(&mut graph, &mut worker, &mut seen, &request)
                            });
                            worker.complete(id, result);
                        }
                    }
                }
                thread::sleep(Duration::from_millis(1));
            }
            worker.close();
            let grace = reset_wait(observed.driver().buffer_frames, observed.sample_rate())
                .max(Duration::from_millis(250));
            let deadline = Instant::now() + grace;
            while observed.phase() == RenderPhase::Running
                && observed.failure().is_none()
                && !shutdown.load(Ordering::Acquire)
                && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(1));
            }
            if observed.phase() == RenderPhase::Disconnected && observed.failure().is_none() {
                // Let the reset block cross the graph, including feedback edges, before removing ports.
                let through = observed.stats().callbacks.saturating_add(2);
                let deadline = Instant::now() + grace;
                while observed.stats().callbacks < through
                    && !shutdown.load(Ordering::Acquire)
                    && Instant::now() < deadline
                {
                    thread::sleep(Duration::from_millis(1));
                }
                observed
                    .reset_flushed
                    .store(observed.stats().callbacks >= through, Ordering::Release);
            }
            if shutdown.load(Ordering::Acquire) {
                observed.fail(StreamFailure::DeviceNotAvailable);
            }
            active
                .deactivate()
                .ok()
                .map(|(_, _, process)| process.cycle)
        })
        .map_err(|failure| StartError::Thread(failure.to_string()))?;
    Ok(assemble(
        endpoint,
        outbox,
        Running {
            configuration: crate::DriverConfig::Jack(config.clone()),
            shared,
            driver: Driver::Jack(Client {
                graph: control,
                monitor_stop,
                monitor,
            }),
            pump: None,
            connections: Vec::new(),
            layout,
            warnings: Vec::new(),
            reset_wait: reset_wait(buffer_frames, sample_rate),
            dummy: None,
            instance_name,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test(no_wasm = "native JACK configuration notification")]
    fn sample_rate_change_invalidates_the_prepared_host_but_initial_notification_does_not() {
        let engine = shoop_rt_engine::prepare(
            &EngineDescription::default(),
            shoop_rt_protocol::DriverStatus {
                kind: DriverKind::Jack,
                state: DriverRunState::Running,
                sample_rate: 48000,
                buffer_frames: 128,
            },
        )
        .unwrap();
        let shared = Arc::new(Shared::new(
            engine.host,
            shoop_rt_protocol::DriverStatus {
                kind: DriverKind::Jack,
                state: DriverRunState::Running,
                sample_rate: 48000,
                buffer_frames: 128,
            },
        ));
        let notifications = Notifications(
            shared.clone(),
            Arc::new(AtomicU64::new(0)),
            Arc::new(Generations::new()),
        );
        notifications.observe_sample_rate(48000);
        assert!(shared.failure().is_none());
        notifications.observe_sample_rate(44100);
        assert_eq!(shared.failure(), Some(StreamFailure::StreamInvalidated));
        assert_eq!(shared.driver().state, DriverRunState::Failed);
    }
}
