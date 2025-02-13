use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use cxx_qt::QVariant;
use cxx_qt::QString;
use cxx_qt::QQuickItem;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct DummyProcessHelperRust {
    wait_start: f32,
    wait_interval: f32,
    n_iters: i32,
    samples_per_iter: i32,
    backend: QVariant,
    active: Arc<AtomicBool>,
}

impl Default for DummyProcessHelperRust {
    fn default() -> DummyProcessHelperRust {
        DummyProcessHelperRust {
            wait_start: 0.0,
            wait_interval: 0.0,
            n_iters: 0,
            samples_per_iter: 0,
            backend: QVariant::default(),
            active: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DummyProcessHelperRust {
    pub fn start(self: Pin<&mut Self>) {
        let active = self.active.clone();
        if active.load(Ordering::SeqCst) {
            panic!("Cannot start dummy process helper: still running");
        }

        active.store(true, Ordering::SeqCst);
        let wait_start = self.wait_start;
        let wait_interval = self.wait_interval;
        let n_iters = self.n_iters;
        let samples_per_iter = self.samples_per_iter;
        let backend = self.backend.clone();

        thread::spawn(move || {
            thread::sleep(Duration::from_secs_f32(wait_start));
            for i in 0..n_iters {
                // FIXME: Implement the actual backend processing logic
                println!("Triggering backend process");
                // Simulate backend processing
                thread::sleep(Duration::from_secs_f32(wait_interval));
            }
            println!("Finished processing");
            active.store(false, Ordering::SeqCst);
        });
    }
}
