//! JACK driver: owns the engine on JACK's realtime thread and shuttles buffers.
//!
//! Behind the `jack` feature, so building and testing the engine does not need
//! libjack. Nothing here is covered by the test suite, because exercising it needs a
//! running JACK server; what the suite does cover is everything it delegates to --
//! the engine's command queue and the external ports' staging, both of which are
//! driven exactly as this driver drives them in `tests/external_ports.rs`.
//!
//! JACK hands out port buffers only inside the process callback, and only for the
//! duration of that call. Rather than teach every port to borrow from a
//! `ProcessScope`, this copies: input buffers are staged into the session's ports
//! before the cycle and output buffers are copied back out after. That costs one
//! memcpy per port per cycle and keeps the whole engine free of JACK's lifetimes.
//!
//! Not implemented here, and deliberately: `Driver` is not a trait yet. This is only
//! the second driver, so the shape it shares with the dummy driver is still a guess;
//! see the note in `dummy_driver.rs`.

use crate::driver::Driver;
use crate::engine::{Engine, EngineHandle, Stats};
use crate::external_audio_port::ExternalAudioPort;
use crate::external_midi_port::ExternalMidiPort;
use crate::port::PortDirection;
use crate::session::{Port, Session};

use std::sync::atomic::Ordering;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JackError {
    #[error("JACK refused the client: {0}")]
    Client(#[from] jack::Error),
}

/// A JACK port paired with the session port index it feeds or drains.
struct Bound<P> {
    jack: P,
    session_idx: usize,
}

/// Registered ports, built before activation and then moved onto JACK's thread.
#[derive(Default)]
pub struct PortMap {
    audio_in: Vec<Bound<jack::Port<jack::AudioIn>>>,
    audio_out: Vec<Bound<jack::Port<jack::AudioOut>>>,
    midi_in: Vec<Bound<jack::Port<jack::MidiIn>>>,
    midi_out: Vec<Bound<jack::Port<jack::MidiOut>>>,
}

/// Registers a JACK port and the matching session port, returning the session index.
///
/// The two are created together so they cannot drift apart: a JACK port with no
/// session port behind it would silently carry nothing.
pub fn add_audio_port(
    client: &jack::Client,
    session: &mut Session,
    ports: &mut PortMap,
    name: &str,
    direction: PortDirection,
    ringbuffer_buffer_size: usize,
) -> Result<usize, JackError> {
    let idx = session.add_port(Port::External(ExternalAudioPort::new(
        name,
        direction,
        ringbuffer_buffer_size,
    )));
    match direction {
        PortDirection::Input => {
            let jp = client.register_port(name, jack::AudioIn::default())?;
            ports.audio_in.push(Bound {
                jack: jp,
                session_idx: idx,
            });
        }
        _ => {
            let jp = client.register_port(name, jack::AudioOut::default())?;
            ports.audio_out.push(Bound {
                jack: jp,
                session_idx: idx,
            });
        }
    }
    Ok(idx)
}

pub fn add_midi_port(
    client: &jack::Client,
    session: &mut Session,
    ports: &mut PortMap,
    name: &str,
    direction: PortDirection,
) -> Result<usize, JackError> {
    let idx = session.add_port(Port::ExternalMidi(ExternalMidiPort::new(name, direction)));
    match direction {
        PortDirection::Input => {
            let jp = client.register_port(name, jack::MidiIn::default())?;
            ports.midi_in.push(Bound {
                jack: jp,
                session_idx: idx,
            });
        }
        _ => {
            let jp = client.register_port(name, jack::MidiOut::default())?;
            ports.midi_out.push(Bound {
                jack: jp,
                session_idx: idx,
            });
        }
    }
    Ok(idx)
}

/// JACK's notification callbacks. Runs off the process thread and has no access to
/// the session, so what it learns is published through atomics.
pub struct Notifications {
    stats: Arc<Stats>,
}

impl jack::NotificationHandler for Notifications {
    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
        self.stats.xruns.fetch_add(1, Ordering::Relaxed);
        jack::Control::Continue
    }
}

/// The process callback. Owns the engine, so nothing else may touch the session.
pub struct Handler {
    engine: Engine,
    ports: PortMap,
}

impl jack::ProcessHandler for Handler {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let n_frames = ps.n_frames() as usize;

