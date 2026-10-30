//! The main thread's half of the browser host, without browser bindings: the
//! record port the protocol client posts into, the MIDI-in lane batch, the
//! conversion between Web MIDI timestamps and `currentFrame` frames, and the
//! host port names of a Web Audio and Web MIDI setup.
//!
//! The browser glue posts what these collect to the processor's `MessagePort`
//! once per main-thread turn and feeds them what the processor posts back.

use std::collections::BTreeMap;

use crate::framing::{self, FramingError};
use crate::lane::MidiEvent;
use shoop_rt_engine::{HostPorts, MAX_HOST_PORTS};
use shoop_rt_protocol::carrier::{CarrierError, RecordPort};
use shoop_rt_protocol::{codec, DisconnectReason, EngineMessage, Name, MAX_NAME_BYTES};

/// The client's record port over the processor's `MessagePort`. Posted records
/// collect into one outgoing batch; received batches are split into records.
///
/// When the worklet is gone, posting fails and the client reads
/// `Disconnected` after the records that had arrived.
#[derive(Debug, Default)]
pub struct RecordBatches {
    outgoing: Vec<u8>,
    incoming: Vec<u8>,
    read: usize,
    closed: Option<DisconnectReason>,
    close_delivered: bool,
}

impl RecordBatches {
    pub fn new() -> Self {
        Self::default()
    }

    /// The records posted since the last call, as one batch.
    pub fn take_outgoing(&mut self) -> Option<Vec<u8>> {
        (!self.outgoing.is_empty()).then(|| std::mem::take(&mut self.outgoing))
    }

    /// Append a batch the processor posted.
    pub fn receive(&mut self, batch: &[u8]) {
        if self.closed.is_some() {
            return;
        }
        if self.read > 0 {
            self.incoming.drain(..self.read);
            self.read = 0;
        }
        self.incoming.extend_from_slice(batch);
    }

    /// End the connection from the carrier's side, for example because the
    /// processor failed or the audio graph closed.
    pub fn close(&mut self, reason: DisconnectReason) {
        if self.closed.is_none() {
            self.closed = Some(reason);
            self.outgoing.clear();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.is_some()
    }

    fn deliver_close(&mut self, buf: &mut [u8]) -> Option<usize> {
        let reason = self.closed?;
        if self.close_delivered {
            return None;
        }
        self.close_delivered = true;
        codec::encode_engine(&EngineMessage::Disconnected(reason), buf).ok()
    }
}

impl RecordPort for RecordBatches {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        if self.closed.is_some() {
            return Err(CarrierError::Disconnected);
        }
        self.outgoing.extend_from_slice(record);
        Ok(())
    }

    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let pending = &self.incoming[self.read..];
        match framing::record_len(pending) {
            Ok(Some(len)) if len <= buf.len() => {
                buf[..len].copy_from_slice(&pending[..len]);
                self.read += len;
                if self.read == self.incoming.len() {
                    self.incoming.clear();
                    self.read = 0;
                }
                Some(len)
            }
            Ok(None) => self.deliver_close(buf),
            Ok(Some(_)) | Err(FramingError) => {
                self.incoming.clear();
                self.read = 0;
                self.close(DisconnectReason::ProtocolError);
                self.deliver_close(buf)
            }
        }
    }
}

/// MIDI-in lane events for the next post, bounded so a flood of input cannot
/// queue without limit.
#[derive(Debug)]
pub struct MidiInBatch {
    bytes: Vec<u8>,
    capacity: usize,
    dropped: u64,
}

impl MidiInBatch {
    pub fn new(capacity: usize) -> Self {
        Self {
            bytes: Vec::new(),
            capacity,
            dropped: 0,
        }
    }

    /// Append an event; false if it does not fit or is invalid, which is counted.
    pub fn push(&mut self, event: MidiEvent<'_>) -> bool {
        let start = self.bytes.len();
        let len = crate::lane::EVENT_HEADER_BYTES + event.bytes.len();
        if start + len > self.capacity {
            self.dropped += 1;
            return false;
        }
        self.bytes.resize(start + len, 0);
        if event.encode(&mut self.bytes[start..]).is_none() {
            self.bytes.truncate(start);
            self.dropped += 1;
            return false;
        }
        true
    }

