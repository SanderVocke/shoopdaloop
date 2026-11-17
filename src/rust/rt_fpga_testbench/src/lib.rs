#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]

pub use rt_fpga_rtl::bits;
pub use rt_fpga_rtl::clock;
pub mod codec;
pub use rt_fpga_rtl::compiler;
pub use rt_fpga_rtl::component;
pub mod drivers;
pub mod loop_image;
pub mod memory;
pub use rt_fpga_rtl::model;
pub mod protocol;
pub mod random;
pub mod reference;
pub use rt_fpga_rtl::report;
pub use rt_fpga_rtl::sim;
pub mod suites;
pub mod timing;
pub use rt_fpga_rtl::toolchain;

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
