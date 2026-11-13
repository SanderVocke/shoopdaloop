#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]

pub mod bits;
pub mod clock;
pub mod codec;
pub mod command_measurements;
pub mod drivers;
pub mod images;
pub mod memory;
pub mod model;
pub mod prepared;
pub mod protocol;
pub mod random;
pub mod reference;
pub mod report;
pub mod sim;
pub mod suites;
pub mod timing;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
pub fn artifact_directory(component: &str) -> std::path::PathBuf {
    report::directory(component)
}
