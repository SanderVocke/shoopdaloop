//! The engine's structural and control state, kept on the control side. The
//! confirmed model changes only when the runtime reports an activation.

use shoop_rt_protocol::{
    BusControl, LoopSlotId, PortKind, StripControl, StripId, StripPort, StripShape,
    MAX_STRIP_CHANNELS,
};

pub(crate) type Masks = [u64; MAX_STRIP_CHANNELS];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Controls {
    pub input_gain: f32,
    pub input_balance: f32,
    pub monitoring: bool,
    pub output_gain: f32,
    pub output_balance: f32,
    pub output_mute: bool,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            input_gain: 1.,
            input_balance: 0.,
            monitoring: false,
            output_gain: 1.,
            output_balance: 0.,
            output_mute: false,
        }
    }
}
impl Controls {
    pub(crate) fn apply(&mut self, control: StripControl) {
        match control {
            StripControl::InputGain(value) => self.input_gain = value,
            StripControl::InputBalance(value) => self.input_balance = value,
            StripControl::Monitoring(value) => self.monitoring = value,
            StripControl::OutputGain(value) => self.output_gain = value,
            StripControl::OutputBalance(value) => self.output_balance = value,
            StripControl::OutputMute(value) => self.output_mute = value,
        }
    }
    pub(crate) fn all(&self) -> [StripControl; 6] {
        [
            StripControl::InputGain(self.input_gain),
            StripControl::InputBalance(self.input_balance),
            StripControl::Monitoring(self.monitoring),
            StripControl::OutputGain(self.output_gain),
            StripControl::OutputBalance(self.output_balance),
            StripControl::OutputMute(self.output_mute),
        ]
    }
}

