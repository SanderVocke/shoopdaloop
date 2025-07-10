pub mod env;
pub mod logging;
pub mod util;
use anyhow;
use anyhow::Context;

pub fn init() -> Result<(), anyhow::Error> {
    logging::init_logging().with_context(|| "Failed to initialize logging")?;
    Ok(())
}

pub fn shoop_version() -> &'static str {
    env!("SHOOP_VERSION")
}

pub fn shoop_description() -> &'static str {
    env!("SHOOP_DESCRIPTION")
}
