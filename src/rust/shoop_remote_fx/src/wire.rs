use rt_buffered::{BufferLimits, MidiLimits, PreparedTiming, Timing};
use rt_runtime::{graph::PortCounts, Error, Quantum};
use serde::{Deserialize, Serialize};

pub(crate) const VERSION: u8 = 1;
pub(crate) const HEADER: usize = 60;
pub(crate) const DATAGRAM: usize = 1200;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaLimits {
    pub channels: u8,
    pub midi: bool,
    pub midi_events: u16,
    pub midi_bytes: u32,
}

impl MediaLimits {
    pub(crate) fn payload(self, quantum: usize) -> usize {
        usize::from(self.channels) * quantum * 4
            + usize::from(self.midi_events) * 4
            + self.midi_bytes as usize
    }

    fn midi(self) -> MidiLimits {
        MidiLimits {
            events_per_quantum: self.midi_events.into(),
            bytes_per_quantum: self.midi_bytes as usize,
        }
    }

    fn validate(self) -> Result<(), Error> {
        if self.channels > 16
            || self.midi_events > 1024
            || self.midi_bytes > 8192
            || (!self.midi && (self.midi_events != 0 || self.midi_bytes != 0))
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub sample_rate: u32,
    pub quantum: u16,
    pub batch_quanta: u16,
    pub slack_quanta: u16,
    pub input: MediaLimits,
    pub output: MediaLimits,
    pub datagram_bytes: u16,
    pub copies: u8,
}

impl Config {
    pub fn new(
        sample_rate: u32,
        timing: PreparedTiming,
        ports: PortCounts,
        limits: BufferLimits,
        copies: u8,
    ) -> Result<Self, Error> {
        fn side(channels: usize, midi: usize, limits: MidiLimits) -> Result<MediaLimits, Error> {
            if midi > 1 {
                return Err(Error::InvalidShape);
            }
            Ok(MediaLimits {
                channels: channels.try_into().map_err(|_| Error::Capacity)?,
                midi: midi == 1,
                midi_events: if midi == 0 {
                    0
                } else {
                    limits
                        .events_per_quantum
                        .try_into()
                        .map_err(|_| Error::Capacity)?
                },
                midi_bytes: if midi == 0 {
                    0
                } else {
                    limits
                        .bytes_per_quantum
                        .try_into()
                        .map_err(|_| Error::Capacity)?
                },
            })
        }

        let config = Self {
            sample_rate,
            quantum: timing
                .quantum()
                .frames()
                .try_into()
                .map_err(|_| Error::Capacity)?,
            batch_quanta: timing
                .timing()
                .batch_quanta
                .try_into()
                .map_err(|_| Error::Capacity)?,
            slack_quanta: timing
                .timing()
                .slack_quanta
                .try_into()
                .map_err(|_| Error::Capacity)?,
            input: side(ports.audio_inputs, ports.midi_inputs, limits.midi_input)?,
            output: side(ports.audio_outputs, ports.midi_outputs, limits.midi_output)?,
            datagram_bytes: DATAGRAM as u16,
            copies,
        };
        config.prepare()?;
        Ok(config)
    }

    pub fn prepare(self) -> Result<PreparedTiming, Error> {
        self.input.validate()?;
        self.output.validate()?;
        if !(8000..=384_000).contains(&self.sample_rate)
            || !(1..=2).contains(&self.copies)
            || !(256..=DATAGRAM).contains(&usize::from(self.datagram_bytes))
        {
            return Err(Error::InvalidConfig);
        }
        let timing = Timing {
            batch_quanta: self.batch_quanta.into(),
            slack_quanta: self.slack_quanta.into(),
        }
        .prepare(Quantum::new(usize::from(self.quantum))?)?;
        if timing.batch_frames() > 8192 {
            return Err(Error::Capacity);
        }
        self.limits()
            .async_storage_bytes_for(self.ports(), timing.quantum(), timing.timing())?;
        Ok(timing)
    }

    pub fn ports(self) -> PortCounts {
        PortCounts {
            audio_inputs: self.input.channels.into(),
            audio_outputs: self.output.channels.into(),
            midi_inputs: usize::from(self.input.midi),
            midi_outputs: usize::from(self.output.midi),
        }
    }

    pub fn limits(self) -> BufferLimits {
        BufferLimits {
            midi_input: self.input.midi(),
            midi_output: self.output.midi(),
            parameters_per_quantum: 0,
        }
    }

    pub(crate) fn fragment_bytes(self) -> usize {
        usize::from(self.datagram_bytes) - HEADER
    }

    pub fn media_bytes(self) -> usize {
        let quantum = usize::from(self.quantum);
        let payload = self
            .input
            .payload(quantum)
            .saturating_add(self.output.payload(quantum));
        let slots = usize::from(self.slack_quanta)
            .div_ceil(usize::from(self.batch_quanta).max(1))
            .saturating_add(2);
        // Include fragment markers, working buffers, and per-quantum metadata.
        slots
            .saturating_add(2)
            .saturating_mul(usize::from(self.batch_quanta))
            .saturating_mul(payload.saturating_mul(2).saturating_add(256))
            .saturating_add(2 * DATAGRAM)
    }
}
