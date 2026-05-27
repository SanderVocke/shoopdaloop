use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::audio_midi_driver::AudioMidiDriverCore;

struct DriverRegistry {
    next_id: AtomicU64,
    map: Mutex<HashMap<u64, Box<AudioMidiDriverCore>>>,
}

impl DriverRegistry {
    fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            map: Mutex::new(HashMap::new()),
        }
    }

    fn create_driver(&self, driver_kind: u32) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let core = {
            let core = AudioMidiDriverCore::new();
            core.set_driver_kind(driver_kind);
            Box::new(core)
        };
        self.map.lock().expect("driver registry poisoned").insert(id, core);
        id
    }

    fn destroy_driver(&self, id: u64) {
        self.map.lock().expect("driver registry poisoned").remove(&id);
    }

    fn get_driver_kind(&self, id: u64) -> u32 {
        self.map
            .lock()
            .expect("driver registry poisoned")
            .get(&id)
            .map(|d| d.get_driver_kind())
            .unwrap_or(2)
    }

    fn get_sample_rate(&self, id: u64) -> u32 {
        self.map
            .lock()
            .expect("driver registry poisoned")
            .get(&id)
            .map(|d| d.get_sample_rate())
            .unwrap_or(0)
    }

    fn get_buffer_size(&self, id: u64) -> u32 {
        self.map
            .lock()
            .expect("driver registry poisoned")
            .get(&id)
            .map(|d| d.get_buffer_size())
            .unwrap_or(0)
    }

    fn get_active(&self, id: u64) -> bool {
        self.map
            .lock()
            .expect("driver registry poisoned")
            .get(&id)
            .map(|d| d.get_active())
            .unwrap_or(false)
    }
}

fn registry() -> &'static DriverRegistry {
    static REGISTRY: OnceLock<DriverRegistry> = OnceLock::new();
    REGISTRY.get_or_init(DriverRegistry::new)
}

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        fn rt_create_driver(driver_kind: u32) -> u64;
        fn rt_destroy_driver(id: u64);
        fn rt_get_driver_kind(id: u64) -> u32;
        fn rt_get_sample_rate(id: u64) -> u32;
        fn rt_get_buffer_size(id: u64) -> u32;
        fn rt_get_active(id: u64) -> bool;
    }
}

fn rt_create_driver(driver_kind: u32) -> u64 {
    registry().create_driver(driver_kind)
}

fn rt_destroy_driver(id: u64) {
    registry().destroy_driver(id)
}

fn rt_get_driver_kind(id: u64) -> u32 {
    registry().get_driver_kind(id)
}

fn rt_get_sample_rate(id: u64) -> u32 {
    registry().get_sample_rate(id)
}

fn rt_get_buffer_size(id: u64) -> u32 {
    registry().get_buffer_size(id)
}

fn rt_get_active(id: u64) -> bool {
    registry().get_active(id)
}
