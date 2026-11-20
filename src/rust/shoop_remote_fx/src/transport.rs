use crate::{
    media::{Assembly, Encoder, Fragment, Receipt, MAX_DATAGRAMS_PER_POLL},
    wire::{Config, DATAGRAM},
};
use rt_buffered::{AsyncBatch, AsyncOutput, LocalJob, LocalTransport, LocalWorker};
use rt_runtime::{buffer::MidiBuffer, Error, Frame};
use std::{
    io,
    net::UdpSocket,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Counters {
    pub sent_datagrams: u64,
    pub received_datagrams: u64,
    pub sent_batches: u64,
    pub completed_batches: u64,
    pub missed_batches: u64,
    pub duplicate_fragments: u64,
    pub invalid_fragments: u64,
    pub stale_fragments: u64,
    pub queue_full: u64,
    pub peak_pending: usize,
    pub peak_round_trip_us: u64,
    pub peak_processing_us: u64,
}

fn midi_buffers(format: crate::MediaLimits, batches: usize) -> Result<Vec<MidiBuffer>, Error> {
    let mut midi = Vec::with_capacity(usize::from(format.midi));
    if format.midi {
        midi.push(MidiBuffer::new(
            usize::from(format.midi_events) * batches,
            format.midi_bytes as usize * batches,
        )?);
    }
    Ok(midi)
}

pub fn transport_bytes(config: Config) -> Result<usize, Error> {
    let timing = config.prepare()?;
    LocalTransport::prepared_bytes(config.ports(), timing, config.limits(), timing.slots())?
        .checked_add(config.media_bytes())
        .ok_or(Error::Capacity)
}

struct Pending {
    job: Option<(LocalJob, Instant)>,
    assembly: Assembly,
}

pub struct ClientMedia {
    config: Config,
    worker: LocalWorker,
    pending: Vec<Pending>,
    outgoing: Encoder,
    generation: Option<(u64, u64)>,
    timeout: Duration,
    counters: Counters,
}

impl ClientMedia {
    pub fn prepare(config: Config) -> Result<(LocalTransport, Self), Error> {
        let timing = config.prepare()?;
        let slots = timing.slots();
        let (endpoint, worker) =
            LocalTransport::prepare(config.ports(), timing, config.limits(), slots)?;
        Ok((
            endpoint,
            Self {
                config,
                worker,
                pending: (0..slots)
                    .map(|_| Pending {
                        job: None,
                        assembly: Assembly::new(config, config.output),
                    })
                    .collect(),
                outgoing: Encoder::new(config, config.input),
                generation: None,
                timeout: Duration::from_secs_f64(
                    timing.latency_frames() as f64 / f64::from(config.sample_rate),
                )
                .max(Duration::from_millis(2)),
                counters: Counters::default(),
            },
        ))
    }

    pub fn counters(&self) -> Counters {
        self.counters
    }

    pub fn disconnect(&mut self) {
        self.generation = None;
        for pending in &mut self.pending {
            if let Some((job, _)) = pending.job.take() {
                let _ = self.worker.finish(job, Err(Error::Disconnected));
            }
            pending.assembly.reset();
        }
        for _ in 0..self.pending.len() {
            let Some(job) = self.worker.try_start() else {
                break;
            };
            let _ = self.worker.finish(job, Err(Error::Disconnected));
        }
    }

    pub fn poll(&mut self, socket: &UdpSocket, stream: u128) -> io::Result<()> {
        let mut packet = [0; DATAGRAM + 1];
        for _ in 0..MAX_DATAGRAMS_PER_POLL {
            let size = match socket.recv(&mut packet) {
                Ok(size) => size,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                // Windows reports an oversized UDP datagram as WSAEMSGSIZE.
                Err(error) if error.raw_os_error() == Some(10040) => continue,
                Err(error) => return Err(error),
            };
            self.counters.received_datagrams += 1;
            let Some(fragment) = Fragment::parse(&packet[..size], stream, self.config) else {
                self.counters.invalid_fragments += 1;
                continue;
            };
            let tag = fragment.tag();
            let Some(pending) = self.pending.iter_mut().find(|pending| {
                pending
                    .job
                    .as_ref()
                    .is_some_and(|(job, _)| job.input().tag == tag)
            }) else {
                self.counters.stale_fragments += 1;
                continue;
            };
            let now = Instant::now();
            match pending.assembly.push(fragment, now) {
                Receipt::Invalid => self.counters.invalid_fragments += 1,
                Receipt::Duplicate => self.counters.duplicate_fragments += 1,
                Receipt::Pending => {}
                Receipt::Complete => {
                    if let Some((mut job, submitted)) = pending.job.take() {
                        let (audio, midi) = job.output_mut();
                        let result = pending.assembly.decode(audio, midi);
                        let _ = self.worker.finish(job, result);
                        pending.assembly.reset();
                        self.counters.completed_batches += u64::from(result.is_ok());
                        self.counters.peak_round_trip_us = self
                            .counters
                            .peak_round_trip_us
                            .max((now - submitted).as_micros() as u64);
                    }
                }
            }
        }

        for pending in &mut self.pending {
            if pending
                .job
                .as_ref()
                .is_some_and(|(_, start)| start.elapsed() >= self.timeout)
            {
                if let Some((job, _)) = pending.job.take() {
                    let _ = self.worker.finish(job, Err(Error::Incomplete));
                }
                pending.assembly.reset();
                self.counters.missed_batches += 1;
            }
        }

        for _ in 0..self.pending.len() {
            let Some(job) = self.worker.try_start() else {
                break;
            };
            let input = job.input();
            let tag = input.tag;
            let first_sequence = self
                .generation
                .filter(|(generation, _)| *generation == tag.generation)
                .map_or(tag.sequence, |(_, first)| first);
            self.generation = Some((tag.generation, first_sequence));
            let Some(pending) = self
                .pending
                .iter_mut()
                .find(|pending| pending.job.is_none())
            else {
                self.counters.queue_full += 1;
                let _ = self.worker.finish(job, Err(Error::Full));
                continue;
            };
            let submitted = Instant::now();
            let sent = self.outgoing.send(
                socket,
                stream,
                tag,
                first_sequence,
                AsyncOutput {
                    frames: input.frames,
                    audio: input.audio,
                    midi: input.midi,
                },
            );
            match sent {
                Ok(datagrams) => self.counters.sent_datagrams += datagrams,
                Err(error) => {
                    let _ = self.worker.finish(job, Err(Error::Incomplete));
                    if error.kind() != io::ErrorKind::InvalidData {
                        return Err(error);
                    }
                    self.counters.invalid_fragments += 1;
                    continue;
                }
            }
            pending.job = Some((job, submitted));
            self.counters.sent_batches += 1;
        }
        let pending = self
            .pending
            .iter()
            .filter(|pending| pending.job.is_some())
            .count();
        self.counters.peak_pending = self.counters.peak_pending.max(pending);
        Ok(())
    }
}

pub trait Dsp: Send {
    fn process(
        &mut self,
        input: AsyncBatch<'_>,
        audio: &mut [f32],
        midi: &mut [MidiBuffer],
    ) -> Result<(), Error>;
}

struct Generation {
    id: u64,
    first: u64,
    next: u64,
    origin: u64,
}

pub struct ServerMedia {
    config: Config,
    frames: usize,
    reorder: Duration,
    incoming: Vec<Assembly>,
    generation: Option<Generation>,
    outgoing: Encoder,
    audio_input: Vec<f32>,
    audio_output: Vec<f32>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
    counters: Counters,
}

impl ServerMedia {
    pub fn prepare(config: Config) -> Result<Self, Error> {
        let timing = config.prepare()?;
        let frames = timing.batch_frames();
        let batches = usize::from(config.batch_quanta);
        Ok(Self {
            config,
            frames,
            reorder: Duration::from_secs_f64(frames as f64 / f64::from(config.sample_rate) / 2.0)
                .clamp(Duration::from_micros(300), Duration::from_millis(5)),
            incoming: (0..timing.slots())
                .map(|_| Assembly::new(config, config.input))
                .collect(),
            generation: None,
            outgoing: Encoder::new(config, config.output),
            audio_input: vec![0.; frames * usize::from(config.input.channels)],
            audio_output: vec![0.; frames * usize::from(config.output.channels)],
            midi_input: midi_buffers(config.input, batches)?,
            midi_output: midi_buffers(config.output, batches)?,
            counters: Counters::default(),
        })
    }

    pub fn counters(&self) -> Counters {
        self.counters
    }

    fn receive(&mut self, fragment: Fragment<'_>, now: Instant) {
        let tag = fragment.tag();
        if let Some(generation) = &self.generation {
            if tag.generation < generation.id {
                self.counters.stale_fragments += 1;
                return;
            }
        }
        let first = fragment.first_sequence;
        if self
            .generation
            .as_ref()
            .is_none_or(|generation| generation.id != tag.generation)
        {
            let Some(origin) = (tag.sequence - first)
                .checked_mul(self.frames as u64)
                .and_then(|delta| tag.source.0.checked_sub(delta))
            else {
                self.counters.invalid_fragments += 1;
                return;
            };
            self.generation = Some(Generation {
                id: tag.generation,
                first,
                next: first,
                origin,
            });
            for pending in &mut self.incoming {
                pending.reset();
            }
        }
        let generation = self.generation.as_mut().unwrap();
        if tag.sequence < generation.next {
            self.counters.stale_fragments += 1;
            return;
        }
        let source = (tag.sequence - generation.first)
            .checked_mul(self.frames as u64)
            .and_then(|offset| generation.origin.checked_add(offset))
            .map(Frame);
        if generation.first != first || source != Some(tag.source) || tag.sequence == u64::MAX {
            self.counters.invalid_fragments += 1;
            return;
        }
        let window = self.incoming.len();
        if tag.sequence - generation.next >= window as u64 {
            self.counters.missed_batches = self
                .counters
                .missed_batches
                .saturating_add(tag.sequence - generation.next);
            generation.next = tag.sequence;
            for pending in &mut self.incoming {
                pending.reset();
            }
        }
        let pending = &mut self.incoming[(tag.sequence % window as u64) as usize];
        match pending.push(fragment, now) {
            Receipt::Invalid => self.counters.invalid_fragments += 1,
            Receipt::Duplicate => self.counters.duplicate_fragments += 1,
            Receipt::Pending | Receipt::Complete => {}
        }
    }

    pub fn poll(&mut self, socket: &UdpSocket, stream: u128, dsp: &mut dyn Dsp) -> io::Result<()> {
        let mut datagram = [0; DATAGRAM + 1];
        for _ in 0..MAX_DATAGRAMS_PER_POLL {
            let size = match socket.recv(&mut datagram) {
                Ok(size) => size,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.raw_os_error() == Some(10040) => continue,
                Err(error) => return Err(error),
            };
            if size == 16 && datagram[..size] == stream.to_le_bytes() {
                let _ = socket.send(&datagram[..size]);
            } else if let Some(fragment) = Fragment::parse(&datagram[..size], stream, self.config) {
                self.counters.received_datagrams += 1;
                self.receive(fragment, Instant::now());
            } else {
                self.counters.invalid_fragments += 1;
            }
        }

        let window = self.incoming.len();
        let Some(generation) = self.generation.as_mut() else {
            return Ok(());
        };
        self.counters.peak_pending = self.counters.peak_pending.max(
            self.incoming
                .iter()
                .filter(|pending| pending.tag().is_some())
                .count(),
        );
        for _ in 0..window {
            let index = (generation.next % window as u64) as usize;
            let pending = &self.incoming[index];
            if !pending.is_complete() {
                let expired = self
                    .incoming
                    .iter()
                    .filter_map(Assembly::started)
                    .min()
                    .is_some_and(|since| since.elapsed() >= self.reorder);
                if !expired {
                    break;
                }
                self.incoming[index].reset();
                self.counters.missed_batches += 1;
                generation.next += 1;
                continue;
            }
            let tag = pending.tag().unwrap();
            let result = pending.decode(&mut self.audio_input, &mut self.midi_input);
            self.incoming[index].reset();
            generation.next += 1;
            self.audio_output.fill(0.);
            for midi in &mut self.midi_output {
                midi.clear();
            }
            let input = AsyncBatch {
                tag,
                frames: self.frames,
                audio: &self.audio_input,
                midi: &self.midi_input,
                parameters: &[],
            };
            let started = Instant::now();
            if result
                .and_then(|()| dsp.process(input, &mut self.audio_output, &mut self.midi_output))
                .is_err()
            {
                self.counters.missed_batches += 1;
                continue;
            }
            self.counters.peak_processing_us = self
                .counters
                .peak_processing_us
                .max(started.elapsed().as_micros() as u64);
            self.counters.completed_batches += 1;
            let sent = self.outgoing.send(
                socket,
                stream,
                tag,
                generation.first,
                AsyncOutput {
                    frames: self.frames,
                    audio: &self.audio_output,
                    midi: &self.midi_output,
                },
            );
            match sent {
                Ok(datagrams) => {
                    self.counters.sent_batches += 1;
                    self.counters.sent_datagrams += datagrams;
                }
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    self.counters.invalid_fragments += 1;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaLimits;
    use rt_buffered::BatchTag;
    use shoop_wasm_test_support::shoop_test;

    #[derive(Default)]
    struct Meter(Vec<BatchTag>);

    impl Dsp for Meter {
        fn process(
            &mut self,
            input: AsyncBatch<'_>,
            audio: &mut [f32],
            _midi: &mut [MidiBuffer],
        ) -> Result<(), Error> {
            self.0.push(input.tag);
            audio.fill(input.audio[0] * 2.);
            Ok(())
        }
    }

    #[shoop_test]
    fn expired_input_is_skipped_and_late_copies_do_not_execute_dsp() {
        let side = |channels| MediaLimits {
            channels,
            midi: false,
            midi_events: 0,
            midi_bytes: 0,
        };
        let config = Config {
            sample_rate: 48_000,
            quantum: 32,
            batch_quanta: 2,
            slack_quanta: 4,
            input: side(16),
            output: side(1),
            datagram_bytes: DATAGRAM as u16,
            copies: 2,
        };
        let mut media = ServerMedia::prepare(config).unwrap();
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.connect(peer.local_addr().unwrap()).unwrap();
        socket.set_nonblocking(true).unwrap();
        let mut meter = Meter::default();
        let mut encoder = Encoder::new(config, config.input);
        let mut add = |media: &mut ServerMedia, generation, sequence, allowed: usize| {
            let tag = BatchTag {
                generation,
                sequence,
                source: Frame(sequence * 64),
            };
            let mut sent = 0;
            encoder
                .encode(
                    42,
                    tag,
                    0,
                    AsyncOutput {
                        frames: 64,
                        audio: &[0.25; 1024],
                        midi: &[],
                    },
                    |bytes| {
                        if sent < allowed {
                            media.receive(
                                Fragment::parse(bytes, 42, config).unwrap(),
                                Instant::now() - Duration::from_millis(2),
                            );
                        }
                        sent += 1;
                        Ok(())
                    },
                )
                .unwrap();
        };
        add(&mut media, 1, 0, 2);
        add(&mut media, 1, 2, usize::MAX);
        add(&mut media, 1, 1, usize::MAX);
        media.poll(&socket, 42, &mut meter).unwrap();
        assert_eq!(
            meter.0.iter().map(|tag| tag.sequence).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(media.counters().missed_batches, 1);
        add(&mut media, 1, 0, usize::MAX);
        add(&mut media, 2, 0, usize::MAX);
        add(&mut media, 1, 3, usize::MAX);
        media.poll(&socket, 42, &mut meter).unwrap();
        assert_eq!(meter.0.len(), 3);
        assert_eq!(meter.0[2].generation, 2);
        assert!(media.counters().stale_fragments > 0);
        assert!(media.counters().duplicate_fragments > 0);
    }
}
