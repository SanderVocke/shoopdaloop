use crate::EngineDescription;
use rt_runtime::{buffer::MidiCapacity, Error};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Layout {
    pub enabled: bool,
    pub monitor: MidiCapacity,
    pub playback_merge: MidiCapacity,
    pub global: MidiCapacity,
    pub send: MidiCapacity,
    pub input: MidiCapacity,
    pub mixer: MidiCapacity,
    pub tick: MidiCapacity,
    pub callback: MidiCapacity,
}
impl Layout {
    pub fn new(d: &EngineDescription) -> Result<Self, Error> {
        if d.audio_bench_only {
            return Ok(Self::default());
        }
        let input = MidiCapacity {
            events: d.midi_events,
            bytes: d.midi_bytes,
        };
        let playback = shoop_loops::midi_restore::output_capacity(d.midi_events, d.midi_bytes)?;
        let monitor = input.checked_add(MidiCapacity {
            events: shoop_loops::midi_restore::CLEANUP_MAX_EVENTS,
            bytes: 3 * shoop_loops::midi_restore::CLEANUP_MAX_EVENTS,
        })?;
        let loops = playback.checked_mul(d.loops())?;
        let global = input.checked_mul(d.host.midi_inputs.len())?;
        let send = loops.checked_add(monitor)?.checked_add(global)?;
        let mixer = loops.checked_add(monitor)?;
        let tick = loops.checked_add(monitor.checked_mul(d.strips())?)?;
        let ticks = if d.quantum == 0 {
            return Err(Error::InvalidConfig);
        } else {
            d.max_frames
                .div_ceil(d.quantum)
                .checked_add(1)
                .ok_or(Error::Capacity)?
        };
        let callback = if d.host.midi_outputs.is_empty() {
            MidiCapacity::default()
        } else {
            tick.checked_mul(ticks)?
        };
        // Refuse unsupported portable allocations before preparing the engine.
        let default = input.prepared_bytes()?;
        let total = playback
            .prepared_bytes()?
            .checked_mul(d.loops())
            .and_then(|n| {
                mixer
                    .prepared_bytes()
                    .ok()?
                    .checked_mul(d.strips())?
                    .checked_add(n)
            })
            .and_then(|n| {
                monitor
                    .prepared_bytes()
                    .ok()?
                    .checked_mul(2 * d.strips())?
                    .checked_add(n)
            })
            .and_then(|n| {
                default
                    .checked_mul(2 * d.loops() + 4 * d.strips())?
                    .checked_add(n)
            })
            .and_then(|n| {
                callback
                    .prepared_bytes()
                    .ok()?
                    .checked_mul(d.host.midi_outputs.len())?
                    .checked_add(n)
            })
            .and_then(|n| {
                tick.prepared_bytes()
                    .ok()?
                    .checked_mul(d.host.midi_outputs.len())?
                    .checked_add(n)
            })
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let _ = total;
        Ok(Self {
            enabled: true,
            monitor,
            playback_merge: loops,
            global,
            send,
            input,
            mixer,
            tick,
            callback,
        })
    }
}
impl EngineDescription {
    /// Host output storage includes routed playback and complete MIDI restoration.
    pub fn midi_output_capacity(&self) -> Result<MidiCapacity, Error> {
        Layout::new(self).map(|layout| layout.callback)
    }
}

impl EngineDescription {
    pub fn global_midi_input_capacity(&self) -> Result<MidiCapacity, Error> {
        Layout::new(self).map(|layout| layout.global)
    }
    pub fn processor_midi_input_capacity(&self) -> Result<MidiCapacity, Error> {
        Layout::new(self).map(|layout| layout.send)
    }
}
