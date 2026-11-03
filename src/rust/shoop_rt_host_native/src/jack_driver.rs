use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use jack::{AudioIn, AudioOut, MidiIn, MidiOut, Port};
use shoop_rt_engine::{EngineDescription, HostPorts, MAX_HOST_PORTS};
use shoop_rt_protocol::{DriverKind, DriverRunState, Name};

use crate::cycle::{Cycle, DirectIo};
use crate::status::{Shared, StreamFailure};
use crate::{assemble, reset_wait, Driver, Prepared, Running, StartError, Started};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JackConfig {
    pub client_name: String,
    pub audio_inputs: Vec<String>,
    pub audio_outputs: Vec<String>,
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
    /// A short endpoint is local; a client:port endpoint belongs to the JACK graph.
    pub connections: Vec<(String, String)>,
}
impl Default for JackConfig {
    fn default() -> Self {
        Self {
            client_name: "ShoopDaLoop".into(),
            audio_inputs: vec!["audio_in_1".into(), "audio_in_2".into()],
            audio_outputs: vec!["audio_out_1".into(), "audio_out_2".into()],
            midi_inputs: vec!["midi_in_1".into()],
            midi_outputs: vec!["midi_out_1".into()],
            connections: Vec::new(),
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
        let mut names = std::collections::BTreeSet::new();
        for ports in [
            &self.audio_inputs,
            &self.audio_outputs,
            &self.midi_inputs,
            &self.midi_outputs,
        ] {
            if ports.len() > MAX_HOST_PORTS {
                return Err(StartError::InvalidConfig("too many JACK ports"));
            }
            for name in ports {
                if name.trim().is_empty()
                    || name.len() > 59
                    || name.contains([':', ',', '\0'])
                    || !names.insert(name)
                {
                    return Err(StartError::InvalidConfig(
                        "invalid or duplicate JACK port name",
                    ));
                }
            }
        }
        if self.connections.len() > 256
            || self.connections.iter().any(|(a, b)| {
                [a, b]
                    .iter()
                    .any(|s| s.trim().is_empty() || s.len() > 256 || s.contains('\0'))
            })
        {
            return Err(StartError::InvalidConfig(
                "invalid JACK startup connections",
            ));
        }
        Ok(())
    }
}

struct Notifications(Arc<Shared>);
impl jack::NotificationHandler for Notifications {
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
    audio_inputs: Vec<Port<AudioIn>>,
    audio_outputs: Vec<Port<AudioOut>>,
    midi_inputs: Vec<Port<MidiIn>>,
    midi_outputs: Vec<Port<MidiOut>>,
    shared: Arc<Shared>,
}
struct Io<'a> {
    scope: &'a jack::ProcessScope,
    inputs: &'a [Port<AudioIn>],
    outputs: &'a mut [Port<AudioOut>],
    midi: &'a [Port<MidiIn>],
    writers: [Option<jack::MidiWriter<'a>>; MAX_HOST_PORTS],
}
impl DirectIo for Io<'_> {
    fn begin(&mut self) {}
    fn input(&self, port: usize, offset: usize, samples: &mut [f32]) {
        samples.copy_from_slice(
            &self.inputs[port].as_slice(self.scope)[offset..offset + samples.len()],
        );
    }
    fn output(&mut self, port: usize, offset: usize, samples: &[f32]) {
        self.outputs[port].as_mut_slice(self.scope)[offset..offset + samples.len()]
            .copy_from_slice(samples);
    }
    fn midi_input(&self, port: usize, receive: &mut dyn FnMut(u32, &[u8])) {
        for event in self.midi[port].iter(self.scope) {
            receive(event.time, event.bytes);
        }
    }
    fn midi_output(&mut self, port: usize, offset: u32, bytes: &[u8]) -> bool {
        self.writers[port].as_mut().is_some_and(|writer| {
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
        let mut writers = std::array::from_fn(|_| None);
        for (slot, port) in writers.iter_mut().zip(&mut self.midi_outputs) {
            *slot = Some(port.writer(scope));
        }
        if self.shared.failure().is_some() {
            for port in &mut self.audio_outputs {
                port.as_mut_slice(scope).fill(0.);
            }
            return jack::Control::Continue;
        }
        let mut io = Io {
            scope,
            inputs: &self.audio_inputs,
            outputs: &mut self.audio_outputs,
            midi: &self.midi_inputs,
            writers,
        };
        self.cycle
            .process_direct(scope.n_frames() as usize, &mut io);
        self.shared.set_state(DriverRunState::Running);
        jack::Control::Continue
    }
    fn buffer_size(&mut self, _: &jack::Client, frames: u32) -> jack::Control {
        self.shared.set_buffer_frames(frames);
        jack::Control::Continue
    }
}

pub(crate) struct Client {
    active: jack::AsyncClient<shoop_jack::Notifications<Notifications>, Process>,
    monitor_stop: Arc<AtomicBool>,
    monitor: JoinHandle<()>,
}
impl Client {
    pub fn stop(self) -> Option<Cycle> {
        self.monitor_stop.store(true, Ordering::Release);
        let _ = self.monitor.join();
        self.active
            .deactivate()
            .ok()
            .map(|(_, _, process)| process.cycle)
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
    let names = |ports: &[String]| {
        ports
            .iter()
            .map(|name| Name::new(&format!("jack:{name}")).expect("validated port name"))
            .collect()
    };
    let ports = HostPorts {
        audio_inputs: names(&config.audio_inputs),
        audio_outputs: names(&config.audio_outputs),
        midi_inputs: names(&config.midi_inputs),
        midi_outputs: names(&config.midi_outputs),
    };
    let audio_inputs = config
        .audio_inputs
        .iter()
        .map(|name| {
            client
                .register_port(name, AudioIn::default())
                .map_err(error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let audio_outputs = config
        .audio_outputs
        .iter()
        .map(|name| {
            client
                .register_port(name, AudioOut::default())
                .map_err(error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let midi_inputs = config
        .midi_inputs
        .iter()
        .map(|name| client.register_port(name, MidiIn::default()).map_err(error))
        .collect::<Result<Vec<_>, _>>()?;
    let midi_outputs = config
        .midi_outputs
        .iter()
        .map(|name| {
            client
                .register_port(name, MidiOut::default())
                .map_err(error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let Prepared {
        description,
        endpoint,
        outbox,
        render,
        shared,
        layout,
    } = Prepared::new(
        engine,
        ports,
        DriverKind::Jack,
        sample_rate,
        Some(buffer_frames),
    )?;
    let cycle = Cycle::new(render, shared.clone(), &description, Vec::new(), Vec::new())
        .map_err(|_| StartError::InvalidConfig("could not prepare JACK callback buffers"))?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let active = client
        .activate_async(
            shoop_jack::Notifications::new(Notifications(shared.clone()), shutdown.clone()),
            Process {
                cycle,
                audio_inputs,
                audio_outputs,
                midi_inputs,
                midi_outputs,
                shared: shared.clone(),
            },
        )
        .map_err(error)?;
    let endpoint_name = |name: &str| {
        if name.contains(':') {
            name.to_owned()
        } else {
            format!("{instance_name}:{name}")
        }
    };
    for (source, destination) in &config.connections {
        if let Err(failure) = active
            .as_client()
            .connect_ports_by_name(&endpoint_name(source), &endpoint_name(destination))
        {
            drop(active);
            return Err(error(format!(
                "JACK connection {source} -> {destination}: {failure}"
            )));
        }
    }
    let monitor_stop = Arc::new(AtomicBool::new(false));
    let finished = monitor_stop.clone();
    let observed = shared.clone();
    let monitor = thread::Builder::new()
        .name("jack-status".into())
        .spawn(move || {
            while !finished.load(Ordering::Acquire) {
                if shutdown.load(Ordering::Acquire) {
                    observed.fail(StreamFailure::DeviceNotAvailable);
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        })
        .map_err(|failure| StartError::Thread(failure.to_string()))?;
    Ok(assemble(
        endpoint,
        outbox,
        Running {
            shared,
            driver: Driver::Jack(Client {
                active,
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
        let notifications = Notifications(shared.clone());
        notifications.observe_sample_rate(48000);
        assert!(shared.failure().is_none());
        notifications.observe_sample_rate(44100);
        assert_eq!(shared.failure(), Some(StreamFailure::StreamInvalidated));
        assert_eq!(shared.driver().state, DriverRunState::Failed);
    }
}
