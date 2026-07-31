use crate::loop_mode::LoopMode;
use crate::state::LoopState;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

const NO_MODE: i32 = -1;
const NO_DELAY: u64 = u64::MAX;

#[derive(Debug)]
pub struct LoopStateMirror {
    mode: AtomicI32,
    length: AtomicU32,
    position: AtomicU32,
    next_mode: AtomicI32,
    next_delay: AtomicU64,
}

impl Default for LoopStateMirror {
    fn default() -> Self {
        Self {
            mode: AtomicI32::new(LoopMode::Stopped as i32),
            length: AtomicU32::new(0),
            position: AtomicU32::new(0),
            next_mode: AtomicI32::new(NO_MODE),
            next_delay: AtomicU64::new(NO_DELAY),
        }
    }
}

impl LoopStateMirror {
    pub fn publish(
        &self,
        mode: LoopMode,
        length: u32,
        position: u32,
        next: Option<(LoopMode, u32)>,
    ) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.length.store(length, Ordering::Relaxed);
        self.position.store(position, Ordering::Relaxed);
        self.next_mode.store(
            next.map(|(mode, _)| mode as i32).unwrap_or(NO_MODE),
            Ordering::Relaxed,
        );
        self.next_delay.store(
            next.map(|(_, delay)| delay as u64).unwrap_or(NO_DELAY),
            Ordering::Relaxed,
        );
    }

    pub fn read(&self) -> LoopState {
        let next_mode = self.next_mode.load(Ordering::Relaxed);
        let next_delay = self.next_delay.load(Ordering::Relaxed);
        LoopState {
            mode: LoopMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(LoopMode::Unknown),
            length: self.length.load(Ordering::Relaxed),
            position: self.position.load(Ordering::Relaxed),
            maybe_next_mode: (next_mode != NO_MODE)
                .then(|| LoopMode::try_from(next_mode).unwrap_or(LoopMode::Unknown)),
            maybe_next_mode_delay: (next_delay != NO_DELAY).then_some(next_delay as u32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[test]
    fn loop_state_fields_are_independently_published() {
        let mirror = LoopStateMirror::default();
        check!(mirror.read().mode == LoopMode::Stopped);

        mirror.publish(LoopMode::Playing, 128, 17, Some((LoopMode::Recording, 2)));
        let state = mirror.read();
        check!(state.mode == LoopMode::Playing);
        check!(state.length == 128);
        check!(state.position == 17);
        check!(state.maybe_next_mode == Some(LoopMode::Recording));
        check!(state.maybe_next_mode_delay == Some(2));

        mirror.publish(LoopMode::Stopped, 0, 0, None);
        let state = mirror.read();
        check!(state.maybe_next_mode.is_none());
        check!(state.maybe_next_mode_delay.is_none());
    }
}