    pub fn take(&mut self) -> Option<Vec<u8>> {
        (!self.bytes.is_empty()).then(|| std::mem::take(&mut self.bytes))
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// The relation between the performance clock of Web MIDI timestamps, in
/// milliseconds, and the AudioContext's timeline, whose frames the worklet sees
/// as `currentFrame`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidiClock {
    /// Context time being heard at `performance_ms`, as from
    /// `AudioContext.getOutputTimestamp()`.
    pub context_seconds: f64,
    pub performance_ms: f64,
    pub sample_rate: f64,
    /// How far rendering runs ahead of what is heard (`baseLatency` plus
    /// `outputLatency`).
    pub latency_seconds: f64,
}

impl MidiClock {
    /// The frame for a MIDI input event received at `event_ms`: the frame being
    /// rendered at that moment plus `lead_frames`, so the event reaches the
    /// worklet before its quantum.
    pub fn input_frame(&self, event_ms: f64, lead_frames: u64) -> u64 {
        let seconds =
            self.context_seconds + (event_ms - self.performance_ms) / 1000.0 + self.latency_seconds;
        let frame = (seconds * self.sample_rate).round();
        let frame = if frame.is_finite() && frame > 0.0 {
            frame as u64
        } else {
            0
        };
        frame.saturating_add(lead_frames)
    }

