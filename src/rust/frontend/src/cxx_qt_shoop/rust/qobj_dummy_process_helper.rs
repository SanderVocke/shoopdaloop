use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use cxx_qt_lib;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cxx_qt_shoop::qobj_dummy_process_helper_bridge::ffi::*;

impl DummyProcessHelper {
    pub fn start(mut self: Pin<&mut Self>) {
        let active = *self.as_mut().active();
        if active {
            panic!("Cannot start dummy process helper: still running");
        }
        self.as_mut().set_active(true);

        let wait_start = self.wait_start().clone();
        let wait_interval = self.wait_interval().clone();
        let n_iters = self.n_iters().clone();
        let samples_per_iter = self.samples_per_iter().clone();
        // let backend = self.backend.clone();

        thread::spawn(move || {
            thread::sleep(Duration::from_secs_f32(wait_start));
            for i in 0..n_iters {
                // FIXME: Implement the actual backend processing logic
                println!("Triggering backend process");
                // Simulate backend processing
                thread::sleep(Duration::from_secs_f32(wait_interval));
            }
            println!("Finished processing");
            // self.set_active(false);
        });
    }
}