        // Stage this cycle's arrivals before the engine runs, because each port's
        // `prepare` is ordered against the channels that read it and picks the staged
        // data up from there.
        for b in &self.ports.audio_in {
            let samples = b.jack.as_slice(ps);
            if let Some(p) = self
                .engine
                .session_mut()
                .port_mut(b.session_idx)
                .and_then(Port::as_external_mut)
            {
                p.stage_input(samples);
            }
        }
        for b in &self.ports.midi_in {
            if let Some(p) = self
                .engine
                .session_mut()
                .port_mut(b.session_idx)
                .and_then(Port::as_external_midi_mut)
            {
                for e in b.jack.iter(ps) {
                    // An oversized message is refused rather than truncated; JACK can
                    // carry sysex this engine's fixed-size elements cannot hold.
                    p.push_incoming(e.time, e.bytes);
                }
            }
        }

        self.engine.process(n_frames);

        for b in &mut self.ports.audio_out {
            let out = b.jack.as_mut_slice(ps);
            match self
                .engine
                .session()
                .port(b.session_idx)
                .and_then(Port::as_external)
            {
                Some(p) => {
                    let produced = p.output(n_frames);
                    let n = produced.len().min(out.len());
                    out[..n].copy_from_slice(&produced[..n]);
                    // A port that produced less than the cycle asked for is silent for
                    // the rest of it, rather than leaving JACK's buffer as it was.
                    for s in &mut out[n..] {
                        *s = 0.0;
                    }
                }
                None => {
                    for s in out.iter_mut() {
                        *s = 0.0;
                    }
                }
            }
        }
        for b in &mut self.ports.midi_out {
            let mut writer = b.jack.writer(ps);
            if let Some(p) = self
                .engine
                .session()
                .port(b.session_idx)
                .and_then(Port::as_external_midi)
            {
                for e in p.outgoing() {
                    // Already ordered by time, which JACK requires of a writer.
                    let _ = writer.write(&jack::RawMidi {
                        time: e.time,
                        bytes: e.data(),
                    });
                }
            }
        }

        jack::Control::Continue
    }

    /// JACK announces a buffer-size change on the process thread, and unlike `process`
    /// this call is allowed to allocate, so the session can be resized in place.
    fn buffer_size(&mut self, _: &jack::Client, size: jack::Frames) -> jack::Control {
        self.engine.session_mut().set_buffer_size(size);
        jack::Control::Continue
    }
}

impl Driver for JackDriver {
    fn sample_rate(&self) -> u32 {
        self.client.as_client().sample_rate()
    }
    fn buffer_size(&self) -> u32 {
        self.client.as_client().buffer_size()
    }
    fn client_name(&self) -> &str {
        self.client.as_client().name()
    }
    fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }
    fn handle(&mut self) -> &mut EngineHandle {
        &mut self.handle
    }
}

/// A running JACK client with the engine on its thread.
pub struct JackDriver {
    client: jack::AsyncClient<Notifications, Handler>,
    handle: EngineHandle,
    stats: Arc<Stats>,
}

/// Starts a JACK client, registers ports via `setup`, then activates.
///
/// Ports have to exist before activation, and after it the session is unreachable
/// except through the returned handle, so `setup` is where a caller builds its graph.
pub fn start<F>(
    client_name: &str,
    session: Session,
    command_queue_capacity: usize,
    setup: F,
) -> Result<JackDriver, JackError>
where
    F: FnOnce(&jack::Client, &mut Session, &mut PortMap) -> Result<(), JackError>,
{
    let (client, _status) = jack::Client::new(client_name, jack::ClientOptions::NO_START_SERVER)?;

    let mut session = session;
    let mut ports = PortMap::default();
    setup(&client, &mut session, &mut ports)?;

    // The schedule is built here rather than on the first callback, so the engine
    // never has to refuse a cycle for a graph it could have had ready.
    session.set_sample_rate(client.sample_rate());
    session.set_buffer_size(client.buffer_size());
    let _ = session.apply_graph_changes();

    let (engine, handle) = crate::engine::split(session, command_queue_capacity);
    let stats = Arc::clone(handle.stats());
    let notifications = Notifications {
        stats: Arc::clone(&stats),
    };
    let client = client.activate_async(notifications, Handler { engine, ports })?;

    Ok(JackDriver {
        client,
        handle,
        stats,
    })
}

