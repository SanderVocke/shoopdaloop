//! Prepared automatic monitoring demand from settled transport and program state.
use alloc::{boxed::Box, vec};
use shoop_loops::{LoopId, LoopKind, Mode, PendingTransition, ProgramKind, Resolver, Transition};

fn prepared<T: Copy>(slots: usize, initial: T) -> Box<[T]> {
    let mut values = vec![initial; slots];
    for value in &mut values {
        *value = core::hint::black_box(initial);
    }
    values.into_boxed_slice()
}

pub(crate) struct Demand {
    recording: Box<[bool]>,
    future: Box<[Option<u64>]>,
    queue: Box<[Option<LoopId>]>,
    head: usize,
    tail: usize,
    direct: Box<[Option<PendingTransition>]>,
}
impl Demand {
    pub(crate) fn new(loops: usize) -> Self {
        Self {
            recording: prepared(loops, false),
            future: prepared(loops, None),
            queue: prepared(loops, None),
            head: 0,
            tail: 0,
            direct: prepared(loops, None),
        }
    }
    pub(crate) fn recording(&self, slot: usize) -> bool {
        self.recording[slot]
    }
    fn intended(&self, id: LoopId, authored: Mode) -> Mode {
        match self.direct[id.slot() as usize].map(|pending| pending.request.transition) {
            Some(Transition::Stop) => Mode::Stopped,
            Some(Transition::Record { .. }) => Mode::Recording,
            Some(Transition::ReplaceInput { .. }) => Mode::Replacing,
            Some(Transition::PlayDry { .. }) => Mode::PlayingDryThroughWet,
            Some(Transition::Rerecord) => Mode::RecordingDryIntoWet,
            Some(Transition::Play { .. } | Transition::PlayIteration { .. }) => Mode::Playing,
            _ => authored,
        }
    }
    fn prepare_target(&mut self, resolver: &Resolver, id: LoopId, authored: Mode) {
        match self.intended(id, authored) {
            Mode::Recording | Mode::Replacing
                if resolver
                    .topology()
                    .node(id)
                    .is_ok_and(|node| node.kind == LoopKind::Primitive) =>
            {
                self.recording[id.slot() as usize] = true
            }
            Mode::Playing if resolver.topology().program(id).is_some() => {
                let iteration = match self.direct[id.slot() as usize]
                    .map(|pending| pending.request.transition)
                {
                    Some(
                        Transition::PlayIteration { iteration, .. }
                        | Transition::SeekIteration { iteration, .. },
                    ) => iteration,
                    _ => 0,
                };
                let slot = id.slot() as usize;
                if self.future[slot].is_none() {
                    self.future[slot] = Some(iteration);
                    self.queue[self.tail] = Some(id);
                    self.tail += 1;
                }
            }
            _ => {}
        }
    }
    pub(crate) fn update(&mut self, resolver: &Resolver) {
        self.recording.fill(false);
        self.future.fill(None);
        self.head = 0;
        self.tail = 0;
        self.direct.fill(None);
        for pending in resolver.pending_transitions() {
            let ready = match pending.remaining_ticks {
                Some(remaining) => remaining == 0 && pending.effective <= resolver.frame(),
                None => {
                    pending.effective.0
                        <= resolver
                            .frame()
                            .0
                            .saturating_add(resolver.quantum().frames() as u64)
                }
            };
            if !ready || resolver.topology().node(pending.request.target).is_err() {
                continue;
            }
            let slot = pending.request.target.slot() as usize;
            if self.direct[slot].is_none_or(|old| old.request.sequence < pending.request.sequence) {
                self.direct[slot] = Some(pending);
            }
        }
        for node in resolver.topology().nodes() {
            let Ok(state) = resolver.state(node.id) else {
                continue;
            };
            if node.kind == LoopKind::Primitive
                && matches!(state.mode, Mode::Recording | Mode::Replacing)
            {
                self.recording[node.id.slot() as usize] = true;
            }
            if let Some(pending) = self.direct[node.id.slot() as usize] {
                match pending.request.transition {
                    Transition::Record { .. } => {
                        self.prepare_target(resolver, node.id, Mode::Recording)
                    }
                    Transition::ReplaceInput { .. } => {
                        self.prepare_target(resolver, node.id, Mode::Replacing)
                    }
                    Transition::Play { .. } | Transition::PlayIteration { .. } => {
                        self.prepare_target(resolver, node.id, Mode::Playing)
                    }
                    Transition::SeekIteration { .. } if state.mode == Mode::Playing => {
                        self.prepare_target(resolver, node.id, Mode::Playing)
                    }
                    _ => {}
                }
            }
            let Some(program) = resolver.topology().program(node.id) else {
                continue;
            };
            let Some(cursor) = state
                .composite
                .filter(|cursor| state.mode == Mode::Playing && !cursor.waiting_repeat)
            else {
                continue;
            };
            if self.intended(node.id, Mode::Playing) != Mode::Playing {
                continue;
            }
            let next = if cursor.iteration.saturating_add(1) < program.iterations {
                cursor.iteration + 1
            } else if program.kind == ProgramKind::Regular && program.iterations > 0 {
                0
            } else {
                continue;
            };
            for target in program.targets() {
                if let Some(segment) = target.at(next).filter(|segment| segment.start == next) {
                    let nested_entry = next == 0
                        && segment.mode == Mode::Playing
                        && cursor
                            .entered_at
                            .and_then(|entered| entered.0.checked_add(program.nominal_unit_frames))
                            .is_some_and(|frame| {
                                resolver
                                    .program_needs_entry_at(
                                        target.target,
                                        shoop_loops::Frame(frame),
                                    )
                                    .unwrap_or(false)
                            });
                    if target
                        .at(cursor.iteration)
                        .is_some_and(|old| old.mode == segment.mode)
                        && !nested_entry
                    {
                        continue;
                    }
                    self.prepare_target(resolver, target.target, segment.mode);
                }
            }
        }
        while self.head < self.tail {
            let id = self.queue[self.head].expect("queued composite");
            self.head += 1;
            let iteration = self.future[id.slot() as usize].expect("queued iteration");
            if let Some(program) = resolver.topology().program(id) {
                for target in program.targets() {
                    if let Some(segment) = target.at(iteration) {
                        self.prepare_target(resolver, target.target, segment.mode);
                    }
                }
            }
        }
    }
}
