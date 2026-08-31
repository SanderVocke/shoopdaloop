#![cfg(not(feature = "prebuild"))]

pub mod logging;
#[cfg(not(target_arch = "wasm32"))]
pub mod tracing_capture;
pub mod tracing_helpers;
use anyhow::Context;

pub fn init() -> Result<(), anyhow::Error> {
    logging::init_logging().with_context(|| "Failed to initialize logging")?;
    tracing::debug!(target: "app.common", "common runtime initialized");
    Ok(())
}

pub fn shoop_version() -> &'static str {
    env!("SHOOP_VERSION")
}

pub fn shoop_description() -> &'static str {
    env!("SHOOP_DESCRIPTION")
}
