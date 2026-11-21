//! Prepared output/channel directories and snapshot comparison maps.
use super::performance::{self as p, Bits, RestoreIo, State};
use super::{ContentStore, Error, Message, OutputEvent, ViewId};
use alloc::{boxed::Box, vec, vec::Vec};

#[derive(Clone, Copy, Debug)]
pub struct StateLimits {
    pub inputs: u16,
    pub outputs: u16,
    pub sequences: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateBinding {
    pub owner: crate::records::Identity,
    pub input: u16,
    pub outputs: u8,
    pub channels: u16,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct StateWork {
    pub tracker_visits: u64,
    pub compared_fields: u64,
    pub changed_bits: u64,
    pub restored_messages: u64,
    pub prefix_events: u64,
}
#[derive(Clone, Copy, Default)]
struct Activity {
    keys: [u128; 16],
    pedals: [u8; 16],
}
struct Tracker {
    target: Option<ViewId>,
    bits: Bits,
    activity: Activity,
}
pub struct Expression {
    pub limits: StateLimits,
    pub work: StateWork,
    inputs: Box<[State]>,
    outputs: Box<[State]>,
    bindings: Box<[Option<StateBinding>]>,
    trackers: Box<[Tracker]>,
    directory: Box<[u128]>,
    policies: Box<[u128]>,
    scratch: Option<State>,
}
impl Expression {
    pub fn prepare(limits: StateLimits) -> Result<Self, Error> {
        if limits.inputs == 0
            || limits.outputs == 0
            || limits.outputs > 8
            || limits.sequences == 0
            || limits.sequences > 128
        {
            return Err(Error::Capacity);
        }
        Ok(Self {
            limits,
            work: StateWork::default(),
            inputs: (0..limits.inputs)
                .map(|_| State::new())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            outputs: (0..limits.outputs)
                .map(|_| State::new())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            bindings: vec![None; limits.sequences as usize].into_boxed_slice(),
            trackers: (0..usize::from(limits.sequences) * usize::from(limits.outputs))
                .map(|_| Tracker {
                    target: None,
                    bits: Bits::new(),
                    activity: Activity::default(),
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            directory: vec![0; limits.outputs as usize * 16].into_boxed_slice(),
            policies: vec![p::DEFAULT_CONTROLLERS; limits.outputs as usize].into_boxed_slice(),
            scratch: Some(State::new()),
        })
    }
    fn index(&self, sequence: u16, output: u16) -> usize {
        sequence as usize * self.limits.outputs as usize + output as usize
    }
    pub fn validate_owner(
        &self,
        sequence: u16,
        owner: crate::records::Identity,
    ) -> Result<(), Error> {
        if self
            .bindings
            .get(sequence as usize)
            .copied()
            .flatten()
            .is_some_and(|b| b.owner == owner)
        {
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }
    pub fn input(&self, input: u16) -> Result<&State, Error> {
        self.inputs.get(input as usize).ok_or(Error::InvalidState)
    }
    pub fn output(&self, output: u16) -> Result<&State, Error> {
        self.outputs.get(output as usize).ok_or(Error::InvalidState)
    }
    pub fn seed_input(&mut self, input: u16, state: &State) -> Result<(), Error> {
        self.inputs
            .get_mut(input as usize)
            .ok_or(Error::InvalidState)?
            .copy_from(state, u16::MAX);
        Ok(())
    }
    pub fn seed_output(
        &mut self,
        output: u16,
        state: &State,
        store: &ContentStore,
    ) -> Result<(), Error> {
        self.outputs
            .get_mut(output as usize)
            .ok_or(Error::InvalidState)?
            .copy_from(state, u16::MAX);
        for seq in 0..self.limits.sequences {
            self.refresh(seq, output, store)?;
        }
        Ok(())
    }
    pub fn policy(&mut self, output: u16, mask: u128, store: &ContentStore) -> Result<(), Error> {
        if (0..128).any(|cc| mask & (1u128 << cc) != 0 && !p::ordinary_controller(cc as u8)) {
            return Err(Error::Unsupported);
        }
        *self
            .policies
            .get_mut(output as usize)
            .ok_or(Error::InvalidState)? = mask | (1 << 64) | (1 << 66);
        for seq in 0..self.limits.sequences {
            self.refresh(seq, output, store)?;
        }
        Ok(())
    }
    pub fn bind(
        &mut self,
        sequence: u16,
        binding: StateBinding,
        view: Option<ViewId>,
        store: &ContentStore,
    ) -> Result<(), Error> {
        if sequence >= self.limits.sequences
            || binding.input >= self.limits.inputs
            || u16::from(binding.outputs) >> self.limits.outputs != 0
            || binding.channels == 0
        {
            return Err(Error::InvalidState);
        }
        self.bindings[sequence as usize] = Some(binding);
        for output in 0..self.limits.outputs {
            let index = self.index(sequence, output);
            self.trackers[index].target = if binding.outputs & (1 << output) != 0 {
                view
            } else {
                None
            };
            for ch in 0..16 {
                let directory = &mut self.directory[output as usize * 16 + ch];
                if view.is_some()
                    && binding.outputs & (1 << output) != 0
                    && binding.channels & (1 << ch) != 0
                {
                    *directory |= 1 << sequence;
                } else {
                    *directory &= !(1 << sequence);
                }
            }
            self.refresh(sequence, output, store)?;
        }
        Ok(())
    }
    pub fn rebind(
        &mut self,
        sequence: u16,
        view: Option<ViewId>,
        store: &ContentStore,
    ) -> Result<(), Error> {
        if let Some(binding) = self.bindings.get(sequence as usize).copied().flatten() {
            self.bind(sequence, binding, view, store)?;
        }
        Ok(())
    }
    pub fn capture(
        &self,
        sequence: u16,
        view: ViewId,
        store: &mut ContentStore,
    ) -> Result<(), Error> {
        let binding = self
            .bindings
            .get(sequence as usize)
            .copied()
            .flatten()
            .ok_or(Error::InvalidState)?;
        store.seed_snapshot(view, self.input(binding.input)?, binding.channels)
    }
    pub fn observe_input(&mut self, input: u16, message: Message) -> Result<(), Error> {
        self.inputs
            .get_mut(input as usize)
            .ok_or(Error::InvalidState)?
            .apply(message);
        Ok(())
    }
    fn managed(&self, output: u16, field: usize) -> bool {
        let f = field % p::STRIDE;
        f < 128
            || f == p::PITCH
            || f == p::PRESSURE
            || (128..256).contains(&f) && self.policies[output as usize] & (1 << (f - 128)) != 0
    }
    fn refresh(&mut self, sequence: u16, output: u16, store: &ContentStore) -> Result<(), Error> {
        let index = self.index(sequence, output);
        self.trackers[index].bits.clear();
        let Some(view) = self.trackers[index].target else {
            return Ok(());
        };
        let target = store.snapshot(view)?;
        let binding = self.bindings[sequence as usize].unwrap();
        for ch in 0..16 {
            if binding.channels & (1 << ch) == 0 {
                continue;
            }
            for f in 0..=p::PRESSURE {
                let field = ch * p::STRIDE + f;
                if self.managed(output, field) {
                    let dirty = !p::matches(
                        target.word(field),
                        self.outputs[output as usize].word(field),
                        field,
                    );
                    self.trackers[index].bits.set(field, dirty);
                }
            }
        }
        Ok(())
    }
    pub fn differences(&self, sequence: u16, output: u16) -> Result<&Bits, Error> {
        if sequence >= self.limits.sequences || output >= self.limits.outputs {
            return Err(Error::InvalidState);
        }
        Ok(&self.trackers[self.index(sequence, output)].bits)
    }
    pub fn accept(
        &mut self,
        output: u16,
        message: Message,
        owner: Option<u16>,
        store: &ContentStore,
    ) -> Result<(), Error> {
        self.accept_changes(output, message, owner, store)
            .map(|_| ())
    }
    fn accept_changes(
        &mut self,
        output: u16,
        message: Message,
        owner: Option<u16>,
        store: &ContentStore,
    ) -> Result<Bits, Error> {
        let changes = self
            .outputs
            .get_mut(output as usize)
            .ok_or(Error::InvalidState)?
            .apply(message);
        let mut at = 0;
        while let Some(field) = changes.next(at, p::FIELDS) {
            let channel = field / p::STRIDE;
            let current = self.outputs[output as usize].word(field);
            let mut watchers = self.directory[output as usize * 16 + channel];
            while watchers != 0 {
                let seq = watchers.trailing_zeros() as u16;
                watchers &= watchers - 1;
                let index = self.index(seq, output);
                if self.managed(output, field) {
                    let view = self.trackers[index].target.unwrap();
                    let target = store.snapshot(view)?.word(field);
                    let next = !p::matches(target, current, field);
                    self.work.tracker_visits += 1;
                    self.work.compared_fields += 1;
                    self.work.changed_bits +=
                        u64::from(self.trackers[index].bits.contains(field) != next);
                    self.trackers[index].bits.set(field, next);
                }
                if field % p::STRIDE < 128 && current & 127 == 0 {
                    self.trackers[index].activity.keys[channel] &= !(1u128 << (field % p::STRIDE));
                }
            }
            at = field + 1;
        }
        if let Some(sequence) = owner.filter(|seq| *seq < self.limits.sequences) {
            let index = self.index(sequence, output);
            let bytes = message.as_bytes();
            let ch = (bytes[0] & 15) as usize;
            if bytes[0] < 0xf0 && matches!(bytes[0] & 0xf0, 0x80 | 0x90) {
                let key = bytes[1] as usize;
                if self.outputs[output as usize].word(ch * p::STRIDE + key) & 127 != 0 {
                    self.trackers[index].activity.keys[ch] |= 1 << key;
                } else {
                    self.trackers[index].activity.keys[ch] &= !(1 << key);
                }
            }
            if bytes[0] & 0xf0 == 0xb0 && matches!(bytes[1], 64 | 66) {
                let bit = if bytes[1] == 64 { 1 } else { 2 };
                if bytes[2] >= 64 {
                    self.trackers[index].activity.pedals[ch] |= bit;
                } else {
                    self.trackers[index].activity.pedals[ch] &= !bit;
                }
            }
        }
        Ok(changes)
    }
    pub fn cleanup(
        &mut self,
        sequence: u16,
        offset: u32,
        store: &ContentStore,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let Some(binding) = self.bindings.get(sequence as usize).copied().flatten() else {
            return Ok(());
        };
        for output in 0..self.limits.outputs {
            if binding.outputs & (1 << output) == 0 {
                continue;
            }
            let index = self.index(sequence, output);
            let activity = self.trackers[index].activity;
            for ch in 0..16 {
                for (bit, cc) in [(1, 64), (2, 66)] {
                    if activity.pedals[ch] & bit != 0
                        && self.outputs[output as usize].word(ch * p::STRIDE + 128 + cc) & 127 >= 64
                    {
                        let message = Message::parse(&[0xb0 | ch as u8, cc as u8, 0])?;
                        sink(OutputEvent {
                            destination: output,
                            offset,
                            message,
                        })?;
                        self.accept(output, message, Some(sequence), store)?;
                    }
                }
                let mut keys = activity.keys[ch];
                while keys != 0 {
                    let key = keys.trailing_zeros() as usize;
                    keys &= keys - 1;
                    if self.outputs[output as usize].word(ch * p::STRIDE + key) & 127 != 0 {
                        let message = Message::parse(&[0x80 | ch as u8, key as u8, 0])?;
                        sink(OutputEvent {
                            destination: output,
                            offset,
                            message,
                        })?;
                        self.accept(output, message, Some(sequence), store)?;
                    }
                }
            }
            self.trackers[index].activity = Activity::default();
        }
        Ok(())
    }
    pub fn restore(
        &mut self,
        sequence: u16,
        view: ViewId,
        position: u64,
        offset: u32,
        store: &ContentStore,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let binding = self
            .bindings
            .get(sequence as usize)
            .copied()
            .flatten()
            .ok_or(Error::InvalidState)?;
        if position != 0 {
            let mut target = self.scratch.take().ok_or(Error::Busy)?;
            target.copy_from(store.snapshot(view)?, binding.channels);
            let result = (|| {
                for index in 0..store.lower_bound(view, position)? {
                    target.apply(store.read(view, index)?.unwrap().message);
                    self.work.prefix_events += 1;
                }
                self.restore_target(sequence, &target, offset, store, sink, false)
            })();
            self.scratch = Some(target);
            return result;
        }
        self.restore_target(sequence, store.snapshot(view)?, offset, store, sink, true)
    }
    fn restore_target(
        &mut self,
        sequence: u16,
        target: &State,
        offset: u32,
        store: &ContentStore,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
        persistent: bool,
    ) -> Result<(), Error> {
        let binding = self.bindings[sequence as usize].unwrap();
        for output in 0..self.limits.outputs {
            if binding.outputs & (1 << output) == 0 {
                continue;
            }
            let mut temporary = Bits::new();
            if !persistent {
                for ch in 0..16 {
                    if binding.channels & (1 << ch) == 0 {
                        continue;
                    }
                    for f in 0..=p::PRESSURE {
                        let field = ch * p::STRIDE + f;
                        if self.managed(output, field) {
                            temporary.set(
                                field,
                                !p::matches(
                                    target.word(field),
                                    self.outputs[output as usize].word(field),
                                    field,
                                ),
                            );
                        }
                    }
                }
            }
            let policy = self.policies[output as usize];
            let mut io = Emitter {
                engine: self,
                store,
                sink,
                sequence,
                output,
                offset,
                target,
                temporary: if persistent { None } else { Some(temporary) },
            };
            p::restore(target, binding.channels, policy, &mut io)?;
        }
        Ok(())
    }
}
struct Emitter<'a, F> {
    engine: &'a mut Expression,
    store: &'a ContentStore,
    sink: &'a mut F,
    sequence: u16,
    output: u16,
    offset: u32,
    target: &'a State,
    temporary: Option<Bits>,
}
impl<F: FnMut(OutputEvent) -> Result<(), Error>> RestoreIo for Emitter<'_, F> {
    fn current(&self, field: usize) -> u32 {
        self.engine.outputs[self.output as usize].word(field)
    }
    fn next_dirty(&self, start: usize, end: usize) -> Option<usize> {
        self.temporary
            .as_ref()
            .unwrap_or(&self.engine.trackers[self.engine.index(self.sequence, self.output)].bits)
            .next(start, end)
    }
    fn emit(&mut self, message: Message) -> Result<(), Error> {
        (self.sink)(OutputEvent {
            destination: self.output,
            offset: self.offset,
            message,
        })?;
        let changes =
            self.engine
                .accept_changes(self.output, message, Some(self.sequence), self.store)?;
        self.engine.work.restored_messages += 1;
        if let Some(bits) = &mut self.temporary {
            // Temporary seek comparisons are private to this restoration.
            let mut at = 0;
            while let Some(field) = changes.next(at, p::FIELDS) {
                if self.engine.managed(self.output, field) {
                    bits.set(
                        field,
                        !p::matches(
                            self.target.word(field),
                            self.engine.outputs[self.output as usize].word(field),
                            field,
                        ),
                    );
                }
                at = field + 1;
            }
        }
        Ok(())
    }
}
