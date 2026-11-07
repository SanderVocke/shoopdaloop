use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
pub(crate) struct CallbackGate {
    closed: AtomicBool,
    active: AtomicUsize,
}

pub(crate) struct Turn<'a>(&'a CallbackGate);
impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}
impl CallbackGate {
    pub fn enter(&self) -> Option<Turn<'_>> {
        self.active.fetch_add(1, Ordering::SeqCst);
        if self.closed.load(Ordering::SeqCst) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(Turn(self))
        }
    }
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
    pub fn wait(&self) {
        while self.active.load(Ordering::SeqCst) != 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    use std::sync::{mpsc, Arc};

    #[shoop_test(no_wasm = "native callback shutdown coordination uses threads")]
    fn closing_refuses_new_turns_and_waits_for_both_callbacks_to_leave() {
        let gate = Arc::new(CallbackGate::default());
        let input = gate.enter().unwrap();
        let output = gate.enter().unwrap();
        gate.close();
        assert!(gate.enter().is_none());
        let (started, ready) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        let closing = gate.clone();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            closing.wait();
            finished.send(()).unwrap();
        });
        ready.recv().unwrap();
        assert!(done.try_recv().is_err());
        drop(input);
        std::thread::yield_now();
        assert!(done.try_recv().is_err());
        drop(output);
        done.recv().unwrap();
        worker.join().unwrap();
        assert!(gate.enter().is_none());
    }
}
