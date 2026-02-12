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
        if let Some(lazy) = ptr.as_mut() {
            let threads: &mut Mutex<ThreadsMap> = Lazy::force_mut(lazy);
            if let Ok(mut guard) = threads.lock() {
                guard.insert(id, name.to_string());
            }
        }
    }
}

pub fn current_thread_registered_name() -> Option<String> {
    let id = std::thread::current().id();
    let result: Option<String>;
    unsafe {
        let ptr: *mut once_cell::sync::Lazy<Mutex<ThreadsMap>> = &raw mut REGISTERED_THREADS;
        if let Some(lazy) = ptr.as_mut() {
            let threads: &mut Mutex<ThreadsMap> = Lazy::force_mut(lazy);
            if let Ok(guard) = threads.lock() {
                result = guard.get(&id).map(|s| s.clone());
            } else {
                result = None;
            }
        } else {
            result = None;
        }
    }
    result
}
