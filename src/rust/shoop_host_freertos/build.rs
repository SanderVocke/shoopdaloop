use std::{env, fs, path::PathBuf};

fn budget(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name).map_or(default, |value| {
        let bytes: usize = value.parse().expect("memory budget must be an integer");
        assert!(bytes > 0 && bytes.is_multiple_of(1024 * 1024));
        bytes
    })
}

fn main() {
    let recording = budget("SHOOP_RECORDING_BYTES", 128 * 1024 * 1024);
    let media = budget("SHOOP_MEDIA_BYTES", 64 * 1024 * 1024);
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("memory.rs");
    fs::write(
        output,
        format!(
            "pub const RECORDING_BYTES: usize = {recording};\npub const MEDIA_BYTES: u64 = {media};\n"
        ),
    )
    .unwrap();
}
