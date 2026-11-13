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
    let setting = |name: &str, default: u32| {
        println!("cargo:rerun-if-env-changed={name}");
        env::var(name).map_or(default, |value| {
            value.parse::<u32>().expect("engine setting must be a u32")
        })
    };
    let tracks = setting("SHOOP_ENGINE_TRACKS", 8);
    let loops = setting("SHOOP_ENGINE_LOOPS", 32);
    assert!((1..=65535).contains(&tracks) && (1..=65535).contains(&loops));
    let seconds = setting("SHOOP_INPUT_HISTORY_SECONDS", 0);
    let audio_take_pages = setting("SHOOP_INPUT_HISTORY_AUDIO_TAKE_PAGES", 32768);
    let midi_pages = setting("SHOOP_INPUT_HISTORY_MIDI_PAGES", 4096);
    let midi_records = setting("SHOOP_INPUT_HISTORY_MIDI_RECORDS", 65536);
    let writers = setting("SHOOP_INPUT_HISTORY_WRITERS", 8);
    assert!(
        seconds == 0
            || [audio_take_pages, midi_pages, midi_records, writers]
                .iter()
                .all(|&n| n > 0)
    );
    let address = |name: &str| {
        println!("cargo:rerun-if-env-changed={name}");
        env::var(name).map_or(0, |value| {
            value
                .parse::<u32>()
                .expect("FPGA arena values must be u32 byte counts")
        })
    };
    let arena_base = address("SHOOP_FPGA_ARENA_BASE");
    let arena_bytes = address("SHOOP_FPGA_ARENA_BYTES");
    assert!(arena_base.is_multiple_of(1024 * 1024) && arena_bytes.is_multiple_of(1024 * 1024));
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("memory.rs");
    fs::write(
        output,
        format!(
            "pub const FPGA_ARENA_BASE: u32 = {arena_base};\npub const FPGA_ARENA_BYTES: u32 = {arena_bytes};\npub const RECORDING_BYTES: usize = {recording};\npub const MEDIA_BYTES: u64 = {media};\npub const ENGINE_TRACKS: u32 = {tracks};\npub const ENGINE_LOOPS: u32 = {loops};\npub const INPUT_HISTORY: shoop_rt_protocol::host::InputHistorySettings = shoop_rt_protocol::host::InputHistorySettings {{ seconds: {seconds}, audio_take_pages: {audio_take_pages}, midi_pages: {midi_pages}, midi_records: {midi_records}, writers: {writers} }};\n"
        ),
    )
    .unwrap();
}
