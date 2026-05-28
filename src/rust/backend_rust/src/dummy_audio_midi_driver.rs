use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct DummyAudioMidiDriverState {
    finish: AtomicBool,
    mode: AtomicU32,
    controlled_samples: AtomicU32,
    paused: AtomicBool,
}

pub struct DummyAudioMidiDriver {
    state: Arc<DummyAudioMidiDriverState>,
    process_thread: Mutex<Option<JoinHandle<()>>>,
}

impl DummyAudioMidiDriver {
    pub fn new() -> Self {
        Self {
            state: Arc::new(DummyAudioMidiDriverState {
                finish: AtomicBool::new(false),
                mode: AtomicU32::new(1),
                controlled_samples: AtomicU32::new(0),
                paused: AtomicBool::new(false),
            }),
            process_thread: Mutex::new(None),
        }
    }

    pub fn is_finish(&self) -> bool { self.state.finish.load(Ordering::SeqCst) }
    pub fn set_finish(&self) { self.state.finish.store(true, Ordering::SeqCst); }
    pub fn enter_mode(&self, mode: u32) { self.state.mode.store(mode, Ordering::SeqCst); self.state.controlled_samples.store(0, Ordering::SeqCst); }
    pub fn get_mode(&self) -> u32 { self.state.mode.load(Ordering::SeqCst) }
    pub fn pause(&self) { self.state.paused.store(true, Ordering::SeqCst); }
    pub fn resume(&self) { self.state.paused.store(false, Ordering::SeqCst); }
    pub fn is_paused(&self) -> bool { self.state.paused.load(Ordering::SeqCst) }
    pub fn controlled_mode_request_samples(&self, samples: u32) { self.state.controlled_samples.fetch_add(samples, Ordering::SeqCst); }
    pub fn get_controlled_mode_samples_to_process(&self) -> u32 { self.state.controlled_samples.load(Ordering::SeqCst) }
    pub fn controlled_mode_advance(&self, samples: u32) {
        let prev = self.state.controlled_samples.fetch_sub(samples, Ordering::SeqCst);
        if prev < samples { self.state.controlled_samples.store(0, Ordering::SeqCst); }
    }

    pub fn start_process_thread(&self, owner_ptr: usize, sample_rate: u32, buffer_size: u32) {
        let mut guard = self.process_thread.lock().unwrap();
        if guard.is_some() { return; }
        self.state.finish.store(false, Ordering::SeqCst);
        let state = Arc::clone(&self.state);
        *guard = Some(thread::spawn(move || {
            let bufs_per_second = (sample_rate / buffer_size).max(1);
            let micros = (1_000_000.0f32 / bufs_per_second as f32) as u64;
            let mut time_taken = 0u64;
            while !state.finish.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_micros(micros.saturating_sub(time_taken)));
                let start = Instant::now();
                unsafe { crate::dummy_audio_midi_driver_cxx::ffi::dummy_audiomididriver_exec_commands(owner_ptr); }
                if !state.paused.load(Ordering::SeqCst) {
                    let mode = state.mode.load(Ordering::SeqCst);
                    let samples = state.controlled_samples.load(Ordering::SeqCst);
                    let to_process = if mode == 0 { samples.min(buffer_size) } else { buffer_size };
                    unsafe { crate::dummy_audio_midi_driver_cxx::ffi::dummy_audiomididriver_process(owner_ptr, to_process); }
                    if mode == 0 {
                        let prev = state.controlled_samples.fetch_sub(to_process, Ordering::SeqCst);
                        if prev < to_process { state.controlled_samples.store(0, Ordering::SeqCst); }
                    }
                }
                time_taken = start.elapsed().as_micros() as u64;
            }
        }));
    }

    pub fn stop_process_thread(&self) {
        self.set_finish();
        let mut guard = self.process_thread.lock().unwrap();
        if let Some(handle) = guard.take() {
            if handle.thread().id() == thread::current().id() {
                drop(handle);
            } else {
                let _ = handle.join();
            }
        }
    }
}
