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

#[test]
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
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
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
        permission_count, 34,
        "the explicit realtime lock permission baseline changed"
    );
}

#[test]
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
