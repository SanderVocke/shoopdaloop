#![cfg(not(target_arch = "wasm32"))]

mod args;
mod host_manager;

pub use args::ManagerArgs;
pub use host_manager::{run, verify_host_discovery};

fn application_icon() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../resources/iconset/icon.png"
    )))
    .expect("embedded application icon must be valid PNG")
}
