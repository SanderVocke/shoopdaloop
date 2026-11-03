use crate::{polling::Polling, Session};
use anyhow::{ensure, Result};
use shoop_dsp_link::{memory::ServiceEndpoint, validate_block, Buffer, Header, MidiEvent};
use shoop_remote_dsp::wire;
use std::{
    io::ErrorKind,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct Stats {
    pub sent: u64,
    pub send_dropped: u64,
    pub received: u64,
    pub invalid: u64,
    pub output_dropped: u64,
    pub last_received_frame: u64,
}
pub struct Adapter {
    pub session: Session,
    pub endpoint: ServiceEndpoint,
    stats: Stats,
    encoded: Vec<u8>,
    raw: Vec<u8>,
    midi: Vec<MidiEvent>,
    reset_remaining: u8,
    reset_until: u64,
    sent_until: u64,
    outstanding_since: Option<Instant>,
}
impl Adapter {
    pub fn prepare(mut session: Session, endpoint: ServiceEndpoint) -> Result<Self> {
        let d = endpoint.descriptor();
        let c = session.config.validate()?;
        ensure!(
            d.generation == session.epoch
                && d.start_frame == c.start_frame
                && d.quantum == c.packet_frames
                && d.dsp_frames as usize == c.dsp().frames
                && d.channels == c.channels
                && d.sample_rate == c.sample_rate
                && d.midi_capacity == c.midi_per_packet,
            "incompatible UDP link descriptor"
        );
        let packet_bytes = c.max_packet_bytes();
        let midi_capacity = c.midi_per_packet as usize;
        ensure!(
            packet_bytes <= 65535 - 48,
            "UDP storage exceeds wire limits"
        );
        if midi_capacity > 16 {
            return Err(anyhow::anyhow!("UDP MIDI capacity exceeds wire limit"));
        }
        let encoded = vec![0; packet_bytes];
        let raw = vec![0; packet_bytes + 1];
        let midi = Vec::with_capacity(midi_capacity);
        session.start()?;
        Ok(Self {
            session,
            endpoint,
            stats: Stats {
                last_received_frame: c.start_frame,
                ..Stats::default()
            },
            encoded,
            raw,
            midi,
            reset_remaining: 0,
            reset_until: c.start_frame,
            sent_until: c.start_frame,
            outstanding_since: None,
        })
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    /// Platform service call, never a render endpoint operation.
    pub fn poll(&mut self) -> Result<bool> {
        let result = self.poll_inner();
        if result.is_err() {
            self.endpoint.set_available(false);
        }
        result
    }
    fn poll_inner(&mut self) -> Result<bool> {
        ensure!(
            self.outstanding_since
                .is_none_or(|since| since.elapsed() < Duration::from_secs(2)),
            "DSP service made no progress for two seconds"
        );
        let mut probe = [0];
        match self.session.tcp.peek(&mut probe) {
            Ok(0) => anyhow::bail!("DSP control disconnected"),
            Ok(_) => anyhow::bail!("unexpected DSP control response"),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
        let d = self.endpoint.descriptor();
        let c = self.session.config;
        let mut progress = false;
        for _ in 0..d.slots {
            let Some(input) = self.endpoint.receive() else {
                break;
            };
            let result = (|| -> Result<()> {
                validate_block(d, &input)?;
                let h = input.header();
                let sequence = (h.frame - d.start_frame) / u64::from(d.quantum);
                self.midi.clear();
                if h.midi_reset && c.midi_per_packet != 0 {
                    self.reset_remaining = 32;
                }
                if self.reset_remaining != 0 {
                    while self.reset_remaining != 0 && self.midi.len() < c.midi_per_packet as usize
                    {
                        let index = 32 - self.reset_remaining;
                        self.midi.push(MidiEvent::new(
                            0,
                            0,
                            &[
                                0xb0 | (index / 2),
                                if index % 2 == 0 { 120 } else { 123 },
                                0,
                            ],
                        )?);
                        self.reset_remaining -= 1;
                    }
                    self.reset_until = h
                        .frame
                        .checked_add(u64::from(d.quantum))
                        .ok_or_else(|| anyhow::anyhow!("frame overflow"))?;
                } else {
                    self.midi.extend_from_slice(input.midi());
                }
                let mut header = wire::Header::input(self.session.epoch, sequence, c);
                header.transport = h.transport;
                let n = wire::encode(header, c, input.audio(), &self.midi, &mut self.encoded)?;
                if self.session.udp.send(&self.encoded[..n]).is_ok() {
                    self.stats.sent += 1;
                    self.outstanding_since.get_or_insert_with(Instant::now);
                    self.sent_until = self.sent_until.max(h.frame + u64::from(d.quantum));
                } else {
                    self.stats.send_dropped += 1;
                }
                Ok(())
            })();
            self.endpoint.release_input(input);
            result?;
            progress = true;
        }
        for _ in 0..d.slots {
            let n = match self.session.udp.recv(&mut self.raw) {
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            };
            progress = true;
            let packet = match wire::decode(&self.raw[..n], c) {
                Ok(p)
                    if p.header.kind == wire::OUTPUT
                        && p.header.epoch == self.session.epoch
                        && p.header
                            .frame
                            .checked_add(u64::from(d.quantum))
                            .is_some_and(|end| end <= self.sent_until) =>
                {
                    p
                }
                _ => {
                    self.stats.invalid += 1;
                    continue;
                }
            };
            let end = packet.header.frame + u64::from(d.quantum);
            let advanced = end > self.stats.last_received_frame;
            self.stats.last_received_frame = self.stats.last_received_frame.max(end);
            if self.stats.last_received_frame >= self.sent_until {
                self.outstanding_since = None;
            } else if advanced {
                self.outstanding_since = Some(Instant::now());
            }
            self.stats.received += 1;
            let Some(mut output) = self.endpoint.acquire_output() else {
                self.stats.output_dropped += 1;
                continue;
            };
            output.set_header(Header {
                generation: self.session.epoch,
                frame: packet.header.frame,
                transport: packet.header.transport,
                concealed: packet.header.flags & wire::CONCEALED != 0,
                midi_reset: packet.header.flags & wire::MIDI_RESET != 0
                    || packet.header.frame < self.reset_until,
            });
            for channel in 0..d.channels as usize {
                for i in 0..d.quantum as usize {
                    output.audio_mut()[channel * d.quantum as usize + i] =
                        packet.sample(channel, i);
                }
            }
            output.clear_midi();
            let mut valid = true;
            for i in 0..packet.midi_len() {
                valid &= output.push_midi(packet.midi(i)).is_ok();
            }
            if !valid {
                let mut h = output.header();
                h.midi_reset = true;
                output.set_header(h);
                output.clear_midi();
            }
            if let Err(output) = self.endpoint.publish(output) {
                self.endpoint.release_output(output);
                self.stats.output_dropped += 1;
            }
        }
        Ok(progress)
    }
    pub fn into_session(self) -> Session {
        self.session
    }
}

pub struct Worker {
    stop: Arc<AtomicBool>,
    received: Arc<AtomicU64>,
    handle: Option<JoinHandle<Result<Adapter>>>,
}
impl Worker {
    pub fn spawn(mut adapter: Adapter, polling: Polling) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let received = Arc::new(AtomicU64::new(adapter.stats.last_received_frame));
        let progress = received.clone();
        let handle = thread::Builder::new()
            .name("dsp-link-network".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    match adapter.poll() {
                        Ok(made_progress) => {
                            if !made_progress {
                                polling.idle();
                            }
                        }
                        Err(error) => {
                            adapter.endpoint.set_available(false);
                            return Err(error);
                        }
                    }
                    progress.store(adapter.stats.last_received_frame, Ordering::Release);
                }
                adapter.endpoint.set_available(false);
                Ok(adapter)
            })?;
        Ok(Self {
            stop,
            received,
            handle: Some(handle),
        })
    }
    /// Test/host coordination only; never call from a processor.
    pub fn wait_until(&self, frame: u64, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while self.received.load(Ordering::Acquire) < frame {
            ensure!(
                Instant::now() < deadline && self.handle.as_ref().is_some_and(|h| !h.is_finished()),
                "DSP worker did not return the requested source range"
            );
            thread::sleep(Duration::from_micros(100));
        }
        Ok(())
    }
    pub fn stop(mut self) -> Result<Adapter> {
        self.stop.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.handle.as_ref().is_some_and(|h| h.is_finished()) {
            ensure!(Instant::now() < deadline, "network worker stop timed out");
            thread::sleep(Duration::from_millis(1));
        }
        self.handle
            .take()
            .expect("live worker handle")
            .join()
            .map_err(|_| anyhow::anyhow!("network worker panicked"))?
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
