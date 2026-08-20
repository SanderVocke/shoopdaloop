#![cfg(not(target_arch = "wasm32"))]

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources(directory: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            rust_sources(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}

#[shoop_wasm_test_support::shoop_test]
fn production_engine_mutexes_use_the_checked_abstraction() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let checked_mutex = source_root.join("realtime_lock_guard.rs");
    let direct_import = Regex::new(r"usestd::sync::\{[^}]*Mutex").unwrap();
    let mut sources = Vec::new();
    let mut permission_count = 0;
    rust_sources(&source_root, &mut sources);

    for path in sources {
        if path == checked_mutex {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read Rust source");
        let production = source
            .split("\n#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or(&source);
        permission_count += production.matches("realtime_allow_lock!").count();
        let compact: String = production
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert!(
            !compact.contains("std::sync::Mutex") && !direct_import.is_match(&compact),
            "{} bypasses the checked mutex abstraction",
            path.display()
        );
    }
    assert_eq!(
        permission_count, 28,
        "the explicit realtime lock permission baseline changed"
    );
}

#[shoop_wasm_test_support::shoop_test]
fn processor_callback_owns_lock_free_endpoints() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let session = fs::read_to_string(source_root.join("session.rs")).expect("read session source");
    let callback = session
        .split("fn process_processor")
        .nth(1)
        .and_then(|tail| tail.split("fn synth_prerecorded_midi_playback").next())
        .expect("processor callback body");
    assert!(!callback.contains(".lock("));
    assert!(!callback.contains("Arc::clone"));
    assert!(!callback.contains("format!("));
    assert!(callback.contains("std::mem::take(&mut self.processors)"));
    assert!(callback.contains("ProcessorBackend::Carla"));

    let processor = fs::read_to_string(source_root.join("carla_processor.rs"))
        .expect("read Carla processor source");
    let realtime = processor
        .split("impl CarlaProcessor for CarlaRealtimeProcessor")
        .nth(1)
        .and_then(|tail| tail.split("fn process_bridge_block").next())
        .expect("realtime endpoint implementation");
    assert!(!realtime.contains(".lock("));
    assert!(!realtime.contains("Tcp"));
    assert!(!realtime.contains("Command::"));
}

#[shoop_wasm_test_support::shoop_test]
fn engine_and_driver_realtime_boundaries_are_marked() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let engine = fs::read_to_string(source_root.join("engine.rs")).expect("read engine source");
    let backend =
        fs::read_to_string(source_root.join("app_backend.rs")).expect("read backend source");

    assert!(engine.matches("forbid_locks_if_enabled").count() >= 3);
    assert!(backend.matches("forbid_locks_if_enabled").count() >= 4);
    for permission in [
        "JACK registered port registry",
        "CPAL capture ring input",
        "CPAL external connection registry",
        "dummy driver iteration state",
        "object creation failure publication",
    ] {
        assert!(
            backend.contains(permission),
            "missing explicit realtime lock permission: {permission}"
        );
    }
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