/// Audio connections of one strip direction in two banks. The active bank
/// carries the signal; the other is silent and receives the next change.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Banks {
    pub masks: [Masks; 2],
    pub master: [bool; 2],
    pub active: usize,
}
impl Banks {
    pub(crate) fn current(&self) -> &Masks {
        &self.masks[self.active]
    }
    /// Prepare the silent bank as a copy of the active one and make it active.
    fn swap(&mut self) -> &mut Masks {
        let next = 1 - self.active;
        self.masks[next] = self.masks[self.active];
        self.master[next] = self.master[self.active];
        self.active = next;
        &mut self.masks[next]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Strip {
    pub id: StripId,
    pub shape: StripShape,
    pub controls: Controls,
    pub inputs: Banks,
    pub outputs: Banks,
    pub midi_inputs: u64,
    pub midi_outputs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Loop {
    pub id: LoopSlotId,
    pub strip: StripId,
    pub sync: Option<LoopSlotId>,
    pub gain: f32,
    pub balance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bus {
    pub gain: f32,
    pub balance: f32,
    pub mute: bool,
}
impl Default for Bus {
    fn default() -> Self {
        Self {
            gain: 1.,
            balance: 0.,
            mute: false,
        }
    }
}
impl Bus {
    pub(crate) fn apply(&mut self, control: BusControl) {
        match control {
            BusControl::Gain(value) => self.gain = value,
            BusControl::Balance(value) => self.balance = value,
            BusControl::Mute(value) => self.mute = value,
        }
    }
}

/// A structural change, applied as one activation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Change {
    CreateStrip {
        strip: StripId,
        shape: StripShape,
    },
    /// Removes the strip together with its loops.
    RemoveStrip {
        strip: StripId,
    },
    CreateLoop {
        loop_id: LoopSlotId,
        strip: StripId,
    },
    RemoveLoop {
        loop_id: LoopSlotId,
    },
    Sync {
        loop_id: LoopSlotId,
        source: Option<LoopSlotId>,
    },
    /// `host` is the index of the host port within its kind and direction.
    Connect {
        port: StripPort,
        host: usize,
        connected: bool,
    },
    Route {
        strip: StripId,
        connected: bool,
    },
}

pub(crate) struct Model {
    pub strips: Box<[Option<Strip>]>,
    pub loops: Box<[Option<Loop>]>,
    pub strip_generations: Box<[u32]>,
    pub loop_generations: Box<[u32]>,
    pub master: Bus,
    pub revision: u64,
}

impl Model {
    pub(crate) fn new(strips: usize, loops: usize) -> Self {
        Self {
            strips: vec![None; strips].into_boxed_slice(),
            loops: vec![None; loops].into_boxed_slice(),
            strip_generations: vec![0; strips].into_boxed_slice(),
            loop_generations: vec![0; loops].into_boxed_slice(),
            master: Bus::default(),
            revision: 0,
        }
    }
    pub(crate) fn copy_from(&mut self, other: &Self) {
        self.strips.copy_from_slice(&other.strips);
        self.loops.copy_from_slice(&other.loops);
        self.strip_generations
            .copy_from_slice(&other.strip_generations);
        self.loop_generations
            .copy_from_slice(&other.loop_generations);
        self.master = other.master;
        self.revision = other.revision;
    }
    pub(crate) fn strip(&self, id: StripId) -> Option<&Strip> {
        self.strips
            .get(id.0.index as usize)
            .and_then(Option::as_ref)
            .filter(|strip| strip.id == id)
    }
    pub(crate) fn strip_mut(&mut self, id: StripId) -> Option<&mut Strip> {
        self.strips
            .get_mut(id.0.index as usize)
            .and_then(Option::as_mut)
            .filter(|strip| strip.id == id)
    }
    pub(crate) fn find_loop(&self, id: LoopSlotId) -> Option<&Loop> {
        self.loops
            .get(id.0.index as usize)
            .and_then(Option::as_ref)
            .filter(|entry| entry.id == id)
    }
    fn clear_sync_to(&mut self, removed: LoopSlotId) {
        for entry in self.loops.iter_mut().flatten() {
            if entry.sync == Some(removed) {
                entry.sync = None;
            }
        }
    }

    /// Apply a validated change.
    pub(crate) fn apply(&mut self, change: Change) {
        match change {
            Change::CreateStrip { strip, shape } => {
                let slot = strip.0.index as usize;
                self.strip_generations[slot] = strip.0.generation;
                self.strips[slot] = Some(Strip {
                    id: strip,
                    shape,
                    controls: Controls::default(),
                    inputs: Banks::default(),
                    outputs: Banks::default(),
                    midi_inputs: 0,
                    midi_outputs: 0,
                });
            }
            Change::RemoveStrip { strip } => {
                for slot in 0..self.loops.len() {
                    if let Some(entry) = self.loops[slot].filter(|l| l.strip == strip) {
                        self.loops[slot] = None;
                        self.clear_sync_to(entry.id);
                    }
                }
                self.strips[strip.0.index as usize] = None;
            }
            Change::CreateLoop { loop_id, strip } => {
                let slot = loop_id.0.index as usize;
                self.loop_generations[slot] = loop_id.0.generation;
                self.loops[slot] = Some(Loop {
                    id: loop_id,
                    strip,
                    sync: None,
                    gain: 1.,
                    balance: 0.,
                });
            }
            Change::RemoveLoop { loop_id } => {
                self.loops[loop_id.0.index as usize] = None;
                self.clear_sync_to(loop_id);
            }
            Change::Sync { loop_id, source } => {
                if let Some(entry) = self.loops[loop_id.0.index as usize].as_mut() {
                    entry.sync = source;
                }
            }
            Change::Connect {
                port,
                host,
                connected,
            } => {
                let Some(strip) = self.strip_mut(port.strip) else {
                    return;
                };
                let bit = 1u64 << host;
                let set = |mask: &mut u64| {
                    if connected {
                        *mask |= bit;
                    } else {
                        *mask &= !bit;
                    }
                };
                let index = port.index as usize;
                match port.kind {
                    PortKind::AudioIn => set(&mut strip.inputs.swap()[index]),
                    PortKind::AudioOut => set(&mut strip.outputs.swap()[index]),
                    PortKind::MidiIn => set(&mut strip.midi_inputs),
                    PortKind::MidiOut => set(&mut strip.midi_outputs),
                }
            }
            Change::Route { strip, connected } => {
                if let Some(strip) = self.strip_mut(strip) {
                    strip.outputs.swap();
                    let active = strip.outputs.active;
                    strip.outputs.master[active] = connected;
                }
            }
        }
    }

    /// Whether `source` may become the sync source of `target` without a cycle.
    pub(crate) fn sync_allowed(&self, target: LoopSlotId, source: LoopSlotId) -> bool {
        let mut current = Some(source);
        for _ in 0..=self.loops.len() {
            match current {
                None => return true,
                Some(id) if id == target => return false,
                Some(id) => current = self.find_loop(id).and_then(|entry| entry.sync),
            }
        }
        false
    }
}