impl JackDriver {
    /// Queues control work for the engine. The only way to reach it once running.
    pub fn handle(&mut self) -> &mut EngineHandle {
        &mut self.handle
    }

    pub fn sample_rate(&self) -> u32 {
        self.client.as_client().sample_rate()
    }
    pub fn buffer_size(&self) -> u32 {
        self.client.as_client().buffer_size()
    }
    pub fn client_name(&self) -> &str {
        self.client.as_client().name()
    }

    /// Counters the engine and the driver publish. Includes xruns and DSP load.
    pub fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }

    /// Samples JACK's DSP load into the stats, for a caller that polls.
    ///
    /// Pulled rather than pushed from the callback: asking JACK costs a call this
    /// engine has no reason to make on the audio thread.
    pub fn sample_dsp_load(&self) {
        self.stats
            .set_dsp_load_percent(self.client.as_client().cpu_load());
    }

    // --- external connections ---

    /// External ports matching `pattern`, as JACK names.
    ///
    /// `pattern` is a regular expression, as JACK itself expects, not a glob.
    pub fn find_external_ports(
        &self,
        pattern: Option<&str>,
        flags: jack::PortFlags,
    ) -> Vec<String> {
        self.client.as_client().ports(pattern, None, flags)
    }

    /// Connects one of this client's ports to an external one, by name.
    ///
    /// Already-connected is not an error: a caller reapplying a saved session should
    /// not have to care whether the connection survived.
    pub fn connect(&self, source: &str, destination: &str) -> Result<(), JackError> {
        match self
            .client
            .as_client()
            .connect_ports_by_name(source, destination)
        {
            Ok(()) => Ok(()),
            Err(jack::Error::PortAlreadyConnected(_, _)) => Ok(()),
            Err(e) => Err(JackError::Client(e)),
        }
    }

    /// Disconnects a pair of ports by name. Not-connected is not an error.
    pub fn disconnect(&self, source: &str, destination: &str) -> Result<(), JackError> {
        match self
            .client
            .as_client()
            .disconnect_ports_by_name(source, destination)
        {
            Ok(()) => Ok(()),
            Err(jack::Error::PortDisconnectionError) => Ok(()),
            Err(e) => Err(JackError::Client(e)),
        }
    }

    /// Stops the client and gives the session back.
    pub fn close(self) -> Result<Session, JackError> {
        let (_client, _notify, handler) = self.client.deactivate()?;
        Ok(handler.into_session())
    }
}

impl Handler {
    fn into_session(self) -> Session {
        self.engine.into_session()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PortConnectability, PortDataType};
    use jack::PortSpec;

    fn jack_client(name: &str) -> Option<jack::Client> {
        match jack::Client::new(name, jack::ClientOptions::NO_START_SERVER) {
            Ok((client, _status)) => Some(client),
            Err(e) => {
                eprintln!("skipping JACK integration test; could not open JACK client: {e}");
                None
            }
        }
    }

