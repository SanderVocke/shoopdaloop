use crate::realtime_lock_guard::{Mutex, MutexGuard};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const DSP: u32 = 1;
const CONTROL: u32 = 2;

/// Control owners wait for a block boundary; DSP only attempts atomic admission.
#[derive(Debug, Default)]
pub(crate) struct ExecutionGate {
    state: AtomicU32,
    control_owner: Mutex<()>,
}

impl ExecutionGate {
    pub fn try_dsp(&self) -> Option<DspGuard<'_>> {
        self.state
            .compare_exchange(0, DSP, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| DspGuard(self))
    }

    pub fn control(&self) -> ControlGuard<'_> {
        let owner = self
            .control_owner
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.state.fetch_or(CONTROL, Ordering::AcqRel);
        while self.state.load(Ordering::Acquire) & DSP != 0 {
            std::thread::sleep(Duration::from_micros(50));
        }
        ControlGuard {
            gate: self,
            _owner: owner,
        }
    }
}

pub(crate) struct DspGuard<'a>(&'a ExecutionGate);

impl Drop for DspGuard<'_> {
    fn drop(&mut self) {
        self.0.state.fetch_and(!DSP, Ordering::Release);
    }
}

pub(crate) struct ControlGuard<'a> {
    gate: &'a ExecutionGate,
    _owner: MutexGuard<'a, ()>,
}

impl Drop for ControlGuard<'_> {
    fn drop(&mut self) {
        self.gate.state.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc};
    use std::time::Instant;

    #[shoop_wasm_test_support::shoop_test]
    fn control_waits_for_dsp_then_dsp_fails_fast_until_control_finishes() {
        let gate = Arc::new(ExecutionGate::default());
        let dsp = gate.try_dsp().unwrap();
        let (entered, receive) = mpsc::sync_channel(1);
        let (release, finish) = mpsc::sync_channel(1);
        let worker_gate = Arc::clone(&gate);
        let worker = std::thread::spawn(move || {
            let _control = worker_gate.control();
            entered.send(()).unwrap();
            finish.recv().unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while gate.state.load(Ordering::Acquire) & CONTROL == 0 {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(receive.try_recv().is_err());
        drop(dsp);
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_no_alloc::assert_no_alloc(|| {
            for _ in 0..1000 {
                assert!(gate.try_dsp().is_none());
            }
        });
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(gate.try_dsp().is_some());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn panicking_control_owner_does_not_leave_dsp_quiesced() {
        let gate = ExecutionGate::default();
        let result = std::panic::catch_unwind(|| {
            let _control = gate.control();
            panic!("control failure");
        });
        assert!(result.is_err());
        assert!(gate.try_dsp().is_some());
        drop(gate.control());
        assert!(gate.try_dsp().is_some());
    }
}
