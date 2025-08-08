use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread::ThreadId;

type ThreadsMap = HashMap<ThreadId, String>;

static mut REGISTERED_THREADS: Lazy<Mutex<ThreadsMap>> =
    Lazy::new(|| Mutex::new(ThreadsMap::new()));

pub fn register_thread(name: &str) {
    let id = std::thread::current().id();
    unsafe {
        let ptr: *mut once_cell::sync::Lazy<Mutex<ThreadsMap>> = &raw mut REGISTERED_THREADS;
        let threads: &mut Mutex<ThreadsMap> = Lazy::force_mut(ptr.as_mut().unwrap());
        let mut guard = threads.lock();
        guard
            .as_mut()
            .expect("Could not lock threads map")
            .insert(id, name.to_string());
    }
}

pub fn current_thread_registered_name() -> Option<String> {
    let id = std::thread::current().id();
    let result: Option<String>;
    unsafe {
        let ptr: *mut once_cell::sync::Lazy<Mutex<ThreadsMap>> = &raw mut REGISTERED_THREADS;
        let threads: &mut Mutex<ThreadsMap> = Lazy::force_mut(ptr.as_mut().unwrap());
        let mut guard = threads.lock();
        result = guard
            .as_mut()
            .expect("Could not lock threads map")
            .get(&id)
            .map(|s| s.clone());
    }
    result
}