    #[test]
    fn registers_audio_and_midi_ports_with_expected_session_shape() {
        let Some(client) = jack_client(&format!("shoop-test-register-{}", std::process::id()))
        else {
            return;
        };
        let mut session = Session::default();
        let mut ports = PortMap::default();

        let audio_in = add_audio_port(
            &client,
            &mut session,
            &mut ports,
            "audio_in",
            PortDirection::Input,
            128,
        )
        .expect("audio input port");
        let audio_out = add_audio_port(
            &client,
            &mut session,
            &mut ports,
            "audio_out",
            PortDirection::Output,
            128,
        )
        .expect("audio output port");
        let midi_in = add_midi_port(
            &client,
            &mut session,
            &mut ports,
            "midi_in",
            PortDirection::Input,
        )
        .expect("midi input port");
        let midi_out = add_midi_port(
            &client,
            &mut session,
            &mut ports,
            "midi_out",
            PortDirection::Output,
        )
        .expect("midi output port");

        assert_eq!(ports.audio_in.len(), 1);
        assert_eq!(ports.audio_out.len(), 1);
        assert_eq!(ports.midi_in.len(), 1);
        assert_eq!(ports.midi_out.len(), 1);
        assert_eq!(ports.audio_in[0].session_idx, audio_in);
        assert_eq!(ports.audio_out[0].session_idx, audio_out);
        assert_eq!(ports.midi_in[0].session_idx, midi_in);
        assert_eq!(ports.midi_out[0].session_idx, midi_out);

        let ai = session.port(audio_in).and_then(Port::as_external).unwrap();
        assert_eq!(ai.direction(), PortDirection::Input);
        assert_eq!(ai.data_type(), PortDataType::Audio);
        assert_eq!(ai.input_connectability(), PortConnectability::EXTERNAL);
        assert_eq!(ai.output_connectability(), PortConnectability::INTERNAL);

        let ao = session.port(audio_out).and_then(Port::as_external).unwrap();
        assert_eq!(ao.direction(), PortDirection::Output);
        assert_eq!(ao.data_type(), PortDataType::Audio);
        assert_eq!(ao.input_connectability(), PortConnectability::INTERNAL);
        assert_eq!(ao.output_connectability(), PortConnectability::EXTERNAL);

        let mi = session
            .port(midi_in)
            .and_then(Port::as_external_midi)
            .unwrap();
        assert_eq!(mi.direction(), PortDirection::Input);
        assert_eq!(mi.data_type(), PortDataType::Midi);
        assert_eq!(mi.input_connectability(), PortConnectability::EXTERNAL);
        assert_eq!(mi.output_connectability(), PortConnectability::INTERNAL);

        let mo = session
            .port(midi_out)
            .and_then(Port::as_external_midi)
            .unwrap();
        assert_eq!(mo.direction(), PortDirection::Output);
        assert_eq!(mo.data_type(), PortDataType::Midi);
        assert_eq!(mo.input_connectability(), PortConnectability::INTERNAL);
        assert_eq!(mo.output_connectability(), PortConnectability::EXTERNAL);
    }

    #[test]
    fn registered_jack_ports_are_visible_with_direction_flags() {
        let client_name = format!("shoop-test-flags-{}", std::process::id());
        let Some(client) = jack_client(&client_name) else {
            return;
        };
        let mut session = Session::default();
        let mut ports = PortMap::default();
        add_audio_port(
            &client,
            &mut session,
            &mut ports,
            "audio_in",
            PortDirection::Input,
            0,
        )
        .expect("audio input port");
        add_audio_port(
            &client,
            &mut session,
            &mut ports,
            "audio_out",
            PortDirection::Output,
            0,
        )
        .expect("audio output port");
        add_midi_port(
            &client,
            &mut session,
            &mut ports,
            "midi_in",
            PortDirection::Input,
        )
        .expect("midi input port");
        add_midi_port(
            &client,
            &mut session,
            &mut ports,
            "midi_out",
            PortDirection::Output,
        )
        .expect("midi output port");

        let audio_in_type = jack::AudioIn::default();
        let audio_out_type = jack::AudioOut::default();
        let midi_in_type = jack::MidiIn::default();
        let midi_out_type = jack::MidiOut::default();
        let audio_inputs = client.ports(
            Some(&format!("{client_name}:audio_in")),
            Some(audio_in_type.jack_port_type()),
            jack::PortFlags::IS_INPUT,
        );
        let audio_outputs = client.ports(
            Some(&format!("{client_name}:audio_out")),
            Some(audio_out_type.jack_port_type()),
            jack::PortFlags::IS_OUTPUT,
        );
        let midi_inputs = client.ports(
            Some(&format!("{client_name}:midi_in")),
            Some(midi_in_type.jack_port_type()),
            jack::PortFlags::IS_INPUT,
        );
        let midi_outputs = client.ports(
            Some(&format!("{client_name}:midi_out")),
            Some(midi_out_type.jack_port_type()),
            jack::PortFlags::IS_OUTPUT,
        );

        assert_eq!(audio_inputs, vec![format!("{client_name}:audio_in")]);
        assert_eq!(audio_outputs, vec![format!("{client_name}:audio_out")]);
        assert_eq!(midi_inputs, vec![format!("{client_name}:midi_in")]);
        assert_eq!(midi_outputs, vec![format!("{client_name}:midi_out")]);
    }
}