    /// The performance time at which `frame` is heard: when a MIDI output event
    /// rendered at `frame` should be sent.
    pub fn output_time_ms(&self, frame: u64) -> f64 {
        self.performance_ms + (frame as f64 / self.sample_rate - self.context_seconds) * 1000.0
    }
}

/// Host port names for Web Audio capture and destination channels and Web MIDI
/// inputs and outputs: `webaudio:capture_N`, `webaudio:destination_N` and
/// `webmidi:<port> in` or `out`. Port names are shortened to fit and made unique;
/// ports beyond [`MAX_HOST_PORTS`] per kind are left out.
pub fn web_host_ports(
    capture: usize,
    destination: usize,
    midi_inputs: &[&str],
    midi_outputs: &[&str],
) -> HostPorts {
    let numbered = |prefix: &str, count: usize| {
        (1..=count.min(MAX_HOST_PORTS))
            .map(|index| Name::new(&format!("webaudio:{prefix}_{index}")).expect("short name"))
            .collect()
    };
    let mut used = BTreeMap::new();
    let mut midi = |ports: &[&str], suffix: &str| {
        ports
            .iter()
            .take(MAX_HOST_PORTS)
            .map(|port| unique_name(port, suffix, &mut used))
            .collect()
    };
    HostPorts {
        audio_inputs: numbered("capture", capture),
        audio_outputs: numbered("destination", destination),
        midi_inputs: midi(midi_inputs, " in"),
        midi_outputs: midi(midi_outputs, " out"),
    }
}

fn unique_name(port: &str, suffix: &str, used: &mut BTreeMap<String, u32>) -> Name {
    let base = port
        .split(|c: char| c.is_control() || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let base = if base.is_empty() { "port" } else { &base };
    for attempt in 1.. {
        let tail = if attempt == 1 {
            suffix.to_owned()
        } else {
            format!(" {attempt}{suffix}")
        };
        let name = fitted("webmidi:", base, &tail);
        let count = used.entry(name.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            return Name::new(&name).expect("fitted name");
        }
    }
    unreachable!("an unused name exists")
}

/// `prefix`, `base` and `tail`, with `base` shortened to fit a port name.
fn fitted(prefix: &str, base: &str, tail: &str) -> String {
    let room = MAX_NAME_BYTES.saturating_sub(prefix.len() + tail.len());
    let mut end = base.len().min(room);
    while !base.is_char_boundary(end) {
        end -= 1;
    }
    format!("{prefix}{}{tail}", base[..end].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_rt_protocol::carrier::{Carrier, RecordClient};
    use shoop_rt_protocol::{ClientMessage, MAX_RECORD_BYTES};
    use shoop_wasm_test_support::shoop_test;

    fn engine_record(message: EngineMessage) -> Vec<u8> {
        let mut buf = [0; MAX_RECORD_BYTES];
        let len = codec::encode_engine(&message, &mut buf).unwrap();
        buf[..len].to_vec()
    }

    #[shoop_test]
    fn posted_records_leave_as_one_batch() {
        let mut client = RecordClient::new(RecordBatches::new());
        assert_eq!(client.port_mut().take_outgoing(), None);
        client.send(ClientMessage::Hello { version: 1 }).unwrap();
        client.send(ClientMessage::Resync).unwrap();
        let batch = client.port_mut().take_outgoing().unwrap();
        let records = framing::records(&batch)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(
            codec::decode_client(records[0]).unwrap(),
            ClientMessage::Hello { version: 1 }
        );
        assert_eq!(
            codec::decode_client(records[1]).unwrap(),
            ClientMessage::Resync
        );
        assert_eq!(client.port_mut().take_outgoing(), None);
    }

    #[shoop_test]
    fn received_batches_split_into_records_across_posts() {
        let mut client = RecordClient::new(RecordBatches::new());
        let first = engine_record(EngineMessage::Disconnected(DisconnectReason::Shutdown));
        let second = engine_record(EngineMessage::Disconnected(DisconnectReason::Fault));
        let mut stream = first.clone();
        stream.extend_from_slice(&second);
        let (head, tail) = stream.split_at(first.len() + 1);
        client.port_mut().receive(head);
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Shutdown))
        );
        assert_eq!(client.receive(), None);
        client.port_mut().receive(tail);
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Fault))
        );
        assert_eq!(client.receive(), None);
    }

    #[shoop_test]
    fn closing_delivers_received_records_then_one_disconnect() {
        let mut client = RecordClient::new(RecordBatches::new());
        client.send(ClientMessage::Resync).unwrap();
        client
            .port_mut()
            .receive(&engine_record(EngineMessage::Disconnected(
                DisconnectReason::Shutdown,
            )));
        client.port_mut().close(DisconnectReason::Fault);
        assert!(client.port_mut().is_closed());
        assert_eq!(client.port_mut().take_outgoing(), None);
        assert_eq!(
            client.send(ClientMessage::Resync),
            Err(CarrierError::Disconnected)
        );
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Shutdown))
        );
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Fault))
        );
        assert_eq!(client.receive(), None);
        client.port_mut().receive(&[4, 0, 0, 0]);
        assert_eq!(client.receive(), None);
    }

    #[shoop_test]
    fn an_invalid_length_ends_the_connection_with_a_protocol_error() {
        let mut client = RecordClient::new(RecordBatches::new());
        client.port_mut().receive(&[1, 0, 9]);
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::ProtocolError))
        );
        assert_eq!(client.receive(), None);
        assert!(client.port_mut().is_closed());
    }

    #[shoop_test]
    fn the_midi_in_batch_encodes_lane_events_within_its_capacity() {
        let mut batch = MidiInBatch::new(26);
        let note = MidiEvent {
            frame: 300,
            port: 1,
            bytes: &[0x90, 60, 100],
        };
        assert!(batch.push(note));
        assert!(batch.push(MidiEvent { frame: 301, ..note }));
        assert!(!batch.push(MidiEvent { frame: 302, ..note }));
        assert!(!batch.push(MidiEvent { bytes: &[], ..note }));
        assert_eq!(batch.dropped(), 2);
        let bytes = batch.take().unwrap();
        let events = crate::lane::events(&bytes)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(events, [note, MidiEvent { frame: 301, ..note }]);
        assert_eq!(batch.take(), None);
        assert!(batch.push(MidiEvent { frame: 302, ..note }));
    }

    fn clock() -> MidiClock {
        MidiClock {
            context_seconds: 2.0,
            performance_ms: 10_000.0,
            sample_rate: 48_000.0,
            latency_seconds: 0.01,
        }
    }

    #[shoop_test]
    fn web_midi_timestamps_become_frames_ahead_of_rendering() {
        let clock = clock();
        // Heard at 2 s; rendered 10 ms ahead.
        assert_eq!(clock.input_frame(10_000.0, 0), 96_480);
        assert_eq!(clock.input_frame(10_000.0, 256), 96_736);
        // 5 ms earlier.
        assert_eq!(clock.input_frame(9_995.0, 0), 96_240);
        // Before the context started.
        assert_eq!(clock.input_frame(0.0, 128), 128);
        assert_eq!(clock.input_frame(f64::NAN, 1), 1);
    }

    #[shoop_test]
    fn output_frames_are_sent_when_they_are_heard() {
        let clock = clock();
        assert_eq!(clock.output_time_ms(96_000), 10_000.0);
        assert_eq!(clock.output_time_ms(96_480), 10_010.0);
        let frame = clock.input_frame(10_020.0, 0);
        assert_eq!(clock.output_time_ms(frame), 10_030.0);
    }

    #[shoop_test]
    fn web_host_port_names_are_unique_and_fit() {
        let long = "é".repeat(40);
        let ports = web_host_ports(
            1,
            2,
            &["Arturia: KeyStep", "Arturia:  KeyStep\n", &long],
            &["Arturia: KeyStep", ""],
        );
        let names = |list: &[Name]| {
            list.iter()
                .map(|n| n.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&ports.audio_inputs), ["webaudio:capture_1"]);
        assert_eq!(
            names(&ports.audio_outputs),
            ["webaudio:destination_1", "webaudio:destination_2"]
        );
        let inputs = names(&ports.midi_inputs);
        assert_eq!(inputs[0], "webmidi:Arturia: KeyStep in");
        assert_eq!(inputs[1], "webmidi:Arturia: KeyStep 2 in");
        assert!(inputs[2].len() <= MAX_NAME_BYTES && inputs[2].ends_with(" in"));
        assert_eq!(
            names(&ports.midi_outputs),
            ["webmidi:Arturia: KeyStep out", "webmidi:port out"]
        );
        let many = vec!["x"; MAX_HOST_PORTS + 3];
        let ports = web_host_ports(0, 0, &many, &[]);
        assert_eq!(ports.midi_inputs.len(), MAX_HOST_PORTS);
        let unique = names(&ports.midi_inputs)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), MAX_HOST_PORTS);
    }
}
